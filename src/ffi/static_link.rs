pub use super::sqlite3ext::sqlite3_api_routines;
pub use libsqlite3_sys::*;

pub unsafe fn init_api_routines(_api: *mut sqlite3_api_routines) {}
