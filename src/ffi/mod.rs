#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use crate::{value::Blob, Error};
#[cfg(not(feature = "static"))]
pub use dynamic_link::*;
#[cfg(feature = "static")]
pub use static_link::*;
use std::{
    ffi::{c_void, CString},
    os::raw::{c_char, c_int},
    ptr,
};

#[cfg(not(feature = "static"))]
mod dynamic_link;
mod sqlite3ext;
#[cfg(feature = "static")]
mod static_link;

/// Selectively enable features which require a particular SQLite version.
///
/// This macro performs a check for the given SQLite version both at compile time and at
/// runtime. If both checks pass, the expression is evaluated, otherwise the fallback is
/// evaluated. It is implemented as a macro so that when statically linking against Rusqlite
/// with an old SQLite version, the expression can be omitted to prevent a compilation failure.
///
/// # Examples
///
/// ```no_run
/// let query = sqlite3_ext::sqlite3_match_version! {
///     3_020_000 => "SELECT new_fast_query()",
///     _ => "SELECT old_slow_query()",
/// };
/// ```
#[macro_export]
#[cfg(modern_sqlite)]
macro_rules! sqlite3_match_version {
    ($($version:literal => $expr:expr,)* _ => $fallback:expr $(,)?) => {{
        match $crate::SQLITE_VERSION.get() {
            $(x if x >= $version => $expr,)*
            _ => $fallback,
        }
    }};
}

// We are using the oldest supported version of SQLite, so nothing extra is supported.
#[macro_export]
#[cfg(not(modern_sqlite))]
macro_rules! sqlite3_match_version {
    ($($version:literal => $expr:expr,)* _ => $fallback:expr $(,)?) => {{
        $fallback
    }};
}

pub fn str_to_sqlite3(val: &str) -> Result<*mut c_char, Error> {
    let len: usize = val
        .len()
        .checked_add(1)
        .ok_or(Error::Sqlite(SQLITE_NOMEM))?;
    unsafe {
        let ptr: *mut c_char = sqlite3_match_version! {
            3_008_007 => sqlite3_malloc64(len as _) as _,
            _ => sqlite3_malloc(len as _) as _,
        };
        if !ptr.is_null() {
            ptr::copy_nonoverlapping(val.as_ptr(), ptr as _, len as _);
            *ptr.add(len - 1) = 0;
            Ok(ptr)
        } else {
            Err(Error::Sqlite(SQLITE_NOMEM))
        }
    }
}

pub unsafe fn handle_error(err: impl Into<Error>, msg: *mut *mut c_char) -> c_int {
    let err = err.into();
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

pub unsafe extern "C" fn drop_boxed<T>(data: *mut c_void) {
    drop(Box::<T>::from_raw(data as _));
}

pub unsafe extern "C" fn drop_cstring(data: *mut c_void) {
    drop(CString::from_raw(data as _));
}

pub unsafe extern "C" fn drop_blob(data: *mut c_void) {
    drop(Blob::from_raw(data));
}
