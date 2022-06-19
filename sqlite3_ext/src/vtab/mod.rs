//! Wrappers for creating virtual tables.

use super::{ffi, function::Context, types::*, value::Value, Connection};
use std::{ffi::CStr, marker::PhantomData, os::raw::c_void, ptr, slice};

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

    /// Corresponds to xConnect. The virtual table implementation will return an error if
    /// any of the arguments contain invalid UTF-8.
    fn connect(
        db: &mut Connection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)>
    where
        Self: Sized;

    /// Corrresponds to xBestIndex. If best_index returns [`Err(Error::ConstraintViolation)`](Error::ConstraintViolation),
    /// then xBestIndex will return `SQLITE_CONSTRAINT`. Any other error will cause
    /// xBestIndex to fail.
    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()>;

    fn open(&'vtab mut self) -> Result<Self::Cursor>;
}

/// A non-eponymous virtual table that supports CREATE VIRTUAL TABLE.
pub trait CreateVTab<'vtab>: VTab<'vtab> {
    /// Corresponds to xCreate. The virtual table implementation will return an error if
    /// any of the arguments contain invalid UTF-8.
    fn create(
        db: &mut Connection,
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
    /// Insert a new row into the virtual table. For rowid tables, the first value is
    /// either the provided rowid or NULL. For WITHOUT ROWID tables, the first value is
    /// always NULL. If the first value is NULL and the table is a rowid table, then the
    /// returned i64 must be the rowid of the new row. In all other cases the returned
    /// value is ignored.
    fn insert(&mut self, args: &[&Value]) -> Result<i64>;

    /// Update an existing row in the virtual table. The rowid argument corresponds to the
    /// rowid or PRIMARY KEY of the existing row to update. For rowid tables, the first
    /// value of args will be the new rowid for the row. For WITHOUT ROWID tables, the
    /// first value of args will be NULL.
    fn update(&mut self, rowid: &Value, args: &[&Value]) -> Result<()>;

    /// Delete a row from the virtual table. The rowid argument corresopnds to the rowid
    /// (or PRIMARY KEY for WITHOUT ROWID tables) of the row to delete.
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
    /// ambiently available under the module name. This method requires SQLite >= 3.9.0.
    /// For more information, see [Module::eponymous_only_unchecked].
    pub fn eponymous_only() -> Result<Self> {
        ffi::require_version(3_009_000)?;
        unsafe { Ok(Self::eponymous_only_unchecked()) }
    }

    /// Declare an eponymous-only virtual table.
    ///
    /// # Safety
    ///
    /// On versions of SQLite older than 3.9.0, issuing a CREATE VIRTUAL TBALE
    /// on an eponymous-only table results in a crash.
    pub unsafe fn eponymous_only_unchecked() -> Self {
        Module {
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
    pub unsafe fn from_ptr<'a>(ptr: *mut c_void) -> &'a mut ModuleHandle<'vtab, T> {
        &mut *(ptr as *mut ModuleHandle<'vtab, T>)
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

#[repr(transparent)]
pub struct IndexInfo {
    base: ffi::sqlite3_index_info,
}

#[repr(transparent)]
pub struct IndexInfoConstraint {
    base: ffi::sqlite3_index_info_sqlite3_index_constraint,
}

#[repr(transparent)]
pub struct IndexInfoOrderBy {
    base: ffi::sqlite3_index_info_sqlite3_index_orderby,
}

#[repr(transparent)]
pub struct IndexInfoConstraintUsage {
    base: ffi::sqlite3_index_info_sqlite3_index_constraint_usage,
}

impl IndexInfo {
    pub fn constraints(&self) -> &[IndexInfoConstraint] {
        unsafe {
            slice::from_raw_parts(
                self.base.aConstraint as *const IndexInfoConstraint,
                self.base.nConstraint as _,
            )
        }
    }

    pub fn order_by(&self) -> &[IndexInfoOrderBy] {
        unsafe {
            slice::from_raw_parts(
                self.base.aOrderBy as *const IndexInfoOrderBy,
                self.base.nOrderBy as _,
            )
        }
    }

    pub fn constraint_usage(&self) -> &[IndexInfoConstraintUsage] {
        unsafe {
            slice::from_raw_parts(
                self.base.aConstraintUsage as *const IndexInfoConstraintUsage,
                self.base.nConstraint as _,
            )
        }
    }

    pub fn constraint_usage_mut(&mut self) -> &mut [IndexInfoConstraintUsage] {
        unsafe {
            slice::from_raw_parts_mut(
                self.base.aConstraintUsage as *mut IndexInfoConstraintUsage,
                self.base.nConstraint as _,
            )
        }
    }

    pub fn index_num(&self) -> usize {
        self.base.idxNum as _
    }

    pub fn set_index_num(&mut self, val: usize) {
        self.base.idxNum = val as _;
    }

    pub fn index_str(&self) -> Option<&str> {
        if self.base.idxStr.is_null() {
            None
        } else {
            let cstr = unsafe { CStr::from_ptr(self.base.idxStr) };
            cstr.to_str().ok()
        }
    }

    /// Set the index string to the provided value. This function can fail if SQLite is not
    /// able to allocate memory for the string.
    pub fn set_index_str(&mut self, val: Option<&str>) -> Result<()> {
        if self.base.needToFreeIdxStr != 0 {
            unsafe { ffi::sqlite3_free(self.base.idxStr as _) };
        }
        match val {
            None => {
                self.base.idxStr = ptr::null_mut();
                self.base.needToFreeIdxStr = 0;
            }
            Some(x) => {
                self.base.idxStr = ffi::str_to_sqlite3(x)?;
                self.base.needToFreeIdxStr = 1;
            }
        }
        Ok(())
    }

    /// Set the index string without copying.
    pub fn set_index_str_static(&mut self, val: &'static CStr) {
        if self.base.needToFreeIdxStr != 0 {
            unsafe { ffi::sqlite3_free(self.base.idxStr as _) };
        }
        self.base.idxStr = val.as_ptr() as _;
        self.base.needToFreeIdxStr = 0;
    }

    pub fn order_by_consumed(&self) -> bool {
        self.base.orderByConsumed != 0
    }

    pub fn set_order_by_consumed(&mut self, val: bool) {
        self.base.orderByConsumed = val as _;
    }

    pub fn estimated_cost(&self) -> f64 {
        self.base.estimatedCost
    }

    pub fn set_estimated_cost(&mut self, val: f64) {
        self.base.estimatedCost = val;
    }

    /// Requires SQLite 3.8.2.
    pub fn estimated_rows(&self) -> Result<i64> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_008_002)?;
                Ok(self.base.estimatedRows)
            },
            _ => Err(Error::VersionNotSatisfied(3_008_002))
        }
    }

    /// Requires SQLite 3.8.2.
    pub fn set_estimated_rows(&mut self, val: i64) -> Result<()> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_008_002)?;
                self.base.estimatedRows = val;
                Ok(())
            },
            _ => {
                let _ = val;
                Err(Error::VersionNotSatisfied(3_008_002))
            }
        }
    }

    /// Requires SQLite 3.9.0.
    pub fn scan_flags(&self) -> Result<usize> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_009_000)?;
                Ok(self.base.idxFlags as _)
            },
            _ => Err(Error::VersionNotSatisfied(3_009_000))
        }
    }

    /// Requires SQLite 3.9.0.
    pub fn set_scan_flags(&mut self, val: usize) -> Result<()> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_009_000)?;
                self.base.idxFlags = val as _;
                Ok(())
            },
            _ => {
                let _ = val;
                Err(Error::VersionNotSatisfied(3_009_000))
            }
        }
    }

    /// Requires SQLite 3.10.0.
    pub fn columns_used(&self) -> Result<u64> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_010_000)?;
                Ok(self.base.colUsed)
            },
            _ => Err(Error::VersionNotSatisfied(3_010_000))
        }
    }
}

impl IndexInfoConstraint {
    pub fn column(&self) -> isize {
        self.base.iColumn as _
    }

    pub fn op(&self) -> u8 {
        self.base.op
    }

    pub fn usable(&self) -> bool {
        self.base.usable != 0
    }
}

impl IndexInfoOrderBy {
    pub fn column(&self) -> isize {
        self.base.iColumn as _
    }

    pub fn desc(&self) -> bool {
        self.base.desc != 0
    }
}

impl IndexInfoConstraintUsage {
    pub fn argv_index(&self) -> usize {
        self.base.argvIndex as _
    }

    pub fn omit(&self) -> bool {
        self.base.omit != 0
    }
}

impl std::fmt::Debug for IndexInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfo")
            .field("constraints", &self.constraints())
            .field("order_by", &self.order_by())
            .field("constraint_usage", &self.constraint_usage())
            .field("index_num", &self.index_num())
            .field("index_str", &self.index_str())
            .field("order_by_consumed", &self.order_by_consumed())
            .field("estimated_cost", &self.estimated_cost())
            .field("estimated_rows", &self.estimated_rows())
            .field("scan_flags", &self.scan_flags())
            .field("columns_used", &self.columns_used())
            .finish()
    }
}

impl std::fmt::Debug for IndexInfoConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfoConstraint")
            .field("column", &self.column())
            .field("op", &self.op())
            .field("usable", &self.usable())
            .finish()
    }
}

impl std::fmt::Debug for IndexInfoOrderBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfoOrderBy")
            .field("column", &self.column())
            .field("desc", &self.desc())
            .finish()
    }
}

impl std::fmt::Debug for IndexInfoConstraintUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfoConstraintUsage")
            .field("argv_index", &self.argv_index())
            .field("omit", &self.omit())
            .finish()
    }
}
