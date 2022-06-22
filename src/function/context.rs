use super::super::ffi;
use std::ffi::CString;

#[repr(transparent)]
pub struct Context {
    base: ffi::sqlite3_context,
}

impl Context {
    pub fn as_ptr(&self) -> *mut ffi::sqlite3_context {
        &self.base as *const ffi::sqlite3_context as _
    }

    pub fn set_result<T: ToContextResult>(&mut self, val: T) {
        val.assign_to(self);
    }
}

pub trait ToContextResult {
    fn assign_to(self, context: &mut Context);
}

macro_rules! to_context {
    ($ty:ty as ($ctx:ident, $val:ident) => $impl:expr) => {
        impl ToContextResult for $ty {
            fn assign_to(self, context: &mut Context) {
                let $ctx = context.as_ptr() as _;
                let $val = self;
                unsafe { $impl }
            }
        }
    };
}

to_context!(i32 as (ctx, val) => ffi::sqlite3_result_int(ctx, val));
to_context!(i64 as (ctx, val) => ffi::sqlite3_result_int64(ctx, val));
to_context!(&str as (ctx, val) => {
    let val = val.as_bytes();
    let len = val.len();
    ffi::sqlite3_result_text(ctx, val.as_ptr() as _, len as _, None);
});
to_context!(String as (ctx, val) => {
    let val = val.as_bytes();
    let len = val.len();
    let cstring = CString::new(val).unwrap().into_raw();
    ffi::sqlite3_result_text(ctx, cstring, len as _, Some(ffi::drop_cstring));
});

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Context").finish()
    }
}
