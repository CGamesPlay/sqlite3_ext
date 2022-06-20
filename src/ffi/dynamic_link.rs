pub use super::sqlite3ext::*;
use std::ptr;

static mut API: *mut sqlite3_api_routines = ptr::null_mut();

include!(concat!(env!("OUT_DIR"), "/sqlite3_api_routines.rs"));
