use super::FromUserData;
use crate::{ffi, sqlite3_match_version, types::*, value::*, Connection};
use sealed::sealed;
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

#[repr(C)]
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

    /// # Safety
    ///
    /// The called must verify that Rust pointer aliasing rules are followed.
    pub unsafe fn user_data<U>(&self) -> &mut U {
        &mut *(ffi::sqlite3_user_data(self.as_ptr()) as *mut U)
    }

    /// Get the aggregate context, returning a mutable reference to it.
    pub unsafe fn aggregate_context<U, F: FromUserData<U>>(&mut self) -> Result<&mut F> {
        let ptr =
            ffi::sqlite3_aggregate_context(self.as_ptr(), size_of::<AggregateContext<F>>() as _)
                as *mut AggregateContext<F>;
        if ptr.is_null() {
            return Err(SQLITE_NOMEM);
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

    /// Assign the given value to the result of the function. This function always returns Ok.
    pub fn set_result(&self, val: impl ToContextResult) -> Result<()> {
        unsafe { val.assign_to(self.as_ptr()) };
        Ok(())
    }
}

/// A value that can be returned from an SQL function.
///
/// There are several useful implementations available:
///
/// - For nullable values, Option\<ToContextResult\> provides an implementation.
/// - For fallible functions, [Result]\<ToContextResult\> provides an implementation.
/// - For arbitrary Rust objects, [PassedRef] provides an implementation.
/// - For borrowed SQLite values, &[ValueRef] provides an implementation. Note that you have to
///   reborrow as immutable in most cases: `&*value_ref`.
/// - For owned types known only at run-time, [Value] provides an implementation.
#[sealed]
pub trait ToContextResult {
    #[doc(hidden)]
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context);
}

macro_rules! to_context_result {
    ($($(#[$attr:meta])* match $ty:ty as ($ctx:ident, $val:ident) => $impl:expr),*) => {
        $(
        $(#[$attr])*
        #[sealed]
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
    match bool as (ctx, val) => ffi::sqlite3_result_int(ctx, val as i32),
    match i32 as (ctx, val) => ffi::sqlite3_result_int(ctx, val),
    match i64 as (ctx, val) => ffi::sqlite3_result_int64(ctx, val),
    match f64 as (ctx, val) => ffi::sqlite3_result_double(ctx, val),
    /// Assign a static string to the context result.
    match &'static str as (ctx, val) => {
        let val = val.as_bytes();
        let len = val.len();
        sqlite3_match_version! {
            3_008_007 => ffi::sqlite3_result_text64(ctx, val.as_ptr() as _, len as _, None, ffi::SQLITE_UTF8 as _),
            _ => ffi::sqlite3_result_text(ctx, val.as_ptr() as _, len as _, None),
        }
    },
    /// Assign an owned string to the context result.
    match String as (ctx, val) => {
        let val = val.as_bytes();
        let len = val.len();
        let cstring = CString::new(val).unwrap().into_raw();
        sqlite3_match_version! {
            3_008_007 => ffi::sqlite3_result_text64(ctx, cstring, len as _, Some(ffi::drop_cstring), ffi::SQLITE_UTF8 as _),
            _ => ffi::sqlite3_result_text(ctx, cstring, len as _, Some(ffi::drop_cstring)),
        }
    },
    match Blob as (ctx, val) => {
        let len = val.len();
        sqlite3_match_version! {
            3_008_007 => ffi::sqlite3_result_blob64(ctx, val.into_raw(), len as _, Some(ffi::drop_blob),),
            _ => ffi::sqlite3_result_blob(ctx, val.into_raw(), len as _, Some(ffi::drop_blob)),
        }
    },
    /// Sets the context error to this error.
    match Error as (ctx, err) => {
        match err {
            Error::Sqlite(_, Some(desc)) => {
                let bytes = desc.as_bytes();
                ffi::sqlite3_result_error(ctx, bytes.as_ptr() as _, bytes.len() as _)
            },
            Error::Sqlite(code, None) => ffi::sqlite3_result_error_code(ctx, code),
            Error::NoChange => (),
            _ => {
                let msg = format!("{}", err);
                let msg = msg.as_bytes();
                let len = msg.len();
                ffi::sqlite3_result_error(ctx, msg.as_ptr() as _, len as _);
            }
        }
    }
}

/// Sets the context result to the contained value.
#[sealed]
impl<'a> ToContextResult for &'a ValueRef {
    unsafe fn assign_to(self, ctx: *mut ffi::sqlite3_context) {
        ffi::sqlite3_result_value(ctx, self.as_ptr())
    }
}

/// Sets the context result to the contained value.
#[sealed]
impl<'a> ToContextResult for &'a mut ValueRef {
    unsafe fn assign_to(self, ctx: *mut ffi::sqlite3_context) {
        ffi::sqlite3_result_value(ctx, self.as_ptr())
    }
}

/// Sets the context result to the given BLOB.
#[sealed]
impl<'a> ToContextResult for &'a [u8] {
    unsafe fn assign_to(self, ctx: *mut ffi::sqlite3_context) {
        let len = self.len();
        sqlite3_match_version! {
            3_008_007 => ffi::sqlite3_result_blob64(
                ctx,
                self.as_ptr() as _,
                len as _,
                ffi::sqlite_transient(),
            ),
            _ => ffi::sqlite3_result_blob(
                ctx,
                self.as_ptr() as _,
                len as _,
                ffi::sqlite_transient(),
            ),
        }
    }
}

/// Sets the context result to the given BLOB.
#[sealed]
impl<'a, const N: usize> ToContextResult for &'a [u8; N] {
    unsafe fn assign_to(self, ctx: *mut ffi::sqlite3_context) {
        self.as_slice().assign_to(ctx);
    }
}

/// Sets the context result to the contained value or NULL.
#[sealed]
impl<T: ToContextResult> ToContextResult for Option<T> {
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context) {
        match self {
            Some(x) => x.assign_to(context),
            None => ().assign_to(context),
        }
    }
}

/// Sets either the context result or error, depending on the result.
#[sealed]
impl<T: ToContextResult> ToContextResult for Result<T> {
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context) {
        match self {
            Ok(x) => x.assign_to(context),
            Err(x) => x.assign_to(context),
        }
    }
}

/// Sets a dynamically typed [Value] to the context result.
#[sealed]
impl ToContextResult for Value {
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context) {
        match self {
            Value::Integer(x) => x.assign_to(context),
            Value::Float(x) => x.assign_to(context),
            Value::Text(x) => x.assign_to(context),
            Value::Blob(x) => x.assign_to(context),
            Value::Null => ().assign_to(context),
        }
    }
}

/// Sets an arbitrary pointer to the context result.
#[sealed]
impl<T: 'static + ?Sized> ToContextResult for UnsafePtr<T> {
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context) {
        sqlite3_match_version! {
        3_009_000 => {
            let subtype = self.subtype;
            self.to_bytes().assign_to(context);
            ffi::sqlite3_result_subtype(context, subtype as _);
        },
        _ => self.to_bytes().assign_to(context),
        }
    }
}

/// Sets the context result to NULL with this value as an associated pointer.
#[sealed]
impl<T: 'static> ToContextResult for PassedRef<T> {
    unsafe fn assign_to(self, context: *mut ffi::sqlite3_context) {
        let _ = (POINTER_TAG, context);
        sqlite3_match_version! {
            3_020_000 => ffi::sqlite3_result_pointer(
                context,
                Box::into_raw(Box::new(self)) as _,
                POINTER_TAG,
                Some(ffi::drop_boxed::<PassedRef<T>>),
            ),
            _ => (),
        }
    }
}
