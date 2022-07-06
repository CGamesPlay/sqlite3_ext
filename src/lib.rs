pub use extension::Extension;
pub use sqlite3_ext_macro::*;
use std::ffi::CStr;
pub use types::*;
pub use value::*;

mod extension;
pub mod ffi;
pub mod function;
pub mod stack_ref;
pub mod static_ext;
mod test_helpers;
mod types;
mod value;
pub mod vtab;

/// The version of SQLite.
pub struct SqliteVersion;

/// The version of SQLite. See [SqliteVersion] for details.
pub static SQLITE_VERSION: SqliteVersion = SqliteVersion;

impl SqliteVersion {
    /// Returns the numeric version of SQLite.
    ///
    /// The format of this value is the semantic version with a simple encoding: `major *
    /// 1000000 + minor * 1000 + patch`. For example, SQLite version 3.8.2 is encoded as
    /// `3_008_002`.
    pub fn as_i32(&self) -> i32 {
        unsafe { ffi::sqlite3_libversion_number() }
    }

    /// Returns the human-readable version of SQLite. Example: `"3.8.2"`.
    pub fn as_str(&self) -> &'static str {
        let ret = unsafe { CStr::from_ptr(ffi::sqlite3_libversion()) };
        ret.to_str().expect("sqlite3_libversion")
    }

    /// Returns a hash of the SQLite source code. The objective is to detect accidental and/or
    /// careless edits. A forger can subvert this feature.
    ///
    /// Requires SQLite 3.21.0.
    pub fn sourceid(&self) -> Result<&'static str> {
        sqlite3_require_version!(3_021_000, {
            let ret = unsafe { CStr::from_ptr(ffi::sqlite3_sourceid()) };
            Ok(ret.to_str().expect("sqlite3_sourceid"))
        })
    }
}

impl std::fmt::Display for SqliteVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

#[repr(transparent)]
pub struct Connection {
    db: ffi::sqlite3,
}

impl Connection {
    /// Convert an SQLite handle into a reference to Connection.
    ///
    /// # Safety
    ///
    /// The behavior of this method is undefined if the passed pointer is not valid.
    pub unsafe fn from_ptr<'a>(db: *mut ffi::sqlite3) -> &'a mut Connection {
        &mut *(db as *mut Connection)
    }

    /// Get the underlying SQLite handle.
    pub fn as_ptr(&self) -> *const ffi::sqlite3 {
        &self.db
    }

    /// Get the underlying SQLite handle, mutably.
    pub fn as_mut_ptr(&mut self) -> *mut ffi::sqlite3 {
        &self.db as *const _ as _
    }
}

/// Indicate the risk level for a function or virtual table.
///
/// It is recommended that all functions and virtual table implementations set a risk level,
/// but the default is [RiskLevel::Innocuous] if TRUSTED_SCHEMA=on and [RiskLevel::DirectOnly]
/// otherwise.
///
/// See [this discussion](https://www.sqlite.org/src/doc/latest/doc/trusted-schema.md) for more
/// details about the motivation and implications.
pub enum RiskLevel {
    /// An innocuous function or virtual table is one that can only read content from the
    /// database file in which it resides, and can only alter the database in which it
    /// resides.
    Innocuous,
    /// A direct-only function or virtual table has side-effects that go outside the
    /// database file in which it lives, or return information from outside of the database
    /// file.
    DirectOnly,
}
