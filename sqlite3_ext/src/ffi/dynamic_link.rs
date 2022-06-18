use super::super::Error;
pub use super::sqlite3ext::*;
use std::{os::raw::c_char, ptr, sync::Once};

static API_READY: Once = Once::new();
static mut API: *mut sqlite3_api_routines = ptr::null_mut();

pub fn is_ready() -> bool {
    API_READY.is_completed()
}

pub fn str_to_sqlite3(val: &str) -> Result<*mut c_char, Error> {
    let len: usize = val
        .len()
        .checked_add(1)
        .ok_or(Error::OutOfMemory(val.len()))?;
    unsafe {
        let ptr: *mut c_char = sqlite3_malloc64(len as _) as _;
        if !ptr.is_null() {
            ptr::copy_nonoverlapping(val.as_ptr(), ptr as _, len as _);
            *ptr.add(len - 1) = 0;
            Ok(ptr)
        } else {
            Err(Error::OutOfMemory(len))
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/sqlite3_api_routines.rs"));
