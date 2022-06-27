//! Create application-defined functions.
//!
//! The functionality in this module is primarily exposed through
//! [Connection::create_scalar_function] and [Connection::create_aggregate_function].
use super::{ffi, sqlite3_require_version, types::*, value::*, Connection, RiskLevel};
pub use context::*;
use std::{
    cmp::Ordering,
    ffi::{c_void, CStr, CString},
    mem::{drop, transmute},
    ptr::null_mut,
    slice,
    str::from_utf8_unchecked,
};

mod context;

type ScalarFunction<UserData, Return> = fn(&Context<UserData>, &[&ValueRef]) -> Return;
type CollationFunction<UserData> = fn(&UserData, &str, &str) -> Ordering;

/// Implement an application-defined aggregate function which cannot be used as a window
/// function.
///
/// In general, there is no reason to implement this trait instead of [AggregateFunction],
/// because the latter provides a blanket implementation of the former.
pub trait LegacyAggregateFunction: Default {
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
}

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

impl<T: AggregateFunction> LegacyAggregateFunction for T {
    type UserData = T::UserData;
    type Output = T::Output;

    fn default_value(context: &Context<Self::UserData>) -> Self::Output {
        <T as AggregateFunction>::default_value(context)
    }

    fn step(&mut self, context: &Context<Self::UserData>, args: &[&ValueRef]) -> Result<()> {
        <T as AggregateFunction>::step(self, context, args)
    }

    fn value(&self, context: &Context<Self::UserData>) -> Self::Output {
        <T as AggregateFunction>::value(self, context)
    }
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
    /// plus 8 bytes of memory. This is because these versions of SQLite did not provide
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

    /// Create a new aggregate function which cannot be used as a window function.
    ///
    /// In general, you should use
    /// [create_aggregate_function](Connection::create_aggregate_function) instead, which
    /// provides all of the same features as legacy aggregate functions but also support
    /// WINDOW.
    ///
    /// # Compatibility
    ///
    /// On versions of SQLite earlier than 3.7.3, this function will leak the user data
    /// plus 8 bytes of memory. This is because these versions of SQLite did not provide
    /// the ability to specify a destructor function.
    pub fn create_legacy_aggregate_function<F: LegacyAggregateFunction>(
        &self,
        name: &str,
        opts: &FunctionOptions,
        user_data: F::UserData,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(FnUserData::new_aggregate(user_data));
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
                        None,
                        Some(aggregate_step::<F>),
                        Some(aggregate_final::<F>),
                        Some(ffi::drop_boxed::<FnUserData<F::UserData>>),
                    ))
                },
                {
                    Error::from_sqlite(ffi::sqlite3_create_function(
                        self.as_ptr(),
                        name.as_ptr() as _,
                        opts.n_args,
                        opts.flags,
                        Box::into_raw(user_data) as _,
                        None,
                        Some(aggregate_step::<F>),
                        Some(aggregate_final::<F>),
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
    ///
    /// # Compatibility
    ///
    /// Window functions require SQLite 3.25.0. On earlier versions of SQLite, this
    /// function will automatically fall back to
    /// [create_legacy_aggregate_function](Connection::create_legacy_aggregate_function).
    pub fn create_aggregate_function<F: AggregateFunction>(
        &self,
        name: &str,
        opts: &FunctionOptions,
        user_data: F::UserData,
    ) -> Result<()> {
        sqlite3_require_version!(
            3_025_000,
            {
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
            },
            self.create_legacy_aggregate_function::<F>(name, opts, user_data)
        )
    }

    /// Remove an application-defined scalar or aggregate function. The name and n_args
    /// parameters must match the values used when the function was created.
    pub fn remove_function(&self, name: &str, n_args: i32) -> Result<()> {
        unsafe {
            Error::from_sqlite(ffi::sqlite3_create_function(
                self.as_ptr(),
                name.as_ptr() as _,
                n_args,
                0,
                null_mut(),
                None,
                None,
                None,
            ))
        }
    }

    /// Register a new collating sequence.
    pub fn create_collation<U>(
        &self,
        name: &str,
        func: CollationFunction<U>,
        user_data: U,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(FnUserData::new_collation(user_data, func));
        unsafe {
            let user_data = Box::into_raw(user_data);
            let rc = ffi::sqlite3_create_collation_v2(
                self.as_ptr(),
                name.as_ptr() as _,
                ffi::SQLITE_UTF8,
                user_data as _,
                Some(compare::<U>),
                Some(ffi::drop_boxed::<FnUserData<U>>),
            );
            if rc != ffi::SQLITE_OK {
                // The xDestroy callback is not called if the
                // sqlite3_create_collation_v2() function fails.
                drop(Box::from_raw(user_data));
            }
            Error::from_sqlite(rc)
        }
    }

    /// Register a callback for when SQLite needs a collation sequence. The function will
    /// be invoked when a collation sequence is needed, and
    /// [create_collation](Connection::create_collation) can be used to provide the needed
    /// sequence.
    ///
    /// Note: the provided function and any captured variables will be leaked. SQLite does
    /// not provide any facilities for cleaning up this data.
    pub fn set_collation_needed_func<F: Fn(&str)>(&self, func: F) -> Result<()> {
        let func = Box::new(func);
        unsafe {
            Error::from_sqlite(ffi::sqlite3_collation_needed(
                self.as_ptr(),
                Box::into_raw(func) as _,
                Some(collation_needed::<F>),
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

    fn new_collation(user_data: U, func: CollationFunction<U>) -> FnUserData<U> {
        FnUserData {
            user_data,
            func: Some(unsafe { transmute(func) }),
        }
    }

    unsafe fn scalar_func<R: ToContextResult>(&self) -> ScalarFunction<U, R> {
        transmute(self.func.unwrap())
    }

    unsafe fn comparison_func(&self) -> CollationFunction<U> {
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

unsafe extern "C" fn aggregate_step<F: LegacyAggregateFunction>(
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

unsafe extern "C" fn aggregate_final<F: LegacyAggregateFunction>(
    context: *mut ffi::sqlite3_context,
) {
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

unsafe extern "C" fn compare<U>(
    user_data: *mut c_void,
    len_a: i32,
    bytes_a: *const c_void,
    len_b: i32,
    bytes_b: *const c_void,
) -> i32 {
    let user_data = &*(user_data as *const FnUserData<U>);
    let func = user_data.comparison_func();
    let a = from_utf8_unchecked(slice::from_raw_parts(bytes_a as *const u8, len_a as _));
    let b = from_utf8_unchecked(slice::from_raw_parts(bytes_b as *const u8, len_b as _));
    match func(&user_data.user_data, a, b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

unsafe extern "C" fn collation_needed<F: Fn(&str)>(
    user_data: *mut c_void,
    _db: *mut ffi::sqlite3,
    _text_rep: i32,
    name: *const i8,
) {
    let func = &*(user_data as *const F);
    let name = match CStr::from_ptr(name).to_str() {
        Ok(x) => x,
        Err(_) => return,
    };
    func(name);
}
