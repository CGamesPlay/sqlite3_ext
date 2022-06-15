#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

mod sqlite3ext;

pub use sqlite3ext::*;

pub static mut API: *mut sqlite3_api_routines = std::ptr::null_mut();

pub fn sqlite3_str(val: &str) -> Option<*mut ::std::os::raw::c_char> {
    let len: i32 = (val.len() + 1).try_into().ok()?;
    let ptr: *mut std::os::raw::c_char = unsafe { ((*API).malloc.unwrap())(len) } as _;
    if !ptr.is_null() {
        unsafe { std::ptr::copy_nonoverlapping(val.as_ptr(), ptr as _, len as _) };
        Some(ptr)
    } else {
        println!("err_to_sqlite3_str(): sqlite3_malloc returned null");
        None
    }
}

include!(concat!(env!("OUT_DIR"), "/sqlite3_api_routines.rs"));
