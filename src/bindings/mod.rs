use super::types::*;
use std::ffi::CStr;
pub use vtab::*;

pub mod ffi;
mod vtab;

pub fn sqlite3_libversion() -> &'static str {
    let ret = unsafe { CStr::from_ptr(ffi::libversion()) };
    ret.to_str().expect("sqlite3_libversion")
}

pub struct Connection {
    pub(crate) db: *mut ffi::sqlite3,
}

impl Connection {
    pub fn create_module(&self, vtab: VTabModule) -> Result<()> {
        vtab.register(self)
    }
}

impl From<*mut ffi::sqlite3> for Connection {
    fn from(db: *mut ffi::sqlite3) -> Connection {
        Connection { db }
    }
}
