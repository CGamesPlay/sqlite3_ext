#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use super::Error;
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

/// This macro can be used to selectively enable features which require a particular SQLite
/// version.
///
/// It is implemented as a macro so that when statically linking against Rusqlite with an old
/// SQLite version, the expression can be omitted to prevent a compilation failure.
///
/// # Examples
///
/// The single branch is expected to return a [Result](super::Result):
///
/// ```no_run
/// # fn main() -> sqlite3_ext::Result<()> {
/// sqlite3_ext::sqlite3_require_version!(3_008_000, {
///     // Do something only supported here
///     Ok(())
/// })
/// # }
/// ```
///
/// The three argument version specifies a fallback, and does not need to return a Result.
///
/// ```no_run
/// let query = sqlite3_ext::sqlite3_require_version!(3_020_000, "SELECT new_fast_query()", "SELECT old_slow_query()");
/// ```
#[macro_export]
#[cfg(any(not(feature = "static"), feature = "static_modern"))]
macro_rules! sqlite3_require_version {
    ($version:literal, $expr:expr) => {{
        const _: () = {
            if $version < 3_006_008 {
                panic!("the minimum supported version of SQLite is 3.6.8")
            }
        };
        if $crate::sqlite3_libversion_number() < $version {
            Err($crate::Error::VersionNotSatisfied($version))
        } else {
            $expr
        }
    }};

    ($version:literal, $expr:expr, $fallback:expr) => {{
        const _: () = {
            if $version < 3_006_008 {
                panic!("the minimum supported version of SQLite is 3.6.8")
            }
        };
        if $crate::sqlite3_libversion_number() < $version {
            $fallback
        } else {
            $expr
        }
    }};
}

// We are using the oldest supported version of SQLite, so nothing extra is supported.
#[macro_export]
#[cfg(not(any(not(feature = "static"), feature = "static_modern")))]
macro_rules! sqlite3_require_version {
    ($version:literal, $expr:expr) => {
        Err(Error::VersionNotSatisfied($version))
    };

    ($version:literal, $expr:expr, $fallback:expr) => {{
        $fallback
    }};
}

pub fn str_to_sqlite3(val: &str) -> Result<*mut c_char, Error> {
    let len: usize = val
        .len()
        .checked_add(1)
        .ok_or(Error::Sqlite(SQLITE_NOMEM))?;
    unsafe {
        let ptr: *mut c_char = sqlite3_require_version!(
            3_008_007,
            sqlite3_malloc64(len as _),
            sqlite3_malloc(len as _)
        ) as _;
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
    let data: Box<T> = Box::from_raw(data as _);
    std::mem::drop(data);
}

pub unsafe extern "C" fn drop_cstring(data: *mut c_void) {
    let data = CString::from_raw(data as _);
    std::mem::drop(data);
}
