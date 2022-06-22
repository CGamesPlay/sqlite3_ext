use super::{ffi, types::*, value::*, Connection};
pub use context::*;
use std::{ffi::CString, slice};

mod context;

pub trait ScalarFunction: Fn(&mut Context, &[&Value]) -> Result<()> {}
impl<X: Fn(&mut Context, &[&Value]) -> Result<()>> ScalarFunction for X {}

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
}

pub trait AggregateFunction {
    type Return: ToContextResult;

    fn step(&mut self, context: &mut Context, args: &[&Value]) -> Result<()>;

    fn inverse(&mut self, context: &mut Context, args: &[&Value]) -> Result<()>;

    fn value(&self, context: &Context) -> Result<Self::Return>;
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
