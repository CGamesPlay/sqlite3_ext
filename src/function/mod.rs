//! Create application-defined functions.
//!
//! The functionality in this module is primarily exposed through
//! [Connection::create_scalar_function] and [Connection::create_aggregate_function].
use super::{ffi, types::*, value::*, Connection};
use bitflags::bitflags;
pub use context::*;
use std::{ffi::CString, mem::transmute, slice};

mod context;

bitflags! {
    /// Flags used to indicate the behavior of application-defined functions.
    ///
    /// It is recommended that all functions at least set the
    /// [INNOCUOUS](FunctionFlag::INNOCUOUS) or [DIRECTONLY](FunctionFlag::DIRECTONLY)
    /// flag.
    ///
    /// For details about all flags, see [the SQLite documentation](https://www.sqlite.org/c3ref/c_deterministic.html).
    #[repr(transparent)]
    pub struct FunctionFlag: i32 {
        /// Indicates that the function is pure. It must have no side effects and the
        /// value must be determined solely its the parameters.
        ///
        /// The SQLite query planner is able to perform additional optimizations on
        /// deterministic functions, so use of this flag is recommended where possible.
        const DETERMINISTIC = ffi::SQLITE_DETERMINISTIC;
        /// Indicates that the function is potentially unsafe. See
        /// [vtab::RiskLevel](crate::vtab::RiskLevel) for a discussion about risk
        /// levels.
        const DIRECTONLY = ffi::SQLITE_DIRECTONLY;
        /// Indicates that the function is safe to use in untrusted contexts. See
        /// [vtab::RiskLevel](crate::vtab::RiskLevel) for a discussion about risk
        /// levels.
        const INNOCUOUS = ffi::SQLITE_INNOCUOUS;
        const SUBTYPE = ffi::SQLITE_SUBTYPE;
    }
}

type ScalarFunction<UserData, Return> = fn(&Context<UserData>, &[&ValueRef]) -> Result<Return>;

/// Implement an application-defined aggregate window function.
///
/// The function can be registered with a database connection using
/// [Connection::create_aggregate_function].
pub trait AggregateFunction: Default {
    /// The type of data that is provided to the function when it is created.
    type UserData;
    /// The output type of the function.
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
    /// Create a new scalar function.
    ///
    /// The function will be available under the given name. Multiple functions may be
    /// provided under the same name with different n_args values; the implementation will
    /// be chosen by SQLite based on the number of parameters at the call site. The n_args
    /// parameter may also be -1, which means that the function accepts any number of
    /// parameters. Functions which take a specific number of parameters take precedence
    /// over functions which take any number.
    ///
    /// It is recommended that flags includes one of [FunctionFlag::INNOCUOUS] or
    /// [FunctionFlag::DIRECTONLY].
    ///
    /// An additional value can be associated with the function, which will be made
    /// available using [Context::user_data].
    ///
    /// # Panics
    ///
    /// This function panics if n_args is outside the range -1..128. This limitation is
    /// imposed by SQLite.
    pub fn create_scalar_function<U, R: ToContextResult>(
        &self,
        name: &str,
        n_args: isize,
        flags: FunctionFlag,
        func: ScalarFunction<U, R>,
        user_data: U,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(FnUserData::new_scalar(user_data, func));
        assert!((-1..128).contains(&n_args), "n_args invalid");
        unsafe {
            Error::from_sqlite(ffi::sqlite3_create_function_v2(
                self.as_ptr(),
                name.as_ptr() as _,
                n_args as _,
                flags.bits,
                Box::into_raw(user_data) as _,
                Some(call_scalar::<U, R>),
                None,
                None,
                Some(ffi::drop_boxed::<FnUserData<U>>),
            ))
        }
    }

    /// Create a new aggregate function.
    ///
    /// Aggregate functions are similar to scalar ones; see
    /// [create_scalar_function](Connection::create_scalar_function) for a discussion about
    /// the flags and parameters.
    ///
    /// # Panics
    ///
    /// This function panics if n_args is outside the range -1..128. This limitation is
    /// imposed by SQLite.
    pub fn create_aggregate_function<F: AggregateFunction>(
        &self,
        name: &str,
        n_args: isize,
        flags: FunctionFlag,
        user_data: F::UserData,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(FnUserData::new_aggregate(user_data));
        assert!(n_args >= 0 && n_args <= 127, "n_args invalid");
        unsafe {
            Error::from_sqlite(ffi::sqlite3_create_window_function(
                self.as_ptr(),
                name.as_ptr() as _,
                n_args as _,
                flags.bits,
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
