use super::{types::*, Connection};

/// The type of transaction to create.
pub enum TransactionType {
    /// The transaction does not actually start until the database is first accessed. If the first
    /// statement in the transaction is a SELECT, then a read transaction is started. Subsequent
    /// write statements will upgrade the transaction to a write transaction if possible, or return
    /// SQLITE_BUSY. If the first statement in the transaction is a write statement, then a write
    /// transaction is started.
    Deferred,
    /// The database connection starts a new write immediately, without waiting for a write
    /// statement. The transaction method might fail with SQLITE_BUSY if another write transaction
    /// is already active on another database connection.
    Immediate,
    /// Exclusive is similar to Immediate in that a write transaction is started immediately.
    /// Exclusive and Immediate are the same in WAL mode, but in other journaling modes, Exclusive
    /// prevents other database connections from reading the database while the transaction is
    /// underway.
    Exclusive,
}

#[derive(Debug, PartialEq, Eq)]
enum TransactionState {
    ActiveTransaction,
    ActiveSavepoint,
    Inactive,
}

/// A RAII wrapper around transactions.
///
/// The transaction can be created using [Connection::transaction], and finalized using
/// [commit](Transaction::commit) or [rollback](Transaction::rollback). If no finalization method
/// is used, the transaction will automatically be rolled back when it is dropped. For convenience,
/// Transaction derefs to Connection.
///
/// Note that transactions affect the entire database connection. If other threads need to be
/// prevented from interfering with a transaction, [Connection::lock] should be used.
///
/// This interface is optional. It's permitted to execute BEGIN, COMMIT, and ROLLBACK statements
/// directly, without using this interface. This interface
#[derive(Debug)]
pub struct Transaction<'db> {
    db: &'db Connection,
    state: TransactionState,
}

impl Connection {
    /// Starts a new transaction with the specified behavior.
    pub fn transaction(&self, tt: TransactionType) -> Result<Transaction<'_>> {
        let mut txn = Transaction {
            db: self,
            state: TransactionState::Inactive,
        };
        txn.start(tt)?;
        Ok(txn)
    }
}

impl<'db> Transaction<'db> {
    fn start(&mut self, tt: TransactionType) -> Result<()> {
        let sql = match tt {
            TransactionType::Deferred => "BEGIN",
            TransactionType::Immediate => "BEGIN IMMEDIATE",
            TransactionType::Exclusive => "BEGIN EXCLUSIVE",
        };
        self.execute(sql, ())?;
        self.state = TransactionState::ActiveTransaction;
        Ok(())
    }

    /// Consumes the Transaction, committing it.
    pub fn commit(mut self) -> Result<&'db Connection> {
        self.commit_mut().map(|_| self.db)
    }

    fn commit_mut(&mut self) -> Result<()> {
        let ret = match self.state {
            TransactionState::ActiveTransaction => self.execute("COMMIT", ()),
            TransactionState::ActiveSavepoint => self.execute("RELEASE SAVEPOINT a", ()),
            TransactionState::Inactive => panic!("lifetime error"),
        };
        self.state = TransactionState::Inactive;
        ret.map(|_| ())
    }

    /// Consumes the Transaction, rolling it back.
    pub fn rollback(mut self) -> Result<&'db Connection> {
        self.rollback_mut().map(|_| self.db)
    }

    fn rollback_mut(&mut self) -> Result<()> {
        let ret = match self.state {
            TransactionState::ActiveTransaction => self.execute("ROLLBACK", ()),
            TransactionState::ActiveSavepoint => self.execute("ROLLBACK TO a", ()),
            TransactionState::Inactive => panic!("lifetime error"),
        };
        self.state = TransactionState::Inactive;
        ret.map(|_| ())
    }

    /// Create a savepoint for the current transaction. This functions identically to a
    /// transaction, but committing or rolling back will only affect statements since the savepoint
    /// was created.
    pub fn savepoint(&mut self) -> Result<Transaction<'_>> {
        self.execute("SAVEPOINT a", ())?;
        let txn = Self {
            db: self.db,
            state: TransactionState::ActiveSavepoint,
        };
        Ok(txn)
    }
}

impl std::ops::Deref for Transaction<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.db
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if self.state != TransactionState::Inactive {
            if let Err(e) = self.rollback_mut() {
                if std::thread::panicking() {
                    eprintln!("Error while closing SQLite transaction: {:?}", e);
                } else {
                    panic!("Error while closing SQLite transaction: {:?}", e);
                }
            }
        }
    }
}

#[cfg(all(test, feature = "static"))]
mod test {
    use crate::test_helpers::prelude::*;

    #[test]
    fn commit() -> Result<()> {
        let h = TestHelpers::new();
        h.db.execute("CREATE TABLE tbl(col)", ())?;
        let txn = h.db.transaction(TransactionType::Deferred)?;
        txn.execute("INSERT INTO tbl VALUES (1)", ())?;
        txn.commit()?;
        let count =
            h.db.query_row("SELECT COUNT(*) FROM tbl", (), |r| Ok(r[0].get_i64()))?;
        assert_eq!(count, 1);
        Ok(())
    }

    #[test]
    fn rollback() -> Result<()> {
        let h = TestHelpers::new();
        h.db.execute("CREATE TABLE tbl(col)", ())?;
        let txn = h.db.transaction(TransactionType::Deferred)?;
        txn.execute("INSERT INTO tbl VALUES (1)", ())?;
        txn.rollback()?;
        let count =
            h.db.query_row("SELECT COUNT(*) FROM tbl", (), |r| Ok(r[0].get_i64()))?;
        assert_eq!(count, 0);
        Ok(())
    }

    #[test]
    fn drop() -> Result<()> {
        let h = TestHelpers::new();
        h.db.execute("CREATE TABLE tbl(col)", ())?;
        {
            let txn = h.db.transaction(TransactionType::Deferred)?;
            txn.execute("INSERT INTO tbl VALUES (1)", ())?;
        }
        let count =
            h.db.query_row("SELECT COUNT(*) FROM tbl", (), |r| Ok(r[0].get_i64()))?;
        assert_eq!(count, 0);
        Ok(())
    }

    #[test]
    fn savepoint_commit() -> Result<()> {
        let h = TestHelpers::new();
        h.db.execute("CREATE TABLE tbl(col)", ())?;
        let mut txn = h.db.transaction(TransactionType::Deferred)?;
        txn.execute("INSERT INTO tbl VALUES (1)", ())?;
        let sp = txn.savepoint()?;
        sp.execute("INSERT INTO tbl VALUES (2)", ())?;
        sp.commit()?;
        txn.commit()?;
        let count =
            h.db.query_row("SELECT COUNT(*) FROM tbl", (), |r| Ok(r[0].get_i64()))?;
        assert_eq!(count, 2);
        Ok(())
    }

    #[test]
    fn savepoint_rollback() -> Result<()> {
        let h = TestHelpers::new();
        h.db.execute("CREATE TABLE tbl(col)", ())?;
        let mut txn = h.db.transaction(TransactionType::Deferred)?;
        txn.execute("INSERT INTO tbl VALUES (1)", ())?;
        let sp = txn.savepoint()?;
        sp.execute("INSERT INTO tbl VALUES (2)", ())?;
        sp.rollback()?;
        txn.commit()?;
        let count =
            h.db.query_row("SELECT COUNT(*) FROM tbl", (), |r| Ok(r[0].get_i64()))?;
        assert_eq!(count, 1);
        Ok(())
    }

    #[test]
    fn savepoint_drop() -> Result<()> {
        let h = TestHelpers::new();
        h.db.execute("CREATE TABLE tbl(col)", ())?;
        let mut txn = h.db.transaction(TransactionType::Deferred)?;
        txn.execute("INSERT INTO tbl VALUES (1)", ())?;
        {
            let sp = txn.savepoint()?;
            sp.execute("INSERT INTO tbl VALUES (2)", ())?;
        }
        txn.commit()?;
        let count =
            h.db.query_row("SELECT COUNT(*) FROM tbl", (), |r| Ok(r[0].get_i64()))?;
        assert_eq!(count, 1);
        Ok(())
    }

    #[test]
    fn commit_fail() -> Result<()> {
        let h = TestHelpers::new();
        h.db.execute("CREATE TABLE tbl(col)", ())?;
        let txn = h.db.transaction(TransactionType::Deferred)?;
        txn.execute("ROLLBACK", ())?;
        match txn.commit() {
            Ok(_) => unreachable!(),
            Err(e) => assert_eq!(e.to_string(), "cannot commit - no transaction is active"),
        }
        Ok(())
    }
}
