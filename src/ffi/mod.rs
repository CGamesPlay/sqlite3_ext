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

/// We have to do this trampoline construct because the cfg attributes are evaluated in the
/// context of the transcribed crate.
#[cfg(modern_sqlite)]
#[macro_export]
#[doc(hidden)]
macro_rules! sqlite3_match_version_trampoline {
    ($($tail:tt)*) => { $crate::sqlite3_match_version!(@modern () $($tail)*) };
}

#[cfg(not(modern_sqlite))]
#[macro_export]
#[doc(hidden)]
macro_rules! sqlite3_match_version_trampoline {
    ($($tail:tt)*) => { $crate::sqlite3_match_version!(@old $($tail)*) };
}

/// Selectively enable features which require a particular SQLite version.
///
/// This macro mimics a match expression, except each pattern is a minimum supported version
/// rather than an exact match. It performs a check for the given SQLite version both at
/// compile time and at runtime. If both checks pass, the expression is evaluated, otherwise
/// the following match arms are checked.
///
/// The minimum supported version of SQLite is 3.6.8. It is a compile error to attempt to match
/// against an older version of SQLite using this macro (this helps avoid typos where digits
/// are accidentally omitted from a version number).
///
/// This macro is particularly useful when interacting with ffi methods, since these may be
/// missing on older versions of SQLite, which would cause a compilation error.
///
/// A fallback arm is always required when using this macro. For cases where no fallback is
/// possible, use [sqlite3_require_version].
///
/// # Examples
///
/// ```no_run
/// use sqlite3_ext::{sqlite3_match_version, ffi};
/// use std::ffi::c_void;
///
/// fn alloc_memory_with_sqlite3(len: usize) -> *mut c_void {
///     unsafe {
///         sqlite3_match_version! {
///             3_008_007 => ffi::sqlite3_malloc64(len as _),
///             _ => ffi::sqlite3_malloc(len as _),
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! sqlite3_match_version {
    // Comma optional: version => { block }
    (@modern ($($body:tt)*) $ver:literal => { $($block:tt)* } $($tail:tt)* ) => {
        $crate::sqlite3_match_version!(
            @modern ( $($body)* $ver.. => {
                $crate::sqlite3_match_version!(@verify $ver);
                $($block)*
            })
            $($tail)*
        )
    };
    (@old $ver:literal => { $($block:tt)* } $($tail:tt)* ) => {{
        $crate::sqlite3_match_version!(@verify $ver);
        $crate::sqlite3_match_version!(@old $($tail)*)
    }};

    // Comma required: version => expr,
    (@modern ($($body:tt)*) $ver:literal => $expr:expr, $($tail:tt)* ) => {
        $crate::sqlite3_match_version!(
            @modern ( $($body)* $ver.. => {
                $crate::sqlite3_match_version!(@verify $ver);
                $expr
            })
            $($tail)*
        )
    };
    (@old $ver:literal => $expr:expr, $($tail:tt)* ) => {{
        $crate::sqlite3_match_version!(@verify $ver);
        $crate::sqlite3_match_version!(@old $($tail)*)
    }};

    // Comma missing (no fallback): version => expr
    (@modern ($($body:tt)*) $ver:literal => $expr:expr ) => {
        compile_error!("non-exhaustive patterns: missing a wildcard pattern");
    };
    (@old $ver:literal => $expr:expr ) => {
        compile_error!("non-exhaustive patterns: missing a wildcard pattern");
    };

    // Finish the match with a fallback
    (@modern ($($body:tt)*) _ => $expr:expr $(,)? ) => {
        match $crate::SQLITE_VERSION.as_i32() {
            $($body)*
            _ => $expr
        }
    };
    (@old _ => $expr:expr $(,)? ) => {
        $expr
    };

    // Strip a leading comma
    (@modern ($($body:tt)*) , $($tail:tt)* ) => {
        $crate::sqlite3_match_version!(@modern ( $($body)* ) $($tail)*)
    };
    (@old , $($tail:tt)* ) => {
        $crate::sqlite3_match_version!(@old $($tail)*)
    };

    (@verify $version:literal) => {
        /// Static assertions to verify that there are no mising/extra digits in the
        /// version number.
        #[cfg(debug_assertions)]
        const _: () = {
            assert!($version >= 3_006_008, stringify!($version is earlier than 3.6.8 (the minimum supported version of SQLite)));
            assert!($version < 4_000_000, stringify!($version is newer than 4.0.0 (which is not a valid version of SQLite3)));
        };
    };

    // Base case, with a guard that it has to look like the start of a match
    ( $x:literal => $($tail:tt)* ) => {
        $crate::sqlite3_match_version_trampoline!($x => $($tail)*)
    };
}

/// Guard an expression behind an SQLite version.
///
/// This macro evaluates the SQLite version at compile time and at runtime. If both checks
/// pass, the provided expression is evaluated. Otherwise, the macro evaluates to
/// [Error::VersionNotSatisfied](crate::Error::VersionNotSatisfied).
///
/// This macro is particularly useful when interacting with ffi methods, since these may be
/// missing on older versions of SQLite, which would cause a compilation error.
///
/// # Examples
///
/// ```no_run
/// use sqlite3_ext::{*, ffi};
/// use std::ffi::CStr;
///
/// pub fn sourceid() -> Result<&'static str> {
///     sqlite3_require_version!(3_021_000, {
///         let ret = unsafe { CStr::from_ptr(ffi::sqlite3_sourceid()) };
///         Ok(ret.to_str().expect("sqlite3_sourceid"))
///     })
/// }
/// ```
#[macro_export]
macro_rules! sqlite3_require_version {
    ($version:literal, $expr:expr) => {
        $crate::sqlite3_match_version! {
            $version => {
                let ret: Result<_> = $expr;
                ret
            }
            _ => Err(Error::VersionNotSatisfied($version)),
        }
    };
}

pub fn str_to_sqlite3(val: &str) -> Result<*mut c_char, Error> {
    let len: usize = val.len().checked_add(1).ok_or(crate::types::SQLITE_NOMEM)?;
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
            Err(crate::types::SQLITE_NOMEM)
        }
    }
}

pub unsafe fn handle_error(err: impl Into<Error>, msg: *mut *mut c_char) -> c_int {
    err.into().into_sqlite(msg)
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

#[cfg(test)]
mod test {
    use crate::sqlite3_match_version;

    fn test_patterns() {
        let s = sqlite3_match_version! {
            3_008_008 => "expr,",
            3_008_007 => { "{expr}" }
            3_008_006 => { "{expr}," },
            _ => "fall,",
        };
        assert_eq!(s, "expr,");
        let s = sqlite3_match_version! {
            3_008_006 => "expr,",
            _ => "fall"
        };
        assert_eq!(s, "expr,");
    }
}
