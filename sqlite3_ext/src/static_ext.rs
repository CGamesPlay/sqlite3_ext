//! Helpers for when this extension is intended to be statically linked into a Rust program,
//! rather than being dynamically loaded.
#![cfg(feature = "static")]

use super::*;
use std::ffi::c_void;

/// Register the provided function to be called by each new database connection.
pub fn sqlite3_auto_extension(
    init: unsafe extern "C" fn(
        *mut ffi::sqlite3,
        *mut *mut std::os::raw::c_char,
        *mut ffi::sqlite3_api_routines,
    ) -> std::os::raw::c_int,
) -> Result<()> {
    let rc = unsafe {
        let init: unsafe extern "C" fn() = std::mem::transmute(init as *mut c_void);
        libsqlite3_sys::sqlite3_auto_extension(Some(init))
    };
    Error::from_sqlite(rc)
}

impl Connection {
    /// Convert a rusqlite::Connection to an sqlite3_ext::Connection.
    pub fn from_rusqlite(conn: &rusqlite::Connection) -> &mut Self {
        unsafe { Connection::from_ptr(conn.handle()) }
    }
}

impl From<Error> for rusqlite::Error {
    fn from(e: Error) -> Self {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ffi::ErrorCode::Unknown,
                extended_code: ffi::SQLITE_ERROR,
            },
            Some(format!("{}", e)),
        )
    }
}
