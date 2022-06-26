use super::{ffi, types::*, value::*, Connection};
pub use context::*;
use std::{ffi::CString, mem::transmute, slice};

mod context;

type ScalarFunction<UserData, Return> = fn(&Context<UserData>, &[&ValueRef]) -> Result<Return>;

pub trait AggregateFunction: Default {
    type UserData;
    type Output: ToContextResult;

    /// Return the default value of the aggregate function.
    ///
    /// This method is called when the aggregate function is invoked over an empty set of
    /// rows. The default implementation is equivalent to `Self::default().value(context)`.
    fn default_value(context: &Context<Self::UserData>) -> Result<Self::Output> {
        Self::default().value(context)
    }

    /// Add a new row to the aggregate.
    fn step(&mut self, context: &Context<Self::UserData>, args: &[&ValueRef]) -> Result<()>;

    /// Return the current value of the aggregate function.
    fn value(&self, context: &Context<Self::UserData>) -> Result<Self::Output>;

    /// Remove the oldest presently aggregated row.
    ///
    /// The args are the same that were passed to [AggregateFunction::step] when this row
    /// was added.
    fn inverse(&mut self, context: &Context<Self::UserData>, args: &[&ValueRef]) -> Result<()>;
}

impl Connection {
    pub fn create_scalar_function<U, R: ToContextResult>(
        &self,
        name: &str,
        n_args: isize,
        flags: usize,
        func: ScalarFunction<U, R>,
        user_data: U,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(FnUserData::new_scalar(user_data, func));
        unsafe {
            Error::from_sqlite(ffi::sqlite3_create_function_v2(
                self.as_ptr(),
                name.as_ptr() as _,
                n_args as _,
                flags as _,
                Box::into_raw(user_data) as _,
                Some(call_scalar::<U, R>),
                None,
                None,
                Some(ffi::drop_boxed::<FnUserData<U>>),
            ))
        }
    }

    pub fn create_aggregate_function<F: AggregateFunction>(
        &self,
        name: &str,
        n_args: isize,
        flags: usize,
        user_data: F::UserData,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(FnUserData::new_aggregate(user_data));
        unsafe {
            Error::from_sqlite(ffi::sqlite3_create_window_function(
                self.as_ptr(),
                name.as_ptr() as _,
                n_args as _,
                flags as _,
                Box::into_raw(user_data) as _,
                Some(aggregate_step::<F>),
                Some(aggregate_final::<F>),
                Some(aggregate_value::<F>),
                Some(aggregate_inverse::<F>),
                Some(ffi::drop_boxed::<FnUserData<F::UserData>>),
            ))
        }
    }
}

struct FnUserData<U> {
    user_data: U,
    func: Option<fn()>,
}

impl<U> FnUserData<U> {
    fn new_scalar<R: ToContextResult>(user_data: U, func: ScalarFunction<U, R>) -> FnUserData<U> {
        FnUserData {
            user_data,
            func: Some(unsafe { transmute(func) }),
        }
    }

    fn new_aggregate(user_data: U) -> FnUserData<U> {
        FnUserData {
            user_data,
            func: None,
        }
    }

    unsafe fn scalar_func<R: ToContextResult>(&self) -> ScalarFunction<U, R> {
        transmute(self.func.unwrap())
    }
}

unsafe extern "C" fn call_scalar<U, R: ToContextResult>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let user_data = &*(ffi::sqlite3_user_data(context) as *const FnUserData<U>);
    let func = user_data.scalar_func::<R>();
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let args = slice::from_raw_parts(argv as *mut &ValueRef, argc as _);
    let ret = func(ctx, args);
    ic.set_result(ret);
}

unsafe extern "C" fn aggregate_step<F: AggregateFunction>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ic.aggregate_context::<F>().unwrap();
    let args = slice::from_raw_parts(argv as *mut &ValueRef, argc as _);
    if let Err(e) = agg.step(ctx, args) {
        ic.set_result(e);
    }
}

unsafe extern "C" fn aggregate_final<F: AggregateFunction>(context: *mut ffi::sqlite3_context) {
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    match ic.try_aggregate_context::<F>() {
        Some(agg) => ic.set_result(agg.value(ctx)),
        None => ic.set_result(F::default_value(ctx)),
    };
}

unsafe extern "C" fn aggregate_value<F: AggregateFunction>(context: *mut ffi::sqlite3_context) {
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ic.aggregate_context::<F>().unwrap();
    let ret = agg.value(ctx);
    ic.set_result(ret);
}

unsafe extern "C" fn aggregate_inverse<F: AggregateFunction>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ic.aggregate_context::<F>().unwrap();
    let args = slice::from_raw_parts(argv as *mut &ValueRef, argc as _);
    if let Err(e) = agg.inverse(ctx, args) {
        ic.set_result(e);
    }
}
