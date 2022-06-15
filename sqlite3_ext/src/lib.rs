use std::ffi::{c_void, CStr, CString};
pub use types::*;
pub use vtab::{eponymous_only_module, Module, VTab};

pub mod ffi;
pub mod types;
pub mod vtab;

pub fn sqlite3_libversion() -> &'static str {
    let ret = unsafe { CStr::from_ptr(ffi::libversion()) };
    ret.to_str().expect("sqlite3_libversion")
}

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

pub struct Connection {
    db: *mut ffi::sqlite3,
}

impl Connection {
    pub fn create_module<T: VTab>(
        &self,
        name: &str,
        vtab: Module<T>,
        aux: Option<T::Aux>,
    ) -> Result<()> {
        let name = CString::new(name).unwrap();
        let handle = Box::new(vtab::ModuleHandle { vtab, aux });
        let rc = unsafe {
            ffi::create_module_v2(
                self.db,
                name.as_ptr() as _,
                &handle.vtab.base,
                Box::into_raw(handle) as _,
                Some(drop_box::<vtab::ModuleHandle<T>>),
            )
        };
        match rc {
            ffi::SQLITE_OK => Ok(()),
            _ => Err(Error::Sqlite(rc)),
        }
    }
}

impl From<*mut ffi::sqlite3> for Connection {
    fn from(db: *mut ffi::sqlite3) -> Connection {
        Connection { db }
    }
}

unsafe extern "C" fn drop_box<T>(data: *mut c_void) {
    let aux: Box<T> = Box::from_raw(data as _);
    std::mem::drop(aux);
}
