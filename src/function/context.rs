use super::{
    super::{ffi, sqlite3_require_version, types::*, value::*, Connection},
    FnUserData,
};
use std::{
    ffi::CString,
    marker::PhantomData,
    mem::{size_of, MaybeUninit},
};

#[repr(transparent)]
pub(crate) struct InternalContext {
    base: ffi::sqlite3_context,
}

/// Describes the run-time environment of an application-defined function.
#[repr(transparent)]
pub struct Context<UserData> {
    base: ffi::sqlite3_context,
    phantom: PhantomData<UserData>,
}

struct AggregateContext<T> {
    init: bool,
    val: MaybeUninit<T>,
}

impl InternalContext {
    pub unsafe fn from_ptr<'a>(base: *mut ffi::sqlite3_context) -> &'a mut Self {
        &mut *(base as *mut Self)
    }

    pub fn as_ptr(&self) -> *mut ffi::sqlite3_context {
        &self.base as *const ffi::sqlite3_context as _
    }

    pub fn set_result(&mut self, val: impl ToContextResult) {
        unsafe { val.assign_to(self.as_ptr()) };
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

impl<U> Context<U> {
    pub(crate) unsafe fn from_ptr<'a>(base: *mut ffi::sqlite3_context) -> &'a mut Self {
        &mut *(base as *mut Self)
    }

    /// Return a handle to the current database.
    pub fn db(&self) -> &Connection {
        unsafe {
            Connection::from_ptr(ffi::sqlite3_context_db_handle(
                &self.base as *const ffi::sqlite3_context as _,
            ))
        }
    }

    /// Return the data associated with this function.
    ///
    /// This method returns a reference to the value originally passed when this function
    /// was created.
    pub fn user_data(&self) -> &U {
        let user_data = unsafe {
            let ctx = &self.base as *const ffi::sqlite3_context as _;
            &*(ffi::sqlite3_user_data(ctx) as *const FnUserData<U>)
        };
        &user_data.user_data
    }
}

/// A value that can be returned from an SQL function.
///
/// For functions which have an output type determined at runtime, [Value] is implemented. For
/// nullable types, Option\<ToContextResult\> is implemented. For fallible functions,
/// [Result]\<ToContextResult\> is implemented.
pub trait ToContextResult {
    #[doc(hidden)]
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context);
}

macro_rules! to_context_result {
    ($($(#[$attr:meta])* match $ty:ty as ($ctx:ident, $val:ident) => $impl:expr),*) => {
        $(
        $(#[$attr])*
        impl ToContextResult for $ty {
            unsafe fn assign_to(self, $ctx: *mut ffi::sqlite3_context) {
                let $val = self;
                $impl
            }
        }
        )*
    };
}

to_context_result! {
    /// Assign NULL to the context result.
    match () as (ctx, _val) => ffi::sqlite3_result_null(ctx),
    match i32 as (ctx, val) => ffi::sqlite3_result_int(ctx, val),
    match i64 as (ctx, val) => ffi::sqlite3_result_int64(ctx, val),
    match f64 as (ctx, val) => ffi::sqlite3_result_double(ctx, val),
    /// Assign a static string to the context result.
    match &'static str as (ctx, val) => {
        let val = val.as_bytes();
        let len = val.len();
        sqlite3_require_version!(3_008_007, {
            ffi::sqlite3_result_text64(ctx, val.as_ptr() as _, len as _, None, ffi::SQLITE_UTF8 as _)
        }, {
            ffi::sqlite3_result_text(ctx, val.as_ptr() as _, len as _, None)
        });
    },
    /// Assign an owned string to the context result.
    match String as (ctx, val) => {
        let val = val.as_bytes();
        let len = val.len();
        let cstring = CString::new(val).unwrap().into_raw();
        sqlite3_require_version!(3_008_007, {
            ffi::sqlite3_result_text64(ctx, cstring, len as _, Some(ffi::drop_cstring), ffi::SQLITE_UTF8 as _);
        }, {
            ffi::sqlite3_result_text(ctx, cstring, len as _, None)
        });
    },
    /// Sets the context error to this error.
    match Error as (ctx, err) => {
        if let Error::Sqlite(code) = err {
            ffi::sqlite3_result_error_code(ctx, code);
        } else {
            let msg = format!("{}", err);
            let msg = msg.as_bytes();
            let len = msg.len();
            ffi::sqlite3_result_error(ctx, msg.as_ptr() as _, len as _);
        }
    }
}

/// Sets the context result to the contained value or NULL.
impl<T: ToContextResult> ToContextResult for Option<T> {
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context) {
        match self {
            Some(x) => x.assign_to(context),
            None => ().assign_to(context),
        }
    }
}

/// Sets either the context result or error, depending on the result.
impl<T: ToContextResult> ToContextResult for Result<T> {
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context) {
        match self {
            Ok(x) => x.assign_to(context),
            Err(x) => x.assign_to(context),
        }
    }
}

/// Sets a dynamically typed [Value] to the context result.
impl ToContextResult for Value {
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context) {
        match self {
            Value::Integer(x) => x.assign_to(context),
            Value::Float(x) => x.assign_to(context),
            Value::Text(x) => x.assign_to(context),
            Value::Blob(_) => todo!(),
            Value::Null => ().assign_to(context),
        }
    }
}