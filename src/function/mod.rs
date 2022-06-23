use super::{ffi, types::*, value::*, Connection};
pub use context::*;
use std::{ffi::CString, ptr, slice};

mod context;

pub trait ScalarFunction: Fn(&mut Context, &[&Value]) -> Result<()> {}
impl<X: Fn(&mut Context, &[&Value]) -> Result<()>> ScalarFunction for X {}

pub trait AggregateFunction: Default {
    type Return: ToContextResult;

    const DEFAULT_VALUE: Self::Return;

    /// Add a new row to the aggregate.
    ///
    /// This function should return the current value of the aggregate after adding the
    /// row. Note that step is not allowed to fail, and so failures must be stored and
    /// returned by [value](AggregateFunction::value).
    fn step(&mut self, context: &mut Context, args: &[&Value]);

    /// Return the current value of the aggregate function.
    fn value(&self, context: &mut Context) -> Result<Self::Return>;

    /// Remove the oldest presently aggregated row.
    ///
    /// The args are the same that were passed to [AggregateFunction::step] when this row
    /// was added.
    fn inverse(&mut self, context: &mut Context, args: &[&Value]);
}

impl Connection {
    pub fn create_scalar_function<F: ScalarFunction>(
        &self,
        name: &str,
        n_args: isize,
        flags: usize,
        func: F,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let func = Box::new(func);
        unsafe {
            Error::from_sqlite(ffi::sqlite3_create_function_v2(
                self.as_ptr(),
                name.as_ptr() as _,
                n_args as _,
                flags as _,
                Box::into_raw(func) as _,
                Some(call_scalar::<F>),
                None,
                None,
                Some(ffi::drop_boxed::<F>),
            ))
        }
    }

    pub fn create_aggregate_function<F: AggregateFunction + 'static>(
        &self,
        name: &str,
        n_args: isize,
        flags: usize,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        unsafe {
            Error::from_sqlite(ffi::sqlite3_create_window_function(
                self.as_ptr(),
                name.as_ptr() as _,
                n_args as _,
                flags as _,
                ptr::null_mut(),
                Some(aggregate_step::<F>),
                Some(aggregate_final::<F>),
                Some(aggregate_value::<F>),
                Some(aggregate_inverse::<F>),
                None,
            ))
        }
    }
}

unsafe extern "C" fn call_scalar<F: ScalarFunction>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let func = &*(ffi::sqlite3_user_data(context) as *const F);
    let context = &mut *(context as *mut Context);
    let args = slice::from_raw_parts(argv as *mut &Value, argc as _);
    match func(context, args) {
        Err(err) => {
            let _ = context.set_result(err);
        }
        _ => (),
    }
}

unsafe extern "C" fn aggregate_step<F: AggregateFunction + 'static>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let context = &mut *(context as *mut Context);
    let double_borrow = &mut *(context as *mut Context);
    let agg = context.aggregate_context::<F>().unwrap();
    let args = slice::from_raw_parts(argv as *mut &Value, argc as _);
    agg.step(double_borrow, args);
}

unsafe extern "C" fn aggregate_final<F: AggregateFunction + 'static>(
    context: *mut ffi::sqlite3_context,
) {
    let context = &mut *(context as *mut Context);
    let double_borrow = &mut *(context as *mut Context);
    match context.try_aggregate_context::<F>() {
        Some(agg) => context.set_result(agg.value(double_borrow)),
        None => context.set_result(F::DEFAULT_VALUE),
    };
}

unsafe extern "C" fn aggregate_value<F: AggregateFunction + 'static>(
    context: *mut ffi::sqlite3_context,
) {
    let context = &mut *(context as *mut Context);
    let double_borrow = &mut *(context as *mut Context);
    let triple_borrow = &mut *(context as *mut Context);
    let agg = context.aggregate_context::<F>().unwrap();
    triple_borrow.set_result(agg.value(double_borrow));
}

unsafe extern "C" fn aggregate_inverse<F: AggregateFunction + 'static>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let context = &mut *(context as *mut Context);
    let double_borrow = &mut *(context as *mut Context);
    let agg = context.aggregate_context::<F>().unwrap();
    let args = slice::from_raw_parts(argv as *mut &Value, argc as _);
    agg.inverse(double_borrow, args);
}
