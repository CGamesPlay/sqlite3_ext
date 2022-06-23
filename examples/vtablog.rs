//! Rust implementation of the vtablog virtual table.
//!
//! For more information, consult [the original implementation](https://sqlite.org/src/file/ext/misc/vtablog.c).

use sqlite3_ext::{function::*, vtab::*, *};
use std::sync::atomic::{AtomicUsize, Ordering};

enum VTabArg {
    Schema(String),
    NumRows(i64),
}

mod parsing {
    use super::VTabArg;
    use nom::{
        branch::alt,
        bytes::complete::{is_not, tag},
        character::complete::i64,
        combinator::{eof, map},
        multi::many0,
        sequence::{terminated, tuple},
        IResult,
    };
    use sqlite3_ext::*;

    pub(super) fn parse_arg(input: &str) -> Result<VTabArg> {
        let ret: IResult<&str, VTabArg> = terminated(
            alt((
                map(tuple((tag("rows="), i64)), |(_, s): (&str, i64)| {
                    VTabArg::NumRows(s)
                }),
                map(
                    tuple((
                        tag("schema='"),
                        many0(alt((is_not("'"), tag("''")))),
                        tag("'"),
                    )),
                    |(_, s, _): (&str, Vec<&str>, &str)| VTabArg::Schema(s.join("")),
                ),
            )),
            eof,
        )(input);
        match ret {
            Ok((_, arg)) => Ok(arg),
            Err(e) => Err(Error::Module(format!("{}", e))),
        }
    }
}

#[sqlite3_ext_vtab(
    StandardModule,
    UpdateVTab,
    TransactionVTab,
    FindFunctionVTab,
    RenameVTab
)]
struct VTabLog {
    id: usize,
    num_rows: i64,
    num_cursors: usize,
    num_transactions: usize,
}

struct VTabLogCursor<'vtab> {
    vtab: &'vtab VTabLog,
    id: usize,
    rowid: i64,
}

struct VTabLogTransaction<'vtab> {
    vtab: &'vtab VTabLog,
    id: usize,
}

impl VTabLog {
    fn connect_create(args: &[&str], method: &str) -> Result<(String, Self)> {
        static N_INST: AtomicUsize = AtomicUsize::new(100);
        let id = N_INST.fetch_add(100, Ordering::SeqCst);

        println!("{}(tab={}, args={:?})", method, id, args);

        let mut num_rows = 0;
        let mut schema = None;

        let opts: Vec<VTabArg> = args[3..]
            .iter()
            .map(|a| Ok(parsing::parse_arg(a)?))
            .collect::<Result<_>>()?;
        for o in opts {
            match o {
                VTabArg::Schema(s) => schema = Some(s),
                VTabArg::NumRows(r) => num_rows = r,
            }
        }

        let schema = schema.ok_or_else(|| Error::Module("schema not provided".to_owned()))?;

        Ok((
            schema,
            VTabLog {
                id,
                num_rows,
                num_cursors: 0,
                num_transactions: 0,
            },
        ))
    }
}

impl<'vtab> VTab<'vtab> for VTabLog {
    type Aux = ();
    type Cursor = VTabLogCursor<'vtab>;

    fn connect(
        _: &'vtab mut VTabConnection,
        _: &'vtab Self::Aux,
        args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create(args, "connect")
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        println!("best_index(tab={}, index_info={:?})", self.id, index_info);
        Ok(())
    }

    fn open(&'vtab mut self) -> Result<Self::Cursor> {
        self.num_cursors += 1;
        let ret = VTabLogCursor {
            vtab: self,
            id: self.id + self.num_cursors,
            rowid: 0,
        };
        println!("open(tab={}, cursor={})", self.id, ret.id);
        Ok(ret)
    }
}

impl<'vtab> CreateVTab<'vtab> for VTabLog {
    const SHADOW_NAMES: &'static [&'static str] = &["shadow"];

    fn create(
        _: &'vtab mut VTabConnection,
        _: &'vtab Self::Aux,
        args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create(args, "create")
    }

    fn destroy(&mut self) -> Result<()> {
        println!("destroy(tab={})", self.id);
        Ok(())
    }
}

impl<'vtab> UpdateVTab<'vtab> for VTabLog {
    fn insert(&mut self, args: &[&ValueRef]) -> Result<i64> {
        println!("insert(tab={}, args={:?})", self.id, args);
        Ok(1)
    }

    fn update(&mut self, rowid: &ValueRef, args: &[&ValueRef]) -> Result<()> {
        println!("update(tab={}, rowid={:?}, args={:?}", self.id, rowid, args);
        Ok(())
    }

    fn delete(&mut self, rowid: &ValueRef) -> Result<()> {
        println!("delete(tab={}, rowid={:?})", self.id, rowid);
        Ok(())
    }
}

impl<'vtab> TransactionVTab<'vtab> for VTabLog {
    type Transaction = VTabLogTransaction<'vtab>;

    fn begin(&'vtab mut self) -> Result<Self::Transaction> {
        self.num_transactions += 1;
        let ret = VTabLogTransaction {
            vtab: self,
            id: self.id + self.num_transactions,
        };
        println!("begin(tab={}, transaction={})", self.id, ret.id);
        Ok(ret)
    }
}

impl<'vtab> FindFunctionVTab<'vtab> for VTabLog {}

impl<'vtab> RenameVTab<'vtab> for VTabLog {
    fn rename(&mut self, name: &str) -> Result<()> {
        println!("rename(tab={}, name={:?})", self.id, name);
        Ok(())
    }
}

impl Drop for VTabLog {
    fn drop(&mut self) {
        println!("drop(tab={})", self.id);
    }
}

impl Drop for VTabLogTransaction<'_> {
    fn drop(&mut self) {
        println!(
            "drop_transaction(tab={}, transaction={})",
            self.vtab.id, self.id
        );
    }
}

impl VTabCursor for VTabLogCursor<'_> {
    fn filter(&mut self, _: usize, _: Option<&str>, args: &[&ValueRef]) -> Result<()> {
        println!(
            "filter(tab={}, cursor={}, args={:?})",
            self.vtab.id, self.id, args
        );
        self.rowid = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        println!(
            "next(tab={}, cursor={})\n  rowid {} -> {}",
            self.vtab.id,
            self.id,
            self.rowid,
            self.rowid + 1
        );
        self.rowid += 1;
        Ok(())
    }

    fn eof(&self) -> bool {
        let ret = self.rowid >= self.vtab.num_rows;
        println!("eof(tab={}, cursor={}) -> {}", self.vtab.id, self.id, ret);
        ret
    }

    fn column(&self, _: &Context, idx: usize) -> Result<Value> {
        const ALPHABET: &[u8] = "abcdefghijklmnopqrstuvwxyz".as_bytes();
        let ret = ALPHABET
            .get(idx)
            .map(|l| format!("{}{}", *l as char, self.rowid))
            .unwrap_or_else(|| format!("{{{}}}{}", idx, self.rowid));
        println!(
            "column(tab={}, cursor={}, idx={}) -> {:?}",
            self.vtab.id, self.id, idx, ret
        );
        Ok(Value::Text(ret))
    }

    fn rowid(&self) -> Result<i64> {
        println!(
            "rowid(tab={}, cursor={}) -> {}",
            self.vtab.id, self.id, self.rowid
        );
        Ok(self.rowid)
    }
}

impl Drop for VTabLogCursor<'_> {
    fn drop(&mut self) {
        println!("drop(tab={}, cursor={})", self.vtab.id, self.id);
    }
}

impl<'vtab> VTabTransaction for VTabLogTransaction<'vtab> {
    fn sync(&mut self) -> Result<()> {
        println!("sync(tab={}, transaction={})", self.vtab.id, self.id);
        Ok(())
    }

    fn commit(self) -> Result<()> {
        println!("commit(tab={}, transaction={})", self.vtab.id, self.id);
        Ok(())
    }

    fn rollback(self) -> Result<()> {
        println!("rollback(tab={}, transaction={})", self.vtab.id, self.id);
        Ok(())
    }

    fn savepoint(&mut self, n: i32) -> Result<()> {
        println!(
            "savepoint(tab={}, transaction={}, n={})",
            self.vtab.id, self.id, n
        );
        Ok(())
    }

    fn release(&mut self, n: i32) -> Result<()> {
        println!(
            "release(tab={}, transaction={}, n={})",
            self.vtab.id, self.id, n
        );
        Ok(())
    }

    fn rollback_to(&mut self, n: i32) -> Result<()> {
        println!(
            "rollback_to(tab={}, transaction={}, n={})",
            self.vtab.id, self.id, n
        );
        Ok(())
    }
}

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    db.create_module("vtablog", VTabLog::module(), ())?;
    Ok(())
}

#[cfg(all(test, feature = "static"))]
mod test {
    use super::*;
    use rusqlite;

    fn setup() -> rusqlite::Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open_in_memory()?;
        init(Connection::from_rusqlite(&conn))?;
        conn.execute(
            "CREATE VIRTUAL TABLE temp.log USING vtablog(schema='CREATE TABLE x(a,b,c)', rows=3)",
            [],
        )?;
        Ok(conn)
    }

    #[test]
    fn read() -> rusqlite::Result<()> {
        let conn = setup()?;
        let ret = conn
            .prepare("SELECT * FROM log")?
            .query_map([], |row| {
                Ok(vec![
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ])
            })?
            .into_iter()
            .collect::<rusqlite::Result<Vec<Vec<String>>>>()?;
        assert_eq!(
            ret,
            (0..3)
                .map(|i| vec![format!("a{}", i), format!("b{}", i), format!("c{}", i)])
                .collect::<Vec<Vec<String>>>()
        );
        Ok(())
    }

    #[test]
    fn update() -> rusqlite::Result<()> {
        let conn = setup()?;
        conn.execute("UPDATE log SET a = b WHERE rowid = 1", [])?;
        Ok(())
    }

    #[test]
    fn rename() -> rusqlite::Result<()> {
        let conn = setup()?;
        conn.execute("ALTER TABLE log RENAME to newname", [])?;
        Ok(())
    }

    #[test]
    fn shadow_name() -> rusqlite::Result<()> {
        sqlite3_require_version!(3_026_000, {}, {
            return Ok(());
        });
        let conn = setup()?;
        conn.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
        match conn.execute("CREATE TABLE log_shadow (a, b, c)", []) {
            Err(_) => Ok(()),
            _ => panic!("expected error, got ok"),
        }
    }
}
