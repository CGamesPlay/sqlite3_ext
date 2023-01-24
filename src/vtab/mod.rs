//! Create virtual tables.
//!
//! To create a virtual table, define the virtual table module and then register it on each
//! connection it will be used from. The [sqlite3_ext_vtab](sqlite3_ext_macro::sqlite3_ext_vtab) macro is used to define the virtual table module. It can be registered using [Connection::create_module].
//!
//! There are 3 base types of virtual tables:
//!
//! - [StandardModule] is a virtual table which is created using the CREATE VIRTUAL TABLE
//!   command.
//! - [EponymousModule] is a virtual table which is available ambiently in the database
//!   connection without being explicitly created.
//! - [EponymousOnlyModule] is similar to EponymousModule, but CREATE VIRTUAL TABLE is
//!   explicitly forbidden for these modules.
//!
//! In addition to the base type of virtual table, there are several traits which can be
//! implemented to add behavior.
//!
//! - [VTab] is required to be implemented by all virtual tables.
//! - [CreateVTab] indicates that the table supports CREATE VIRTUAL TABLE.
//! - [UpdateVTab] indicates that the table supports INSERT/UPDATE/DELETE.
//! - [TransactionVTab] indicates that the table supports ROLLBACK.
//! - [FindFunctionVTab] indicates that the table overrides certain SQL functions when they
//!   operate on the table.
//! - [RenameVTab] indicates that the table supports ALTER TABLE RENAME TO.

use super::{
    ffi, function::ToContextResult, sqlite3_match_version, types::*, value::*, Connection,
};
pub use function::*;
pub use index_info::*;
pub use module::*;
use std::{ffi::c_void, ops::Deref, slice};

mod function;
mod index_info;
mod module;
pub(crate) mod stubs;

/// A virtual table.
///
/// This trait defines functionality required by all virtual tables. A read-only,
/// eponymous-only virtual table (e.g. a table-valued function) can implement only this trait.
pub trait VTab<'vtab> {
    /// Additional data associated with the virtual table module.
    ///
    /// When registering the module with [Connection::create_module], additional data can
    /// be passed as a parameter. This data will be passed to [connect](VTab::connect) and
    /// [create](CreateVTab::create). It can be used for any purpose.
    type Aux: 'vtab;

    /// Cursor implementation for this virtual table.
    type Cursor: VTabCursor<'vtab>;

    /// Corresponds to xConnect.
    ///
    /// This method is called called when connecting to an existing virtual table, either
    /// because it was previously created with CREATE VIRTUAL TABLE (see
    /// [CreateVTab::create]), or because it is an eponymous virtual table.
    ///
    /// This method must return a valid CREATE TABLE statement as a [String], along with a
    /// configured table instance. Additionally, all virtual tables are recommended to set
    /// a risk level using [VTabConnection::set_risk_level].
    ///
    /// The virtual table implementation will return an error if any of the arguments
    /// contain invalid UTF-8.
    fn connect(
        db: &'vtab VTabConnection,
        aux: &'vtab Self::Aux,
        args: &[&str],
    ) -> Result<(String, Self)>
    where
        Self: Sized;

    /// Corrresponds to xBestIndex.
    ///
    /// This method is called when SQLite is planning to query a virtual table. See
    /// [IndexInfo] for details.
    ///
    /// If this method returns Err([SQLITE_CONSTRAINT]), that does not indicate an error. Rather,
    /// it indicates that the particular combination of input parameters specified is insufficient
    /// for the virtual table to do its job. This is logically the same as setting the
    /// [estimated_cost](IndexInfo::set_estimated_cost) to infinity. If every call to best_index
    /// for a particular query plan returns this error, that means there is no way for the virtual
    /// table to be safely used, and the SQLite call will fail with a "no query solution" error.
    fn best_index(&'vtab self, index_info: &mut IndexInfo) -> Result<()>;

    /// Create an uninitialized query.
    fn open(&'vtab self) -> Result<Self::Cursor>;

    /// Corresponds to xDisconnect. This method is called when the database connection is
    /// being closed. The implementation should not remove the underlying data, but it
    /// should release any resources associated with the virtual table implementation. This method is the inverse of [Self::connect].
    ///
    /// After invoking this method, the virtual table implementation is immediately
    /// dropped. The default implementation of this method simply returns Ok.
    fn disconnect(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A non-eponymous virtual table that supports CREATE VIRTUAL TABLE.
pub trait CreateVTab<'vtab>: VTab<'vtab> {
    /// List of shadow table names.
    ///
    /// This can be set by a virtual table implementation to automatically implement the
    /// xShadowName method. For example, "data" appears in this slice, then SQLite will
    /// understand that "vtab_data" is a shadow table for a table named "vtab" created with
    /// this module.
    ///
    /// Shadow tables are read-only if the database has SQLITE_DBCONFIG_DEFENSIVE set, and
    /// SQLite is version 3.26.0 or greater. For more information, see [the SQLite
    /// documentation](https://www.sqlite.org/vtab.html#the_xshadowname_method).
    const SHADOW_NAMES: &'static [&'static str] = &[];

    /// Corresponds to xCreate.
    ///
    /// This method is invoked when a CREATE VIRTUAL TABLE statement is invoked on the
    /// module. Future connections to the created table will use [VTab::connect] instead.
    ///
    /// This method has the same requirements as [VTab::connect]; see that method
    /// for more details.
    fn create(
        db: &'vtab VTabConnection,
        aux: &'vtab Self::Aux,
        args: &[&str],
    ) -> Result<(String, Self)>
    where
        Self: Sized;

    /// Corresponds to xDestroy, when DROP TABLE is run on the virtual table. The virtual
    /// table implementation should destroy any underlying state that was created by
    /// [Self::create].
    ///
    /// After invoking this method, the virtual table implementation is immediately
    /// dropped.
    fn destroy(&mut self) -> Result<()>;
}

/// A virtual table that supports INSERT/UPDATE/DELETE.
pub trait UpdateVTab<'vtab>: VTab<'vtab> {
    /// Modify a single row in the virtual table. The info parameter may be used to
    /// determine the type of change being performed by this update.
    ///
    /// If the change is an INSERT for a table with rowids and the provided rowid was NULL,
    /// then the virtual table must generate and return a rowid for the inserted row. In
    /// all other cases, the returned Ok value of this method is ignored.
    ///
    /// It isn't possible to provide a mutable reference to the virtual table
    /// implementation because there may be active cursors affecting the table or even the
    /// row that is being updated. Use Rust's interior mutability types to properly
    /// implement this method.
    fn update(&'vtab self, info: &mut ChangeInfo) -> Result<i64>;
}

/// A virtual table that supports ROLLBACK.
///
/// See [VTabTransaction] for details.
pub trait TransactionVTab<'vtab>: UpdateVTab<'vtab> {
    type Transaction: VTabTransaction<'vtab>;

    /// Begin a transaction.
    fn begin(&'vtab self) -> Result<Self::Transaction>;
}

/// A virtual table that overloads some functions.
///
/// A virtual table implementation may choose to overload certain functions when the first
/// argument to the function refers to a column in the virtual table. To do this, add a
/// [VTabFunctionList] to the virtual table struct and return a reference to it from the
/// [functions][FindFunctionVTab::functions] method. When a function uses a column from this
/// virtual table as its first argument, the returned list will be checked to see if the
/// virtual table would like to overload the function.
///
/// Overloading additionally allows the virtual table to indicate that the virtual table is
/// able to exploit the function to speed up a query result. For this to work, the function
/// must take exactly two arguments and appear as a boolean in the WHERE clause of a query. The
/// [ConstraintOp] supplied with the function will then be provided as an [IndexInfoConstraint]
/// to [VTab::best_index]. This feature additionally requires SQLite 3.25.0.
///
/// For more details, see [the SQLite documentation](https://www.sqlite.org/vtab.html#the_xfindfunction_method).
///
/// # Example
///
/// Here is a brief summary of how to use this trait:
///
/// ```no_run
/// # use sqlite3_ext_macro::*;
/// use sqlite3_ext::{function::*, vtab::*, *};
///
/// #[sqlite3_ext_vtab(StandardModule)]
/// struct MyVTab<'vtab> {
///     /// Used to store the overloaded functions
///     functions: VTabFunctionList<'vtab, Self>
/// }
/// # sqlite3_ext_doctest_impl!(MyVTab<'vtab>);
///
/// impl<'vtab> MyVTab<'vtab> {
///     /// Register the overloaded functions. Should be called from connect/create.
///     fn init_functions(&mut self) {
///         self.functions.add_method(1, "my_func", None, |vtab, ctx, args| {
///             println!("my_func was called");
///             ctx.set_result(&*args[0])
///         });
///     }
/// }
///
/// /// Return the owned functions list.
/// impl<'vtab> FindFunctionVTab<'vtab> for MyVTab<'vtab> {
///     fn functions(&self) -> &VTabFunctionList<'vtab, Self> {
///         &self.functions
///     }
/// }
/// ```
pub trait FindFunctionVTab<'vtab>: VTab<'vtab> {
    /// Retrieve a reference to the [VTabFunctionList] associated with this virtual table.
    fn functions(&'vtab self) -> &'vtab VTabFunctionList<'vtab, Self>;
}

/// A virtual table that supports ALTER TABLE RENAME.
pub trait RenameVTab<'vtab>: VTab<'vtab> {
    /// Corresponds to xRename, when ALTER TABLE RENAME is run on the virtual table. If
    /// this method returns Ok, then SQLite will disconnect this virtual table
    /// implementation and connect to a new implementation with the updated name.
    fn rename(&'vtab self, name: &str) -> Result<()>;
}

/// Implementation of the cursor type for a virtual table.
pub trait VTabCursor<'vtab> {
    /// Begin a search of the virtual table. This method is always invoked after creating
    /// the cursor, before any other methods of this trait. After calling this method, the
    /// cursor should point to the first row of results (or [eof](VTabCursor::eof) should
    /// return true to indicate there are no results).
    ///
    /// The index_num parameter is an arbitrary value which was passed to
    /// [IndexInfo::set_index_num]. The index_str parameter is an arbitrary value which was
    /// passed to [IndexInfo::set_index_str].
    fn filter(
        &mut self,
        index_num: i32,
        index_str: Option<&str>,
        args: &mut [&mut ValueRef],
    ) -> Result<()>;

    /// Move the cursor one row forward.
    fn next(&mut self) -> Result<()>;

    /// Check if the cursor currently points beyond the end of the valid results.
    fn eof(&mut self) -> bool;

    /// Fetch the column numbered idx for the current row. The indexes correspond to the order the
    /// columns were declared by [VTab::connect]. The output value must be assigned to the context
    /// using [ColumnContext::set_result]. If no result is set, SQL NULL is returned. If this
    /// method returns an Err value, the SQL statement will fail, even if a result had been set
    /// before the failure.
    fn column(&mut self, idx: usize, context: &ColumnContext) -> Result<()>;

    /// Fetch the rowid for the current row.
    fn rowid(&mut self) -> Result<i64>;
}

/// Implementation of the transaction type for a virtual table.
///
/// Virtual tables which modify resources outside of the database in which they are defined may
/// require additional work in order to safely implement fallible transactions. If the virtual
/// table only modifies data inside of the database in which it is defined, then SQLite's
/// built-in transaction support is sufficient and implementing [TransactionVTab] is not
/// necessary. The most important methods of this trait are
/// [rollback](VTabTransaction::rollback) and [rollback_to](VTabTransaction::rollback_to). If
/// it is not possible to correctly implement these methods for the virtual table, then there
/// is no need to implement [TransactionVTab] at all.
///
/// Virtual table transactions do not nest, so there will never be more than one instance of
/// this trait per virtual table. Instances are always dropped in a call to either
/// [commit](VTabTransaction::commit) or [rollback](VTabTransaction::rollback), with one
/// exception: eponymous tables implementing this trait automatically begin a transaction after
/// [VTab::connect], but this transaction will be later on dropped without any methods being
/// called on it. This is harmless, because if an UPDATE occurs for such a table, a new
/// transaction will be created, dropping the previous one first.
///
/// Note that the [savepoint](VTabTransaction::savepoint), [release](VTabTransaction::release),
/// and [rollback_to](VTabTransaction::rollback_to) methods require SQLite 3.7.7. On previous
/// versions of SQLite, these methods will not be called, which may result in unsound behavior.
/// In the following example, the virtual table will incorrectly commit changes which should
/// have been rolled back.
///
/// ```sql
/// BEGIN;
/// SAVEPOINT a;
/// UPDATE my_virtual_table SET foo = 'bar';
/// ROLLBACK TO a;
/// COMMIT;
/// ```
pub trait VTabTransaction<'vtab> {
    /// Start a two-phase commit.
    ///
    /// This method is only invoked prior to a commit or rollback. In order to implement
    /// two-phase commit, the sync method on all virtual tables is invoked prior to
    /// invoking the commit method on any virtual table. If any of the sync methods fail,
    /// the entire transaction is rolled back.
    fn sync(&mut self) -> Result<()>;

    /// Finish a commit.
    ///
    /// A call to this method always follows a prior call sync.
    fn commit(self) -> Result<()>;

    /// Roll back a commit.
    fn rollback(self) -> Result<()>;

    /// Save current state as a save point.
    ///
    /// The current state of the virtual table should be saved as savepoint n. There is
    /// no guarantee that n starts at zero or increases by 1 in between calls.
    ///
    /// This method will only be called on SQLite 3.7.7 or later.
    fn savepoint(&mut self, n: i32) -> Result<()>;

    /// Invalidate previous save points.
    ///
    /// All save points numbered >= n should be invalidated. This does not mean the
    /// changes are ready to be committed, just that there is no need to maintain a record
    /// of those saved states any more.
    ///
    /// Note that there is no guarantee that n will be a value from a previous call to
    /// [savepoint](VTabTransaction::savepoint).
    ///
    /// This method will only be called on SQLite 3.7.7 or later.
    fn release(&mut self, n: i32) -> Result<()>;

    /// Restore a save point.
    ///
    /// The virtual table should revert to the state it had when
    /// [savepoint](VTabTransaction::savepoint) was called the lowest number >= n. There is
    /// no guarantee that [savepoint](VTabTransaction::savepoint) was ever called with n
    /// exactly.
    ///
    /// This method will only be called on SQLite 3.7.7 or later.
    fn rollback_to(&mut self, n: i32) -> Result<()>;
}

/// A wrapper around [Connection] that supports configuring virtual table implementations.
#[repr(transparent)]
pub struct VTabConnection {
    db: ffi::sqlite3,
}

impl VTabConnection {
    unsafe fn from_ptr<'a>(db: *mut ffi::sqlite3) -> &'a Self {
        &*(db as *mut Self)
    }

    /// Indicate that this virtual table properly verifies constraints for updates.
    ///
    /// If this is enabled, then the virtual table guarantees that if the
    /// [UpdateVTab::update] method returns Err([SQLITE_CONSTRAINT]), it will do so before
    /// any modifications to internal or persistent data structures have been made. If the
    /// ON CONFLICT mode is ABORT, FAIL, IGNORE or ROLLBACK, SQLite is able to roll back a
    /// statement or database transaction, and abandon or continue processing the current
    /// SQL statement as appropriate. If the ON CONFLICT mode is REPLACE and the update
    /// method returns SQLITE_CONSTRAINT, SQLite handles this as if the ON CONFLICT mode
    /// had been ABORT.
    ///
    /// Requires SQLite 3.7.7. On earlier versions of SQLite, this is a harmless no-op.
    pub fn enable_constraints(&self) {
        sqlite3_match_version! {
            3_007_007 => unsafe {
                let guard = self.lock();
                Error::from_sqlite_desc(ffi::sqlite3_vtab_config()(
                    guard.as_mut_ptr(),
                    ffi::SQLITE_VTAB_CONSTRAINT_SUPPORT,
                    1,
                ), guard)
                .unwrap()
            },
            _ => (),
        }
    }

    /// Set the risk level of this virtual table.
    ///
    /// See the [RiskLevel](super::RiskLevel) enum for details about what the individual
    /// options mean.
    ///
    /// Requires SQLite 3.31.0. On earlier versions of SQLite, this is a harmless no-op.
    pub fn set_risk_level(&self, level: super::RiskLevel) {
        let _ = level;
        sqlite3_match_version! {
            3_031_000 => unsafe {
                let guard = self.lock();
                Error::from_sqlite_desc(ffi::sqlite3_vtab_config()(
                    guard.as_mut_ptr(),
                    match level {
                        super::RiskLevel::Innocuous => ffi::SQLITE_VTAB_INNOCUOUS,
                        super::RiskLevel::DirectOnly => ffi::SQLITE_VTAB_DIRECTONLY,
                    },
                ), guard)
                .unwrap();
            },
            _ => (),
        }
    }
}

impl Deref for VTabConnection {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        unsafe { Connection::from_ptr(&self.db as *const _ as _) }
    }
}

/// Information about an INSERT/UPDATE/DELETE on a virtual table.
pub struct ChangeInfo {
    #[cfg_attr(not(modern_sqlite), allow(unused))]
    db: *mut ffi::sqlite3,
    argc: usize,
    argv: *mut *mut ValueRef,
}

impl ChangeInfo {
    /// Returns the type of update being performed.
    pub fn change_type(&self) -> ChangeType {
        if self.args().len() == 0 {
            ChangeType::Delete
        } else if self.rowid().is_null() {
            ChangeType::Insert
        } else {
            ChangeType::Update
        }
    }

    /// Returns the rowid (or, for WITHOUT ROWID tables, the PRIMARY KEY column) of the row
    /// being deleted or updated.
    ///
    /// Semantically, an UPDATE to a virtual table is identical to a DELETE followed by an
    /// INSERT. In that sense, this method returns the rowid or PRIMARY KEY column of the
    /// row being deleted. The rowid of the row being inserted is available as the first
    /// element in [args](Self::args).
    ///
    /// For the mutable version, see [rowid_mut](Self::rowid_mut).
    pub fn rowid(&self) -> &ValueRef {
        debug_assert!(self.argc > 0);
        unsafe { &**self.argv }
    }

    /// Mutable version of [rowid](Self::rowid).
    pub fn rowid_mut(&mut self) -> &mut ValueRef {
        debug_assert!(self.argc > 0);
        unsafe { &mut **self.argv }
    }

    /// Returns the arguments for an INSERT or UPDATE. The meaning of the first element in
    /// this slice depends on the type of change being performed:
    ///
    /// - For an INSERT on a WITHOUT ROWID table, the first element is always NULL. The
    ///   PRIMARY KEY is listed among the remaining elements.
    /// - For an INSERT on a regular table, if the first element is NULL, it indicates that
    ///   a rowid must be generated and returned from [UpdateVTab::update]. Otherwise, the
    ///   first element is the rowid.
    /// - For an UPDATE, the first element is the new value for the rowid or PRIMARY KEY
    ///   column.
    ///
    /// In all cases, the second and following elements correspond to the values for all
    /// columns in the order declared in the virtual table's schema (returned by
    /// [VTab::connect] / [CreateVTab::create]).
    ///
    /// For the mutable version, see [args_mut](Self::args_mut).
    pub fn args(&self) -> &[&ValueRef] {
        debug_assert!(self.argc > 0);
        unsafe { slice::from_raw_parts(self.argv.offset(1) as _, self.argc - 1) }
    }

    /// Mutable version of [args](Self::args).
    pub fn args_mut(&mut self) -> &mut [&mut ValueRef] {
        debug_assert!(self.argc > 0);
        unsafe { slice::from_raw_parts_mut(self.argv.offset(1) as _, self.argc - 1) }
    }

    /// Return the ON CONFLICT mode of the current SQL statement. In order for this method
    /// to be useful, the virtual table needs to have previously enabled ON CONFLICT
    /// support using [VTabConnection::enable_constraints].
    ///
    /// Requires SQLite 3.7.7. On earlier versions, this will always return
    /// [ConflictMode::Abort].
    pub fn conflict_mode(&self) -> ConflictMode {
        sqlite3_match_version! {
            3_007_007 => {
                ConflictMode::from_sqlite(unsafe { ffi::sqlite3_vtab_on_conflict(self.db) })
            }
            _ => ConflictMode::Abort,
        }
    }
}

impl std::fmt::Debug for ChangeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("ChangeInfo")
            .field("change_type", &self.change_type())
            .field("rowid", &self.rowid())
            .field("args", &self.args())
            .field("conflict_mode", &self.conflict_mode())
            .finish()
    }
}

/// Indicates the type of modification that is being applied to the virtual table.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ChangeType {
    /// Indicates an SQL INSERT.
    Insert,
    /// Indicates an SQL DELETE.
    Delete,
    /// Indicates an SQL UPDATE.
    Update,
}

/// Indicates the ON CONFLICT mode for the SQL statement currently being executed.
///
/// An [UpdateVTab] which has used [VTabConnection::enable_constraints] can examine this value
/// to determine how to handle a conflict during a change.
///
/// For details about what each mode means, see [the SQLite documentation](https://www.sqlite.org/lang_conflict.html).
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ConflictMode {
    /// Corresponds to ON CONFLICT ROLLBACK.
    Rollback,
    /// Corresponds to ON CONFLICT IGNORE.
    Ignore,
    /// Corresponds to ON CONFLICT FAIL.
    Fail,
    /// Corresponds to ON CONFLICT ABORT.
    Abort,
    /// Corresponds to ON CONFLICT REPLACE.
    Replace,
}

impl ConflictMode {
    #[cfg(modern_sqlite)]
    fn from_sqlite(val: i32) -> Self {
        match val {
            1 => ConflictMode::Rollback,
            2 => ConflictMode::Ignore,
            3 => ConflictMode::Fail,
            4 => ConflictMode::Abort,
            5 => ConflictMode::Replace,
            _ => panic!("invalid conflict mode"),
        }
    }
}

/// Describes the run-time environment of the [VTabCursor::column] method.
#[repr(transparent)]
pub struct ColumnContext {
    base: ffi::sqlite3_context,
}

impl ColumnContext {
    pub(crate) fn as_ptr<'a>(&self) -> *mut ffi::sqlite3_context {
        &self.base as *const ffi::sqlite3_context as _
    }

    pub(crate) unsafe fn from_ptr<'a>(base: *mut ffi::sqlite3_context) -> &'a mut Self {
        &mut *(base as *mut Self)
    }

    /// Return a handle to the current database.
    pub fn db(&self) -> &Connection {
        unsafe { Connection::from_ptr(ffi::sqlite3_context_db_handle(self.as_ptr())) }
    }

    /// Return true if the column being fetched is part of an UPDATE operation during which
    /// the column value will not change.
    ///
    /// See [ValueRef::nochange] for details and usage.
    ///
    /// This method is provided as an optimization. It is permissible for this method to
    /// return false even if the value is unchanged. The virtual table implementation must
    /// function correctly even if this method were to always return false.
    ///
    /// Requires SQLite 3.22.0. On earlier versions of SQLite, this method always returns
    /// false.
    pub fn nochange(&self) -> bool {
        crate::sqlite3_match_version! {
            3_022_000 => (unsafe { ffi::sqlite3_vtab_nochange(self.as_ptr()) } != 0),
            _ => false,
        }
    }

    /// Assign the given value to the column. This function always returns Ok.
    pub fn set_result(&self, val: impl ToContextResult) -> Result<()> {
        unsafe { val.assign_to(self.as_ptr()) };
        Ok(())
    }
}
