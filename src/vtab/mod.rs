//! Wrappers for creating virtual tables.

use super::{
    ffi, function::Context, sqlite3_libversion_number, sqlite3_require_version, types::*,
    value::Value, Connection,
};
pub use index_info::*;
use std::{marker::PhantomData, os::raw::c_void};

mod index_info;
pub mod stubs;

union ModuleBytes {
    bytes: [u8; std::mem::size_of::<ffi::sqlite3_module>()],
    module: ffi::sqlite3_module,
}

// We use this empty module hack to avoid specifying all of the fields for the module here. In
// general, we present the most modern API that we can, but use Result types to indicate when a
// feature is not available due to the runtime SQLite version. When statically linking, we
// emulate the same behavior, but we have to be a bit more cautious, since we are using the
// libsqlite3_sys presented API, which might otherwise cause compilation errors.
const EMPTY_MODULE: ffi::sqlite3_module = unsafe {
    ModuleBytes {
        bytes: [0_u8; std::mem::size_of::<ffi::sqlite3_module>()],
    }
    .module
};

/// Functionality required by all virtual tables. A read-only, eponymous-only virtual table
/// (e.g. a table-valued function) can implement only this trait.
pub trait VTab<'vtab> {
    type Aux;
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
        aux: Option<&'vtab Self::Aux>,
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
    /// Corresponds to xCreate.
    ///
    /// This method is invoked when a CREATE VIRTUAL TABLE statement is invoked on the
    /// module. Future connections to the created table will use [VTab::connect] instead.
    ///
    /// This method has the same requirements as [VTab::connect]; see that method
    /// for more details.
    fn create(
        db: &'vtab mut VTabConnection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)>
    where
        Self: Sized;

    /// Corresponds to xDestroy, when DROP TABLE is run on the virtual table.
    fn destroy(&mut self) -> Result<()>;
}

/// A virtual table that supports ALTER TABLE RENAME.
pub trait RenameVTab<'vtab>: VTab<'vtab> {
    /// Corresponds to xRename, when ALTER TABLE RENAME is run on the virtual table.
    fn rename(&mut self, name: &str) -> Result<()>;
}

/// A virtual table that supports INSERT/UPDATE/DELETE.
pub trait UpdateVTab<'vtab>: VTab<'vtab> {
    /// Insert a new row into the virtual table.
    ///
    /// For rowid tables, the first value is either the provided rowid or NULL. For WITHOUT
    /// ROWID tables, the first value is always NULL. If the first value is NULL and the
    /// table is a rowid table, then the returned i64 must be the rowid of the new row. In
    /// all other cases the returned value is ignored.
    fn insert(&mut self, args: &[&Value]) -> Result<i64>;

    /// Update an existing row in the virtual table.
    ///
    /// The rowid argument corresponds to the rowid or PRIMARY KEY of the existing row to
    /// update. For rowid tables, the first value of args will be the new rowid for the
    /// row. For WITHOUT ROWID tables, the first value of args will be NULL.
    fn update(&mut self, rowid: &Value, args: &[&Value]) -> Result<()>;

    /// Delete a row from the virtual table.
    ///
    /// The rowid argument corresopnds to the rowid (or PRIMARY KEY for WITHOUT ROWID
    /// tables) of the row to delete.
    fn delete(&mut self, rowid: &Value) -> Result<()>;
}

/// Implementation of the cursor type for a virtual table.
pub trait VTabCursor {
    fn filter(&mut self, index_num: usize, index_str: Option<&str>, args: &[&Value]) -> Result<()>;

    fn next(&mut self) -> Result<()>;

    fn eof(&self) -> bool;

    fn column(&self, context: &mut Context, idx: usize) -> Result<()>;

    fn rowid(&self) -> Result<i64>;
}

#[repr(transparent)]
pub struct Module<'vtab, T: VTab<'vtab>> {
    pub(crate) base: ffi::sqlite3_module,
    phantom: PhantomData<&'vtab T>,
}

impl<'vtab, T: VTab<'vtab>> Module<'vtab, T> {
    /// Declare an eponymous-only virtual table.
    ///
    /// For this virtual table, CREATE VIRTUAL TABLE is forbidden, but the table is
    /// ambiently available under the module name.
    ///
    /// This feature requires SQLite 3.9.0 or above. Older versions of SQLite do not
    /// support eponymous virtual tables, meaning they require at least one CREATE VIRTUAL
    /// TABLE statement to be used. If supporting these versions of SQLite is desired, you
    /// can either use [Module::eponymous] or [Module::standard] and return an error if
    /// there is an attempt to instantiate the virtual table more than once.
    pub fn eponymous_only() -> Result<Self> {
        const MIN_VERSION: i32 = 3_009_000;
        if sqlite3_libversion_number() >= MIN_VERSION {
            Ok(Module {
                base: ffi::sqlite3_module {
                    iVersion: 2,
                    xConnect: Some(stubs::vtab_connect::<T>),
                    xBestIndex: Some(stubs::vtab_best_index::<T>),
                    xDisconnect: Some(stubs::vtab_disconnect::<T>),
                    xOpen: Some(stubs::vtab_open::<T>),
                    xClose: Some(stubs::vtab_close::<T>),
                    xFilter: Some(stubs::vtab_filter::<T>),
                    xNext: Some(stubs::vtab_next::<T>),
                    xEof: Some(stubs::vtab_eof::<T>),
                    xColumn: Some(stubs::vtab_column::<T>),
                    xRowid: Some(stubs::vtab_rowid::<T>),
                    ..EMPTY_MODULE
                },
                phantom: PhantomData,
            })
        } else {
            Err(Error::VersionNotSatisfied(MIN_VERSION))
        }
    }

    /// Declare an eponymous virtual table. For this module, the virtual table is available
    /// ambiently in the database, but CREATE VIRTUAL TABLE can also be used to instantiate
    /// the table with alternative parameters.
    pub fn eponymous() -> Self {
        Module {
            base: ffi::sqlite3_module {
                iVersion: 2,
                xCreate: Some(stubs::vtab_connect::<T>),
                xConnect: Some(stubs::vtab_connect::<T>),
                xBestIndex: Some(stubs::vtab_best_index::<T>),
                xDisconnect: Some(stubs::vtab_disconnect::<T>),
                xDestroy: Some(stubs::vtab_disconnect::<T>),
                xOpen: Some(stubs::vtab_open::<T>),
                xClose: Some(stubs::vtab_close::<T>),
                xFilter: Some(stubs::vtab_filter::<T>),
                xNext: Some(stubs::vtab_next::<T>),
                xEof: Some(stubs::vtab_eof::<T>),
                xColumn: Some(stubs::vtab_column::<T>),
                xRowid: Some(stubs::vtab_rowid::<T>),
                ..EMPTY_MODULE
            },
            phantom: PhantomData,
        }
    }
}

impl<'vtab, T: CreateVTab<'vtab>> Module<'vtab, T> {
    pub fn standard() -> Self {
        Module {
            base: ffi::sqlite3_module {
                iVersion: 2,
                xCreate: Some(stubs::vtab_create::<T>),
                xConnect: Some(stubs::vtab_connect::<T>),
                xBestIndex: Some(stubs::vtab_best_index::<T>),
                xDisconnect: Some(stubs::vtab_disconnect::<T>),
                xDestroy: Some(stubs::vtab_destroy::<T>),
                xOpen: Some(stubs::vtab_open::<T>),
                xClose: Some(stubs::vtab_close::<T>),
                xFilter: Some(stubs::vtab_filter::<T>),
                xNext: Some(stubs::vtab_next::<T>),
                xEof: Some(stubs::vtab_eof::<T>),
                xColumn: Some(stubs::vtab_column::<T>),
                xRowid: Some(stubs::vtab_rowid::<T>),
                ..EMPTY_MODULE
            },
            phantom: PhantomData,
        }
    }
}

impl<'vtab, T: UpdateVTab<'vtab>> Module<'vtab, T> {
    pub fn with_update(mut self) -> Self {
        self.base.xUpdate = Some(stubs::vtab_update::<T>);
        self
    }
}

impl<'vtab, T: RenameVTab<'vtab>> Module<'vtab, T> {
    pub fn with_rename(mut self) -> Self {
        self.base.xRename = Some(stubs::vtab_rename::<T>);
        self
    }
}

/// Handle to the module and aux data, so that it can be properly dropped when the module is
/// unloaded.
pub(crate) struct ModuleHandle<'vtab, T: VTab<'vtab>> {
    pub vtab: Module<'vtab, T>,
    pub aux: Option<T::Aux>,
}

impl<'vtab, T: VTab<'vtab>> ModuleHandle<'vtab, T> {
    pub unsafe fn from_ptr<'a>(ptr: *mut c_void) -> &'a ModuleHandle<'vtab, T> {
        &*(ptr as *mut ModuleHandle<'vtab, T>)
    }
}

#[repr(C)]
struct VTabHandle<'vtab, T: VTab<'vtab>> {
    base: ffi::sqlite3_vtab,
    vtab: T,
    phantom: PhantomData<&'vtab T>,
}

#[repr(C)]
struct VTabCursorHandle<'vtab, T: VTab<'vtab>> {
    base: ffi::sqlite3_vtab_cursor,
    cursor: T::Cursor,
    phantom: PhantomData<&'vtab T>,
}

pub enum RiskLevel {
    /// An innocuous function or virtual table is one that can only read content from the
    /// database file in which it resides, and can only alter the database in which it
    /// resides.
    Innocuous,
    /// Direct-only elements that have side-effects that go outside the database file in
    /// which it lives, or return information from outside of the database file.
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
    /// See the [RiskLevel] enum for details about what the individual options mean. It is
    /// recommended that all virtual table implementations set a risk level, but the
    /// default is [RiskLevel::Innocuous] if TRUSTED_SCHEMA=on and [RiskLevel::DirectOnly]
    /// otherwise.
    ///
    /// See [this discussion](https://www.sqlite.org/src/doc/latest/doc/trusted-schema.md)
    /// for more details about the motivation and implications.
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
