//! Helpers for when this extension is intended to be statically linked into a Rust program,
//! rather than being dynamically loaded.
#![cfg(feature = "static")]

use super::*;

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

impl From<&rusqlite::Connection> for Connection {
    /// Convert a rusqlite::Connection to an sqlite3_ext::Connection.
    ///
    /// # Panics
    ///
    /// This method will panic if the sqlite3_ext API has not been initialized, see
    /// [sqlite3_auto_extension].
    fn from(conn: &rusqlite::Connection) -> Self {
        unsafe {
            Connection {
                db: conn.handle() as _,
            }
        }
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
