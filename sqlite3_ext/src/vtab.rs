use super::{ffi, types::*, Connection};
use std::{
    ffi::{CStr, CString},
    os::raw::{c_int, c_void},
    ptr, slice,
};

/// Eponymous-only virtual table
pub trait VTab: Sized {
    type Aux;

    /// Corresponds to xConnect. The virtual table implementation will return an error if
    /// any of the arguments contain invalid UTF-8.
    fn connect(
        db: &mut Connection,
        aux: Option<&Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)>;

    /// Corrrsponds to xBestIndex. If best_index returns `Err(Error::ConstraintViolation)`,
    /// then xBestIndex will return `SQLITE_CONSTRAINT`. Any other error will cause
    /// xBestIndex to fail.
    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()>;

    fn open(&self);
}

/// Implementation of the cursor type for a virtual table.
pub trait Cursor {
    fn filter(&self);
    fn next(&self);
    fn eof(&self);
    fn column(&self);
    fn rowid(&self);
}

#[repr(transparent)]
pub struct Module<T: VTab> {
    pub(crate) base: ffi::sqlite3_module,
    phantom: std::marker::PhantomData<T>,
}

pub fn eponymous_only_module<'vtab, T: VTab>() -> Module<T> {
    Module {
        base: ffi::sqlite3_module {
            iVersion: 2,
            xCreate: None,
            xConnect: Some(vtab_connect::<T>),
            xBestIndex: Some(vtab_best_index::<T>),
            xDisconnect: Some(vtab_disconnect::<T>),
            xDestroy: None,
            xOpen: Some(vtab_open::<T>),
            xClose: Some(vtab_close),
            xFilter: Some(vtab_filter),
            xNext: Some(vtab_next),
            xEof: Some(vtab_eof),
            xColumn: Some(vtab_column),
            xRowid: Some(vtab_rowid),
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
        },
        phantom: std::marker::PhantomData,
    }
}

unsafe extern "C" fn vtab_connect<T: VTab>(
    db: *mut ffi::sqlite3,
    module: *mut c_void,
    argc: i32,
    argv: *const *const i8,
    p_vtab: *mut *mut ffi::sqlite3_vtab,
    err_msg: *mut *mut i8,
) -> c_int {
    let mut conn: Connection = db.into();
    let module = ModuleHandle::<T>::from_ptr::<'_>(module);
    let args: std::result::Result<Vec<&str>, std::str::Utf8Error> =
        slice::from_raw_parts(argv, argc as _)
            .into_iter()
            .map(|arg| CStr::from_ptr(*arg).to_str())
            .collect();
    let args = match args {
        Ok(x) => x,
        Err(e) => return ffi::handle_error(e, err_msg),
    };
    let ret = T::connect(
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
    });
    *p_vtab = Box::into_raw(vtab) as _;
    ffi::SQLITE_OK
}

unsafe extern "C" fn vtab_best_index<T: VTab>(
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

unsafe extern "C" fn vtab_open<T: VTab>(
    vtab: *mut ffi::sqlite3_vtab,
    _cursor: *mut *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    todo!()
}

unsafe extern "C" fn vtab_close(_cursor: *mut ffi::sqlite3_vtab_cursor) -> c_int {
    todo!()
}

unsafe extern "C" fn vtab_disconnect<T: VTab>(vtab: *mut ffi::sqlite3_vtab) -> c_int {
    let vtab: Box<VTabHandle<T>> = Box::from_raw(vtab as _);
    std::mem::drop(vtab);
    ffi::SQLITE_OK
}

unsafe extern "C" fn vtab_filter(
    _cursor: *mut ffi::sqlite3_vtab_cursor,
    _index_num: i32,
    _index_name: *const i8,
    _argc: i32,
    _argv: *mut *mut ffi::sqlite3_value,
) -> c_int {
    todo!()
}

unsafe extern "C" fn vtab_next(_cursor: *mut ffi::sqlite3_vtab_cursor) -> c_int {
    todo!()
}

unsafe extern "C" fn vtab_eof(_cursor: *mut ffi::sqlite3_vtab_cursor) -> c_int {
    todo!()
}

unsafe extern "C" fn vtab_column(
    _cursor: *mut ffi::sqlite3_vtab_cursor,
    _context: *mut ffi::sqlite3_context,
    _i: i32,
) -> c_int {
    todo!()
}

unsafe extern "C" fn vtab_rowid(_cursor: *mut ffi::sqlite3_vtab_cursor, _ptr: *mut i64) -> c_int {
    todo!()
}

/// Handle to the module and aux data, so that it can be properly dropped when the module is
/// unloaded.
pub(crate) struct ModuleHandle<T: VTab> {
    pub vtab: Module<T>,
    pub aux: Option<T::Aux>,
}

impl<T: VTab> ModuleHandle<T> {
    pub unsafe fn from_ptr<'a>(ptr: *mut c_void) -> Option<&'a ModuleHandle<T>> {
        ptr.cast::<ModuleHandle<T>>().as_ref()
    }
}

impl<T: VTab> Drop for ModuleHandle<T> {
    fn drop(&mut self) {
        println!("Dropped the module");
    }
}

#[repr(C)]
struct VTabHandle<T: VTab> {
    base: ffi::sqlite3_vtab,
    vtab: T,
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
            None => self.base.idxStr = ptr::null_mut(),
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
