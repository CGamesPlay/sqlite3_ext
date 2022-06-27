use super::{
    super::{ffi, sqlite3_require_version, types::*, value::*, Connection},
    FromUserData,
};
use std::{
    any::TypeId,
    ffi::CString,
    mem::{size_of, MaybeUninit},
};

#[repr(transparent)]
pub(crate) struct InternalContext {
    base: ffi::sqlite3_context,
}

/// Describes the run-time environment of an application-defined function.
#[repr(transparent)]
pub struct Context {
    base: ffi::sqlite3_context,
}

struct AggregateContext<T> {
    init: bool,
    val: MaybeUninit<T>,
}

struct AuxData<T> {
    type_id: TypeId,
    val: T,
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

    pub unsafe fn user_data<U>(&self) -> &U {
        &*(ffi::sqlite3_user_data(self.as_ptr()) as *const U)
    }

    /// Get the aggregate context, returning a mutable reference to it.
    pub unsafe fn aggregate_context<U, F: FromUserData<U>>(&mut self) -> Result<&mut F> {
        let ptr =
            ffi::sqlite3_aggregate_context(self.as_ptr(), size_of::<AggregateContext<F>>() as _)
                as *mut AggregateContext<F>;
        if ptr.is_null() {
            return Err(Error::no_memory());
        }
        let context = &mut *ptr;
        if !context.init {
            context.val = MaybeUninit::new(F::from_user_data(self.user_data()));
            context.init = true;
        }
        Ok(context.val.assume_init_mut())
    }

    /// Try to get the aggregate context, consuming it if it is found.
    pub unsafe fn try_aggregate_context<U, F: FromUserData<U>>(&mut self) -> Option<F> {
        let ptr = ffi::sqlite3_aggregate_context(self.as_ptr(), 0 as _) as *mut AggregateContext<F>;
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

impl Context {
    pub(crate) fn as_ptr<'a>(&self) -> *mut ffi::sqlite3_context {
        &self.base as *const ffi::sqlite3_context as _
    }

    pub(crate) unsafe fn from_ptr<'a>(base: *mut ffi::sqlite3_context) -> &'a mut Self {
        &mut *(base as *mut Self)
    }

    /// Return a handle to the current database.
    pub fn db(&self) -> &Connection {
        unsafe { Connection::from_ptr(ffi::sqlite3_context_db_handle(self.as_ptr())) }
    }

    /// Retrieve data about a function parameter that was previously set with
    /// [set_aux_data](Context::set_aux_data).
    ///
    /// This method returns None if T is different from the data type that was stored
    /// previously.
    pub fn aux_data<T: 'static>(&self, idx: usize) -> Option<&mut T> {
        unsafe {
            let data = ffi::sqlite3_get_auxdata(self.as_ptr(), idx as _) as *mut AuxData<T>;
            if data.is_null() {
                None
            } else {
                let data = &mut *data;
                if data.type_id == TypeId::of::<T>() {
                    Some(&mut data.val)
                } else {
                    None
                }
            }
        }
    }

    /// Set the auxiliary data associated with the corresponding function parameter.
    ///
    /// If some processing is necessary in order for a function parameter to be useful (for
    /// example, compiling a regular expression), this method can be used to cache the
    /// processed value in case it is later reused in the same query. The cached value can
    /// be retrieved with the [aux_data](Context::aux_data) method.
    pub fn set_aux_data<T: 'static>(&self, idx: usize, val: T) {
        let data = Box::new(AuxData {
            type_id: TypeId::of::<T>(),
            val,
        });
        unsafe {
            ffi::sqlite3_set_auxdata(
                self.as_ptr(),
                idx as _,
                Box::into_raw(data) as _,
                Some(ffi::drop_boxed::<AuxData<T>>),
            )
        };
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
