//! Helpers for when this extension is intended to be statically linked into a Rust program,
//! rather than being dynamically loaded.
#![cfg(feature = "rusqlite")]

use super::*;

impl Connection {
    /// Convert a rusqlite::Connection to an sqlite3_ext::Connection.
    pub fn from_rusqlite(conn: &rusqlite::Connection) -> &Self {
        unsafe { Connection::from_ptr(conn.handle() as _) }
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
