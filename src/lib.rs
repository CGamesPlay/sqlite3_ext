mod sqlite3ext;

use sqlite3ext::*;
use std::ffi::CStr;

static mut API: *mut sqlite3_api_routines = std::ptr::null_mut();

#[no_mangle]
pub unsafe extern "C" fn sqlite3_crdb_init(
    _db: *mut sqlite3,
    _err_msg: *mut *mut std::os::raw::c_char,
    api: *mut sqlite3_api_routines,
) -> std::os::raw::c_uint {
    API = api;
    println!(
        "Extension loaded! SQLite {}",
        CStr::from_ptr((*api).libversion.unwrap()())
            .to_str()
            .unwrap()
    );
    SQLITE_OK
}
