pub use extension::Extension;
pub use sqlite3_ext_macro::*;
use std::ffi::CStr;
pub use types::*;
pub use value::*;

mod extension;
pub mod ffi;
pub mod function;
pub mod static_ext;
pub mod types;
pub mod value;
pub mod vtab;

pub fn sqlite3_libversion_number() -> i32 {
    unsafe { ffi::sqlite3_libversion_number() }
}

pub fn sqlite3_libversion() -> &'static str {
    let ret = unsafe { CStr::from_ptr(ffi::sqlite3_libversion()) };
    ret.to_str().expect("sqlite3_libversion")
}

#[repr(transparent)]
pub struct Connection {
    db: ffi::sqlite3,
}

impl Connection {
    pub unsafe fn from_ptr<'a>(db: *mut ffi::sqlite3) -> &'a mut Connection {
        &mut *(db as *mut Connection)
    }

    fn as_ptr(&self) -> *mut ffi::sqlite3 {
        &self.db as *const ffi::sqlite3 as _
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
