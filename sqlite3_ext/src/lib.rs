pub use extension::Extension;
pub use sqlite3_ext_macro::*;
use std::ffi::{c_void, CStr, CString};
pub use types::*;
pub use value::*;

mod extension;
pub mod ffi;
pub mod function;
pub mod static_ext;
pub mod types;
pub mod value;
pub mod vtab;

pub fn sqlite3_libversion() -> &'static str {
    let ret = unsafe { CStr::from_ptr(ffi::sqlite3_libversion()) };
    ret.to_str().expect("sqlite3_libversion")
}

pub struct Connection {
    db: *mut ffi::sqlite3,
}

impl Connection {
    pub fn create_module<'vtab, T: vtab::VTab<'vtab> + 'vtab>(
        &self,
        name: &str,
        vtab: vtab::Module<'vtab, T>,
        aux: Option<T::Aux>,
    ) -> Result<()> {
        let name = CString::new(name).unwrap();
        let handle = Box::new(vtab::ModuleHandle { vtab, aux });
        let rc = unsafe {
            ffi::sqlite3_create_module_v2(
                self.db,
                name.as_ptr() as _,
                &handle.vtab.base,
                Box::into_raw(handle) as _,
                Some(drop_boxed::<vtab::ModuleHandle<T>>),
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

unsafe extern "C" fn drop_boxed<T>(data: *mut c_void) {
    let aux: Box<T> = Box::from_raw(data as _);
    std::mem::drop(aux);
}
