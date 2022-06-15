use crate::*;
use std::ffi::c_void;
use std::mem;

pub fn auto_register() {
    unsafe {
        let init_func: unsafe extern "C" fn() = mem::transmute(sqlite3_crdb_init as *mut c_void);
        libsqlite3_sys::sqlite3_auto_extension(Some(init_func));
    }
}
