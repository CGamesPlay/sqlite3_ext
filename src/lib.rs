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

    /// A convenience method which calls [Module::register](vtab::Module::register) on the
    /// vtab.
    pub fn create_module<'vtab, T: vtab::VTab<'vtab> + 'vtab>(
        &self,
        name: &str,
        vtab: impl vtab::Module<'vtab, T> + 'vtab,
        aux: Option<T::Aux>,
    ) -> Result<()> {
        vtab.register(self, name, aux)
    }

    fn as_ptr(&self) -> *mut ffi::sqlite3 {
        &self.db as *const ffi::sqlite3 as _
    }
}
