#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use super::Error;
pub use sqlite3ext::*;
use std::{
    os::raw::{c_char, c_int},
    ptr,
    sync::Once,
};

mod sqlite3ext;

static API_READY: Once = Once::new();
static mut API: *mut sqlite3_api_routines = ptr::null_mut();

pub fn is_ready() -> bool {
    API_READY.is_completed()
}

pub fn sqlite3_str(val: &str) -> Result<*mut c_char, Error> {
    let len: usize = val
        .len()
        .checked_add(1)
        .ok_or(Error::OutOfMemory(val.len()))?;
    unsafe {
        let ptr: *mut c_char = malloc64(len as _) as _;
        if !ptr.is_null() {
            ptr::copy_nonoverlapping(val.as_ptr(), ptr as _, len as _);
            *ptr.add(len - 1) = 0;
            Ok(ptr)
        } else {
            Err(Error::OutOfMemory(len))
        }
    }
}

pub unsafe fn handle_error(err: Error, msg: *mut *mut c_char) -> c_int {
    if let Error::Sqlite(code) = err {
        if code != SQLITE_OK && code != SQLITE_ROW && code != SQLITE_DONE {
            return code;
        }
    }
    if let Ok(s) = sqlite3_str(&format!("{}", err)) {
        *msg = s;
    }
    SQLITE_ERROR
}

pub unsafe fn handle_result(result: Result<(), Error>, msg: *mut *mut c_char) -> c_int {
    match result {
        Ok(_) => SQLITE_OK,
        Err(e) => handle_error(e, msg),
    }
}

pub fn is_version(min: c_int) -> bool {
    let found = unsafe { libversion_number() };
    found >= min
}

pub fn require_version(min: c_int) -> Result<(), Error> {
    if !is_version(min) {
        Err(Error::VersionNotSatisfied(min))
    } else {
        Ok(())
    }
}

include!(concat!(env!("OUT_DIR"), "/sqlite3_api_routines.rs"));
