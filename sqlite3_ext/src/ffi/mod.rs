#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use super::Error;
#[cfg(not(feature = "static"))]
pub use dynamic_link::*;
#[cfg(feature = "static")]
pub use static_link::*;
use std::{
    os::raw::{c_char, c_int},
    ptr,
};

#[cfg(not(feature = "static"))]
mod dynamic_link;
mod sqlite3ext;
#[cfg(feature = "static")]
mod static_link;

#[cfg(any(not(feature = "static"), feature = "static_modern"))]
macro_rules! match_sqlite {
    (modern => $modern:expr , _ => $old:expr) => {
        $modern
    };
}
#[cfg(not(any(not(feature = "static"), feature = "static_modern")))]
macro_rules! match_sqlite {
    (modern => $modern:expr , _ => $old:expr) => {
        $old
    };
}

pub(crate) use match_sqlite;

pub fn str_to_sqlite3(val: &str) -> Result<*mut c_char, Error> {
    let len: usize = val
        .len()
        .checked_add(1)
        .ok_or(Error::OutOfMemory(val.len()))?;
    unsafe {
        let ptr: *mut c_char =
            match_sqlite!( modern => sqlite3_malloc64, _ => sqlite3_malloc )(len as _) as _;
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
    if let Ok(s) = str_to_sqlite3(&format!("{}", err)) {
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
    let found = unsafe { sqlite3_libversion_number() };
    found >= min
}

pub fn require_version(min: c_int) -> Result<(), Error> {
    if !is_version(min) {
        Err(Error::VersionNotSatisfied(min))
    } else {
        Ok(())
    }
}
