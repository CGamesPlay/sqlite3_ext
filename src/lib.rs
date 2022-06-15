mod ffi;
mod rusqlite;
mod types;
mod vtab;

pub use crate::rusqlite::auto_register;
use ffi::*;
use std::ffi::CStr;
use types::*;

#[no_mangle]
pub unsafe extern "C" fn sqlite3_crdb_init(
    db: *mut sqlite3,
    err_msg: *mut *mut std::os::raw::c_char,
    api: *mut sqlite3_api_routines,
) -> std::os::raw::c_int {
    init_api_routines(api);
    match crdb_init(db) {
        Ok(_) => SQLITE_OK,
        Err(err) => {
            if let Some(ptr) = sqlite3_str(&err.to_string()) {
                *err_msg = ptr;
            }
            SQLITE_ERROR
        }
    }
}

unsafe fn crdb_init(db: *mut sqlite3) -> Result<()> {
    vtab::create_module(db)?;
    println!(
        "Extension loaded! SQLite {}",
        CStr::from_ptr(sqlite3_libversion()).to_str().unwrap()
    );
    Ok(())
}
