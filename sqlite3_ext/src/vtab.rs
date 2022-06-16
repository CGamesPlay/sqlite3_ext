use super::{ffi, function::Context, types::*, value::Value, Connection};
use std::{
    ffi::{CStr, CString},
    marker::PhantomData,
    os::raw::{c_int, c_void},
    ptr, slice,
};

const EMPTY_MODULE: ffi::sqlite3_module = ffi::sqlite3_module {
    iVersion: 2,
    xCreate: None,
    xConnect: None,
    xBestIndex: None,
    xDisconnect: None,
    xDestroy: None,
    xOpen: None,
    xClose: None,
    xFilter: None,
    xNext: None,
    xEof: None,
    xColumn: None,
    xRowid: None,
    xUpdate: None,
    xBegin: None,
    xSync: None,
    xCommit: None,
    xRollback: None,
    xFindFunction: None,
    xRename: None,
    xSavepoint: None,
    xRelease: None,
    xRollbackTo: None,
};

// xCreate = NULL
//     eponymous-only virtual table. create is forbidden, requires SQLite 3.9.0
// xCreate = xConnect
//     eponymous virtual table. create is allowed to instance the table with different
//     parameters, but the default is also available always
// xCreate different from xConnect
//     normal virtual table, cannot be used without a create
// all of these can be read-only or not.

// eponymous-only read-only | VTab                    |
// eponymous-only updatable | UpdateVTab              |
// eponymous read-only      | VTab                    |
// eponymous updatable      | UpdateVTab              |
// normal read-only         | CreateVTab              |
// normal updatable         | UpdateVTab + CreateVTab |

/// Functionality required by all virtual tables. A read-only, eponymous-only virtual table
/// (e.g. a table-valued function) can implement only this trait.
pub trait VTab<'vtab>: Sized {
    type Aux;
    type Cursor: VTabCursor;

    /// Corresponds to xConnect. The virtual table implementation will return an error if
    /// any of the arguments contain invalid UTF-8.
    fn connect(
        db: &mut Connection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)>;

    /// Corrrsponds to xBestIndex. If best_index returns `Err(Error::ConstraintViolation)`,
    /// then xBestIndex will return `SQLITE_CONSTRAINT`. Any other error will cause
    /// xBestIndex to fail.
    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()>;

    fn open(&'vtab mut self) -> Result<Self::Cursor>;
}

/// A virtual table that has xCreate and xDestroy methods.
pub trait CreateVTab<'vtab>: VTab<'vtab> {
    /// Corresponds to xCreate. The virtual table implementation will return an error if
    /// any of the arguments contain invalid UTF-8.
    fn create(
        db: &mut Connection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)>;

    fn destroy(&mut self) -> Result<()>;
}

/// A virtual table that supports INSERT/UPDATE/DELETE.
pub trait UpdateVTab<'vtab>: VTab<'vtab> {
    fn update(&mut self);
}

/// Implementation of the cursor type for a virtual table.
pub trait VTabCursor {
    fn filter(&mut self, index_num: usize, index_str: Option<&str>, args: &[Value]) -> Result<()>;

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
    /// Declare an eponymous-only virtual table. For this module, CREATE VIRTUAL TABLE is
    /// forbidden. This requires SQLITE >= 3.9.0.
    pub fn eponymous_only() -> Result<Module<'vtab, T>> {
        ffi::require_version(3_009_000)?;
        Ok(Module {
            base: ffi::sqlite3_module {
                iVersion: 2,
                xConnect: Some(vtab_connect::<T>),
                xBestIndex: Some(vtab_best_index::<T>),
                xDisconnect: Some(vtab_disconnect::<T>),
                xOpen: Some(vtab_open::<T>),
                xClose: Some(vtab_close::<T>),
                xFilter: Some(vtab_filter::<T>),
                xNext: Some(vtab_next::<T>),
                xEof: Some(vtab_eof::<T>),
                xColumn: Some(vtab_column::<T>),
                xRowid: Some(vtab_rowid::<T>),
                ..EMPTY_MODULE
            },
            phantom: PhantomData,
        })
    }

    /// Declare an eponymous virtual table. For this module, the virtual table is available
    /// ambiently in the database, but CREATE VIRTUAL TABLE can also be used to instantiate
    /// the table with alternative parameters.
    pub fn eponymous() -> Module<'vtab, T> {
        Module {
            base: ffi::sqlite3_module {
                iVersion: 2,
                xCreate: Some(vtab_connect::<T>),
                xConnect: Some(vtab_connect::<T>),
                xBestIndex: Some(vtab_best_index::<T>),
                xDisconnect: Some(vtab_disconnect::<T>),
                xDestroy: Some(vtab_disconnect::<T>),
                xOpen: Some(vtab_open::<T>),
                xClose: Some(vtab_close::<T>),
                xFilter: Some(vtab_filter::<T>),
                xNext: Some(vtab_next::<T>),
                xEof: Some(vtab_eof::<T>),
                xColumn: Some(vtab_column::<T>),
                xRowid: Some(vtab_rowid::<T>),
                ..EMPTY_MODULE
            },
            phantom: PhantomData,
        }
    }
}

impl<'vtab, T: CreateVTab<'vtab>> Module<'vtab, T> {
    pub fn standard() -> Module<'vtab, T> {
        Module {
            base: ffi::sqlite3_module {
                iVersion: 2,
                xCreate: Some(vtab_create::<T>),
                xConnect: Some(vtab_connect::<T>),
                xBestIndex: Some(vtab_best_index::<T>),
                xDisconnect: Some(vtab_disconnect::<T>),
                xDestroy: Some(vtab_destroy::<T>),
                xOpen: Some(vtab_open::<T>),
                xClose: Some(vtab_close::<T>),
                xFilter: Some(vtab_filter::<T>),
                xNext: Some(vtab_next::<T>),
                xEof: Some(vtab_eof::<T>),
                xColumn: Some(vtab_column::<T>),
                xRowid: Some(vtab_rowid::<T>),
                ..EMPTY_MODULE
            },
            phantom: PhantomData,
        }
    }
}

macro_rules! vtab_connect {
    ($name:ident, $trait:ident, $func:ident) => {
        unsafe extern "C" fn $name<'vtab, T: $trait<'vtab> + 'vtab>(
            db: *mut ffi::sqlite3,
            module: *mut c_void,
            argc: i32,
            argv: *const *const i8,
            p_vtab: *mut *mut ffi::sqlite3_vtab,
            err_msg: *mut *mut i8,
        ) -> c_int {
            let mut conn: Connection = db.into();
            let module = ModuleHandle::<'vtab, T>::from_ptr(module);
            let args: Result<Vec<&str>> = slice::from_raw_parts(argv, argc as _)
                .into_iter()
                .map(|arg| {
                    CStr::from_ptr(*arg)
                        .to_str()
                        .map_err(|e| Error::Utf8Error(e))
                })
                .collect();
            let args = match args {
                Ok(x) => x,
                Err(e) => return ffi::handle_error(e, err_msg),
            };
            let ret = T::$func(
                &mut conn,
                module.map_or(None, |m| m.aux.as_ref()),
                args.as_slice(),
            );
            let (sql, vtab) = match ret {
                Ok(x) => x,
                Err(e) => return ffi::handle_error(e, err_msg),
            };
            let rc = ffi::declare_vtab(
                conn.db,
                CString::from_vec_unchecked(sql.into_bytes()).as_ptr() as _,
            );
            if rc != ffi::SQLITE_OK {
                return rc;
            }
            let vtab = Box::new(VTabHandle {
                base: ffi::sqlite3_vtab {
                    pModule: ptr::null_mut(),
                    nRef: 0,
                    zErrMsg: ptr::null_mut(),
                },
                vtab,
                phantom: PhantomData,
            });
            *p_vtab = Box::into_raw(vtab) as _;
            ffi::SQLITE_OK
        }
    };
}

vtab_connect!(vtab_create, CreateVTab, create);
vtab_connect!(vtab_connect, VTab, connect);

unsafe extern "C" fn vtab_best_index<'vtab, T: VTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    info: *mut ffi::sqlite3_index_info,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let info = &mut *(info as *mut IndexInfo);
    let ret = vtab.vtab.best_index(info);
    if let Err(Error::ConstraintViolation) = ret {
        ffi::SQLITE_CONSTRAINT
    } else {
        ffi::handle_result(ret, &mut vtab.base.zErrMsg)
    }
}

unsafe extern "C" fn vtab_open<'vtab, T: VTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    p_cursor: *mut *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let cursor = match vtab.vtab.open() {
        Ok(x) => x,
        Err(e) => return ffi::handle_error(e, &mut vtab.base.zErrMsg),
    };
    let cursor = Box::new(VTabCursorHandle::<'vtab, T> {
        base: ffi::sqlite3_vtab_cursor {
            pVtab: ptr::null_mut(),
        },
        cursor,
        phantom: PhantomData,
    });
    *p_cursor = Box::into_raw(cursor) as _;
    ffi::SQLITE_OK
}

unsafe extern "C" fn vtab_close<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let cursor: Box<VTabCursorHandle<T>> = Box::from_raw(cursor as _);
    std::mem::drop(cursor);
    ffi::SQLITE_OK
}

unsafe extern "C" fn vtab_disconnect<'vtab, T: VTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
) -> c_int {
    let vtab: Box<VTabHandle<T>> = Box::from_raw(vtab as _);
    std::mem::drop(vtab);
    ffi::SQLITE_OK
}

unsafe extern "C" fn vtab_destroy<'vtab, T: CreateVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    match vtab.vtab.destroy() {
        Ok(_) => {
            let vtab: Box<VTabHandle<T>> = Box::from_raw(vtab as _);
            std::mem::drop(vtab);
            ffi::SQLITE_OK
        }
        Err(e) => ffi::handle_error(e, &mut vtab.base.zErrMsg),
    }
}

unsafe extern "C" fn vtab_filter<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
    index_num: i32,
    index_str: *const i8,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    let index_str = if index_str.is_null() {
        None
    } else {
        CStr::from_ptr(index_str).to_str().ok()
    };
    let args = slice::from_raw_parts(*argv as *mut Value, argc as _);
    ffi::handle_result(
        cursor.cursor.filter(index_num as _, index_str, args),
        &mut (*cursor.base.pVtab).zErrMsg,
    )
}

unsafe extern "C" fn vtab_next<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    ffi::handle_result(cursor.cursor.next(), &mut (*cursor.base.pVtab).zErrMsg)
}

unsafe extern "C" fn vtab_eof<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    cursor.cursor.eof() as _
}

unsafe extern "C" fn vtab_column<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
    context: *mut ffi::sqlite3_context,
    i: i32,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    let context = &mut *(context as *mut Context);
    ffi::handle_result(
        cursor.cursor.column(context, i as _),
        &mut (*cursor.base.pVtab).zErrMsg,
    )
}

unsafe extern "C" fn vtab_rowid<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
    ptr: *mut i64,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    match cursor.cursor.rowid() {
        Ok(x) => {
            *ptr = x;
            ffi::SQLITE_OK
        }
        Err(e) => ffi::handle_error(e, &mut (*cursor.base.pVtab).zErrMsg),
    }
}

/// Handle to the module and aux data, so that it can be properly dropped when the module is
/// unloaded.
pub(crate) struct ModuleHandle<'vtab, T: VTab<'vtab>> {
    pub vtab: Module<'vtab, T>,
    pub aux: Option<T::Aux>,
}

impl<'vtab, T: VTab<'vtab>> ModuleHandle<'vtab, T> {
    pub unsafe fn from_ptr<'a>(ptr: *mut c_void) -> Option<&'a ModuleHandle<'vtab, T>> {
        ptr.cast::<ModuleHandle<T>>().as_ref()
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
            unsafe { ffi::free(self.base.idxStr as _) };
        }
        match val {
            None => {
                self.base.idxStr = ptr::null_mut();
                self.base.needToFreeIdxStr = 0;
            }
            Some(x) => {
                self.base.idxStr = ffi::sqlite3_str(x)?;
                self.base.needToFreeIdxStr = 1;
            }
        }
        Ok(())
    }

    /// Set the index string without copying.
    pub fn set_index_str_static(&mut self, val: &'static CStr) {
        if self.base.needToFreeIdxStr != 0 {
            unsafe { ffi::free(self.base.idxStr as _) };
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

    pub fn estimated_rows(&self) -> Result<i64> {
        ffi::require_version(3_008_002)?;
        Ok(self.base.estimatedRows)
    }

    pub fn set_estimated_rows(&mut self, val: i64) -> Result<()> {
        ffi::require_version(3_008_002)?;
        self.base.estimatedRows = val;
        Ok(())
    }

    pub fn scan_flags(&self) -> Result<usize> {
        ffi::require_version(3_009_000)?;
        Ok(self.base.idxFlags as _)
    }

    pub fn set_scan_flags(&mut self, val: usize) -> Result<()> {
        ffi::require_version(3_009_000)?;
        self.base.idxFlags = val as _;
        Ok(())
    }

    pub fn columns_used(&self) -> Result<u64> {
        ffi::require_version(3_010_000)?;
        Ok(self.base.colUsed)
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
