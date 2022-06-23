use super::super::{ffi, types::*};
use std::{
    ffi::CString,
    mem::{size_of, MaybeUninit},
};

#[repr(transparent)]
pub struct Context {
    base: ffi::sqlite3_context,
}

struct AggregateContext<T> {
    init: bool,
    val: MaybeUninit<T>,
}

impl Context {
    pub fn as_ptr(&self) -> *mut ffi::sqlite3_context {
        &self.base as *const ffi::sqlite3_context as _
    }

    pub fn set_result(&mut self, val: impl ToContextResult) {
        val.assign_to(self);
    }

    /// Get the aggregate context, returning a mutable reference to it.
    pub(crate) unsafe fn aggregate_context<T: Default>(&mut self) -> Result<&mut T> {
        let ptr =
            ffi::sqlite3_aggregate_context(self.as_ptr(), size_of::<AggregateContext<T>>() as _)
                as *mut AggregateContext<T>;
        if ptr.is_null() {
            return Err(Error::no_memory());
        }
        let context = &mut *ptr;
        if !context.init {
            context.val = MaybeUninit::new(T::default());
            context.init = true;
        }
        Ok(context.val.assume_init_mut())
    }

    /// Try to get the aggregate context, consuming it if it is found.
    pub(crate) unsafe fn try_aggregate_context<T: Default>(&mut self) -> Option<T> {
        let ptr = ffi::sqlite3_aggregate_context(self.as_ptr(), 0 as _) as *mut AggregateContext<T>;
        if ptr.is_null() {
            return None;
        }
        let context = &mut *ptr;
        if !context.init {
            None
        } else {
            context.init = false;
            Some(context.val.assume_init_read())
        }
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

to_context!(() as (ctx, _val) => ffi::sqlite3_result_null(ctx));
to_context!(i32 as (ctx, val) => ffi::sqlite3_result_int(ctx, val));
to_context!(i64 as (ctx, val) => ffi::sqlite3_result_int64(ctx, val));
to_context!(&'static str as (ctx, val) => {
    let val = val.as_bytes();
    let len = val.len();
    ffi::sqlite3_result_text64(ctx, val.as_ptr() as _, len as _, None, ffi::SQLITE_UTF8 as _);
});
to_context!(String as (ctx, val) => {
    let val = val.as_bytes();
    let len = val.len();
    let cstring = CString::new(val).unwrap().into_raw();
    ffi::sqlite3_result_text64(ctx, cstring, len as _, Some(ffi::drop_cstring), ffi::SQLITE_UTF8 as _);
});
to_context!(Error as (ctx, err) => {
    if let Error::Sqlite(code) = err {
        ffi::sqlite3_result_error_code(ctx, code);
    } else {
        let msg = format!("{}", err);
        let msg = msg.as_bytes();
        let len = msg.len();
        ffi::sqlite3_result_error(ctx, msg.as_ptr() as _, len as _);
    }
});

impl<T: ToContextResult> ToContextResult for Option<T> {
    fn assign_to(self, context: &mut Context) {
        match self {
            Some(x) => x.assign_to(context),
            None => ().assign_to(context),
        }
    }
}

impl<T: ToContextResult> ToContextResult for Result<T> {
    fn assign_to(self, context: &mut Context) {
        match self {
            Ok(x) => x.assign_to(context),
            Err(x) => x.assign_to(context),
        }
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Context").finish()
    }
}
