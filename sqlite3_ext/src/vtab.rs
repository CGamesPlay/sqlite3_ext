use super::ffi;
use std::os::raw::{c_int, c_void};

#[repr(transparent)]
pub struct Module<T: VTab> {
    pub(crate) base: ffi::sqlite3_module,
    phantom: std::marker::PhantomData<T>,
}

impl<T: VTab> Drop for Module<T> {
    fn drop(&mut self) {
        panic!("dropped module");
    }
}

pub fn eponymous_only_module<'vtab, T: VTab>() -> Module<T> {
    Module {
        base: ffi::sqlite3_module {
            iVersion: 2,
            xCreate: None,
            xConnect: Some(vtab_connect),
            xBestIndex: Some(vtab_best_index),
            xDisconnect: Some(vtab_disconnect),
            xDestroy: None,
            xOpen: Some(vtab_open),
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

unsafe extern "C" fn vtab_create(
    _db: *mut ffi::sqlite3,
    _data: *mut c_void,
    _argc: i32,
    _argv: *const *const i8,
    _vtab: *mut *mut ffi::sqlite3_vtab,
    _err: *mut *mut i8,
) -> c_int {
    println!("CALLED xCreate");
    todo!()
}

unsafe extern "C" fn vtab_connect(
    _db: *mut ffi::sqlite3,
    _data: *mut c_void,
    _argc: i32,
    _argv: *const *const i8,
    _vtab: *mut *mut ffi::sqlite3_vtab,
    _err: *mut *mut i8,
) -> c_int {
    println!("CALLED xConnect");
    todo!()
}

unsafe extern "C" fn vtab_best_index(
    _vtab: *mut ffi::sqlite3_vtab,
    _index_info: *mut ffi::sqlite3_index_info,
) -> c_int {
    todo!()
}

unsafe extern "C" fn vtab_open(
    _vtab: *mut ffi::sqlite3_vtab,
    _cursor: *mut *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    todo!()
}

unsafe extern "C" fn vtab_close(_cursor: *mut ffi::sqlite3_vtab_cursor) -> c_int {
    todo!()
}

unsafe extern "C" fn vtab_disconnect(_vtab: *mut ffi::sqlite3_vtab) -> c_int {
    println!("CALLED xDisconnect");
    todo!()
}

unsafe extern "C" fn vtab_destroy(_vtab: *mut ffi::sqlite3_vtab) -> c_int {
    println!("CALLED xDestroy");
    todo!()
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

/// Eponymous-only virtual table
pub trait VTab {
    type Aux;
    fn connect(&self);
    fn best_index(&self);
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

/// Handle to the module and aux data, so that it can be properly dropped when the module is
/// unloaded.
pub(crate) struct ModuleHandle<T: VTab> {
    pub vtab: Module<T>,
    pub aux: Option<T::Aux>,
}
