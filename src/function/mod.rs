//! Create application-defined functions.
//!
//! The functionality in this module is primarily exposed through
//! [Connection::create_scalar_function] and [Connection::create_aggregate_function].
use super::{ffi, sqlite3_require_version, types::*, value::*, Connection, RiskLevel};
pub use context::*;
use std::{ffi::CString, mem::transmute, slice};

mod context;

type ScalarFunction<UserData, Return> = fn(&Context<UserData>, &[&ValueRef]) -> Return;

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
    fn default_value(context: &Context<Self::UserData>) -> Self::Output {
        Self::default().value(context)
    }

    /// Add a new row to the aggregate.
    fn step(&mut self, context: &Context<Self::UserData>, args: &[&ValueRef]) -> Result<()>;

    /// Return the current value of the aggregate function.
    fn value(&self, context: &Context<Self::UserData>) -> Self::Output;

    /// Remove the oldest presently aggregated row.
    ///
    /// The args are the same that were passed to [AggregateFunction::step] when this row
    /// was added.
    fn inverse(&mut self, context: &Context<Self::UserData>, args: &[&ValueRef]) -> Result<()>;
}

#[derive(Clone)]
pub struct FunctionOptions {
    n_args: i32,
    flags: i32,
}

impl Default for FunctionOptions {
    fn default() -> Self {
        FunctionOptions {
            n_args: -1,
            flags: 0,
        }
    }
}

impl FunctionOptions {
    /// Set the number of parameters accepted by this function. Multiple functions may be
    /// provided under the same name with different n_args values; the implementation will
    /// be chosen by SQLite based on the number of parameters at the call site. The value
    /// may also be -1, which means that the function accepts any number of parameters.
    /// Functions which take a specific number of parameters take precedence over functions
    /// which take any number.
    ///
    /// # Panics
    ///
    /// This function panics if n_args is outside the range -1..128. This limitation is
    /// imposed by SQLite.
    pub fn set_n_args(mut self, n_args: i32) -> Self {
        assert!((-1..128).contains(&n_args), "n_args invalid");
        self.n_args = n_args;
        self
    }

    /// Enable or disable the deterministic flag. This flag indicates that the function is
    /// pure. It must have no side effects and the value must be determined solely its the
    /// parameters.
    ///
    /// The SQLite query planner is able to perform additional optimizations on
    /// deterministic functions, so use of this flag is recommended where possible.
    pub fn set_deterministic(mut self, val: bool) -> Self {
        if val {
            self.flags |= ffi::SQLITE_DETERMINISTIC;
        } else {
            self.flags &= !ffi::SQLITE_DETERMINISTIC;
        }
        self
    }

    /// Set the level of risk for this function. See the [RiskLevel] enum for details about
    /// what the individual options mean.
    ///
    /// Requires SQLite 3.31.0. On earlier versions of SQLite, this function is a no-op.
    pub fn set_risk_level(mut self, level: RiskLevel) -> Self {
        sqlite3_require_version!(
            3_031_000,
            {
                self.flags |= match level {
                    RiskLevel::Innocuous => ffi::SQLITE_INNOCUOUS,
                    RiskLevel::DirectOnly => ffi::SQLITE_DIRECTONLY,
                };
                self.flags &= match level {
                    RiskLevel::Innocuous => !ffi::SQLITE_DIRECTONLY,
                    RiskLevel::DirectOnly => !ffi::SQLITE_INNOCUOUS,
                };
            },
            {
                let _ = level;
            }
        );
        self
    }
}

impl Connection {
    /// Create a new scalar function.
    ///
    /// The function will be available under the given name. The user_data parameter is
    /// used to associate an additional value with the function, which will be made
    /// available using [Context::user_data].
    ///
    /// # Compatibility
    ///
    /// On versions of SQLite earlier than 3.7.3, this function will leak the user data
    /// plus 16 bytes of memory. This is because these versions of SQLite did not provide
    /// the ability to specify a destructor function.
    pub fn create_scalar_function<U, R: ToContextResult>(
        &self,
        name: &str,
        opts: &FunctionOptions,
        func: ScalarFunction<U, R>,
        user_data: U,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(FnUserData::new_scalar(user_data, func));
        unsafe {
            sqlite3_require_version!(
                3_007_003,
                {
                    Error::from_sqlite(ffi::sqlite3_create_function_v2(
                        self.as_ptr(),
                        name.as_ptr() as _,
                        opts.n_args,
                        opts.flags,
                        Box::into_raw(user_data) as _,
                        Some(call_scalar::<U, R>),
                        None,
                        None,
                        Some(ffi::drop_boxed::<FnUserData<U>>),
                    ))
                },
                {
                    Error::from_sqlite(ffi::sqlite3_create_function(
                        self.as_ptr(),
                        name.as_ptr() as _,
                        opts.n_args,
                        opts.flags,
                        Box::into_raw(user_data) as _,
                        Some(call_scalar::<U, R>),
                        None,
                        None,
                    ))
                }
            )
        }
    }

    /// Create a new aggregate function.
    ///
    /// Aggregate functions are similar to scalar ones; see
    /// [create_scalar_function](Connection::create_scalar_function) for a discussion about
    /// the parameters.
    pub fn create_aggregate_function<F: AggregateFunction>(
        &self,
        name: &str,
        opts: &FunctionOptions,
        user_data: F::UserData,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(FnUserData::new_aggregate(user_data));
        unsafe {
            Error::from_sqlite(ffi::sqlite3_create_window_function(
                self.as_ptr(),
                name.as_ptr() as _,
                opts.n_args,
                opts.flags,
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
