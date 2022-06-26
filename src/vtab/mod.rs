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
//! - [UpdateVTab] indicates that the table supports INSERT/UPDATE/DELETE.
//! - [TransactionVTab] indicates that the table supports ROLLBACK.
//! - [FindFunctionVTab] indicates that the table overrides certain SQL functions when they
//!   operate on the table.
//! - [RenameVTab] indicates that the table supports ALTER TABLE RENAME TO.

use super::{
    ffi, function::ToContextResult, sqlite3_require_version, types::*, value::*, Connection,
};
pub use index_info::*;
pub use module::*;
use std::ffi::c_void;

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
    /// When registering the module with [Module::register], additional data can be passed
    /// as a parameter. This data will be passed to [connect](VTab::connect) and
    /// [create](CreateVTab::create). It can be used for any purpose.
    type Aux;

    /// Cursor implementation for this virtual table.
    type Cursor: VTabCursor;

    /// Corresponds to xConnect.
    ///
    /// This method is called called when connecting to an existing virtual table, either
    /// because it was previously created with CREATE VIRTUAL TABLE (see
    /// [CreateVTab::create]), or because it is an eponymous virtual table.
    ///
    /// This method must return a valid CREATE TABLE statement as a [String], along with a
    /// configured table instance. Additionally, all virtual tables are recommended to set
    /// a risk level using [VTabConnection::set_risk].
    ///
    /// The virtual table implementation will return an error if any of the arguments
    /// contain invalid UTF-8.
    fn connect(
        db: &'vtab mut VTabConnection,
        aux: &'vtab Self::Aux,
        args: &[&str],
    ) -> Result<(String, Self)>
    where
        Self: Sized;

    /// Corrresponds to xBestIndex.
    ///
    /// If best_index returns
    /// [`Err(Error::constraint_violation())`](Error::constraint_violation), then
    /// xBestIndex will return `SQLITE_CONSTRAINT`. Any other error will cause xBestIndex
    /// to fail.
    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()>;

    fn open(&'vtab mut self) -> Result<Self::Cursor>;
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
        db: &'vtab mut VTabConnection,
        aux: &'vtab Self::Aux,
        args: &[&str],
    ) -> Result<(String, Self)>
    where
        Self: Sized;

    /// Corresponds to xDestroy, when DROP TABLE is run on the virtual table.
    fn destroy(&mut self) -> Result<()>;
}

/// A virtual table that supports INSERT/UPDATE/DELETE.
pub trait UpdateVTab<'vtab>: VTab<'vtab> {
    /// Insert a new row into the virtual table.
    ///
    /// For rowid tables, the first value is either the provided rowid or NULL. For WITHOUT
    /// ROWID tables, the first value is always NULL. If the first value is NULL and the
    /// table is a rowid table, then the returned i64 must be the rowid of the new row. In
    /// all other cases the returned value is ignored.
    fn insert(&mut self, args: &[&ValueRef]) -> Result<i64>;

    /// Update an existing row in the virtual table.
    ///
    /// The rowid argument corresponds to the rowid or PRIMARY KEY of the existing row to
    /// update. For rowid tables, the first value of args will be the new rowid for the
    /// row. For WITHOUT ROWID tables, the first value of args will be NULL.
    fn update(&mut self, rowid: &ValueRef, args: &[&ValueRef]) -> Result<()>;

    /// Delete a row from the virtual table.
    ///
    /// The rowid argument corresopnds to the rowid (or PRIMARY KEY for WITHOUT ROWID
    /// tables) of the row to delete.
    fn delete(&mut self, rowid: &ValueRef) -> Result<()>;
}

/// A virtual table that supports ROLLBACK.
///
/// See [VTabTransaction] for details.
pub trait TransactionVTab<'vtab>: UpdateVTab<'vtab> {
    type Transaction: VTabTransaction;

    /// Begin a transaction.
    fn begin(&'vtab mut self) -> Result<Self::Transaction>;
}

pub trait FindFunctionVTab<'vtab>: VTab<'vtab> {}

/// A virtual table that supports ALTER TABLE RENAME.
pub trait RenameVTab<'vtab>: VTab<'vtab> {
    /// Corresponds to xRename, when ALTER TABLE RENAME is run on the virtual table.
    fn rename(&mut self, name: &str) -> Result<()>;
}

/// Implementation of the cursor type for a virtual table.
pub trait VTabCursor {
    /// The type of all columns in this virtual table. For tables with columns of varying
    /// data types, [Value] can be used.
    type ColumnType: ToContextResult;

    /// Begin a search of the virtual table. This method is always invoked after creating
    /// the cursor, before any other methods of this trait. After calling this method, the
    /// cursor should point to the first row of results (or [eof](VTabCursor::eof) should
    /// return true to indicate there are no results).
    fn filter(
        &mut self,
        index_num: usize,
        index_str: Option<&str>,
        args: &[&ValueRef],
    ) -> Result<()>;

    /// Move the cursor one row forward.
    fn next(&mut self) -> Result<()>;

    /// Check if the cursor currently points beyond the end of the valid results.
    fn eof(&self) -> bool;

    /// Fetch the column numbered idx for the current row. The indexes correspond to the
    /// order the columns were declared by [VTab::connect].
    fn column(&self, idx: usize) -> Self::ColumnType;

    /// Fetch the rowid for the current row.
    fn rowid(&self) -> Result<i64>;
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
pub trait VTabTransaction {
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

/// Indicate the risk level for a virtual table.
///
/// It is recommended that all functions and virtual table implementations set a risk level,
/// but the default is [RiskLevel::Innocuous] if TRUSTED_SCHEMA=on and [RiskLevel::DirectOnly]
/// otherwise.
///
/// See [this discussion](https://www.sqlite.org/src/doc/latest/doc/trusted-schema.md) for more
/// details about the motivation and implications.
pub enum RiskLevel {
    /// An innocuous function or virtual table is one that can only read content from the
    /// database file in which it resides, and can only alter the database in which it
    /// resides.
    Innocuous,
    /// A direct-only function or virtual table has side-effects that go outside the
    /// database file in which it lives, or return information from outside of the database
    /// file.
    DirectOnly,
}

/// A wrapper around [Connection] that supports configuring virtual table implementations.
#[repr(transparent)]
pub struct VTabConnection {
    #[allow(dead_code)]
    db: ffi::sqlite3,
}

impl VTabConnection {
    /// Return the underlying [Connection].
    pub fn get(&mut self) -> &mut Connection {
        unsafe { &mut *(self as *mut VTabConnection as *mut Connection) }
    }

    /// Enable ON CONFLICT support for UPDATEs for this virtual table.
    ///
    /// Enabling this support has additional requirements on the [UpdateVTab::update]
    /// method of the virtual table implementation. See [the SQLite documentation](https://www.sqlite.org/c3ref/c_vtab_constraint_support.html#sqlitevtabconstraintsupport) for more details.
    ///
    /// Requires SQLite 3.7.7.
    pub fn enable_constraints(&mut self) -> Result<()> {
        sqlite3_require_version!(3_007_007, unsafe {
            Error::from_sqlite(ffi::sqlite3_vtab_config(
                &mut self.db,
                ffi::SQLITE_VTAB_CONSTRAINT_SUPPORT,
                1,
            ))
        })
    }

    /// Set the risk level of this virtual table.
    ///
    /// See the [RiskLevel] enum for details about what the individual options mean.
    ///
    /// Requires SQLite 3.31.0.
    pub fn set_risk(&mut self, level: RiskLevel) -> Result<()> {
        let _ = level;
        sqlite3_require_version!(3_031_000, unsafe {
            Error::from_sqlite(ffi::sqlite3_vtab_config(
                &mut self.db,
                match level {
                    RiskLevel::Innocuous => ffi::SQLITE_VTAB_INNOCUOUS,
                    RiskLevel::DirectOnly => ffi::SQLITE_VTAB_DIRECTONLY,
                },
            ))
        })
    }
}

/// Handle to the module and aux data, so that it can be properly dropped when the module is
/// unloaded.
pub(crate) struct ModuleHandle<'vtab, T: VTab<'vtab>> {
    pub vtab: ffi::sqlite3_module,
    pub aux: T::Aux,
}

impl<'vtab, T: VTab<'vtab>> ModuleHandle<'vtab, T> {
    pub unsafe fn from_ptr<'a>(ptr: *mut c_void) -> &'a ModuleHandle<'vtab, T> {
        &*(ptr as *mut ModuleHandle<'vtab, T>)
    }
}
