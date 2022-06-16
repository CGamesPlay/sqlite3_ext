#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use super::Error;
pub use sqlite3ext::*;
use std::os::raw::{c_char, c_int};

mod sqlite3ext;

pub static mut API: *mut sqlite3_api_routines = std::ptr::null_mut();

pub fn sqlite3_str(val: &str) -> Result<*mut c_char, Error> {
    let len: u64 = (val.len() + 1)
        .try_into()
        .map_err(|_| Error::OutOfMemory(val.len() + 1))?;
    let ptr: *mut c_char = unsafe { malloc64(len) } as _;
    if !ptr.is_null() {
        unsafe { std::ptr::copy_nonoverlapping(val.as_ptr(), ptr as _, len as _) };
        Ok(ptr)
    } else {
        Err(Error::OutOfMemory(len as _))
    }
}

pub unsafe fn handle_error<E: std::error::Error>(err: E, msg: *mut *mut c_char) -> c_int {
    if let Ok(s) = sqlite3_str(&format!("{}", err)) {
        *msg = s;
    }
    SQLITE_ERROR
}

pub unsafe fn handle_result<E: std::error::Error>(
    result: Result<(), E>,
    msg: *mut *mut c_char,
) -> c_int {
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
