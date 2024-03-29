//! Create application-defined functions.
//!
//! The functionality in this module is primarily exposed through
//! [Connection::create_scalar_function] and [Connection::create_aggregate_function].
use super::{ffi, sqlite3_match_version, types::*, value::*, Connection, RiskLevel};
pub use context::*;
use std::{cmp::Ordering, ffi::CString, ptr::null_mut};

mod context;
mod stubs;
mod test;

/// Constructor for aggregate functions.
///
/// Aggregate functions are instantiated using user data provided when the function is
/// registered. There is a blanket implementation for types implementing [Default] for cases
/// where user data is not required.
pub trait FromUserData<T> {
    /// Construct a new instance based on the provided user data.
    fn from_user_data(data: &T) -> Self;
}

/// Trait for scalar functions. This trait is used with
/// [Connection::create_scalar_function_object] to implement scalar functions that have a
/// lifetime smaller than `'static`. It is also possible to use closures and avoid implementing
/// this trait, see [Connection::create_scalar_function] for details.
pub trait ScalarFunction<'db> {
    /// Perform a single invocation. The function will be invoked with a [Context] and an
    /// array of [ValueRef] objects. The function is required to set its output using
    /// [Context::set_result]. If no result is set, SQL NULL is returned. If the function
    /// returns an Err value, the SQL statement will fail, even if a result had been set.
    fn call(&self, context: &Context, args: &mut [&mut ValueRef]) -> Result<()>;
}

struct ScalarClosure<F>(F)
where
    F: Fn(&Context, &mut [&mut ValueRef]) -> Result<()> + 'static;

impl<F> ScalarFunction<'_> for ScalarClosure<F>
where
    F: Fn(&Context, &mut [&mut ValueRef]) -> Result<()> + 'static,
{
    fn call(&self, ctx: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
        self.0(ctx, args)
    }
}

/// Implement an application-defined aggregate function which cannot be used as a window
/// function.
///
/// In general, there is no reason to implement this trait instead of [AggregateFunction],
/// because the latter provides a blanket implementation of the former.
pub trait LegacyAggregateFunction<UserData>: FromUserData<UserData> {
    /// Assign the default value of the aggregate function to the context using
    /// [Context::set_result].
    ///
    /// This method is called when the aggregate function is invoked over an empty set of
    /// rows. The default implementation is equivalent to
    /// `Self::from_user_data(user_data).value(context)`.
    fn default_value(user_data: &UserData, context: &Context) -> Result<()>
    where
        Self: Sized,
    {
        Self::from_user_data(user_data).value(context)
    }

    /// Add a new row to the aggregate.
    fn step(&mut self, context: &Context, args: &mut [&mut ValueRef]) -> Result<()>;

    /// Assign the current value of the aggregate function to the context using
    /// [Context::set_result]. If no result is set, SQL NULL is returned. If the function returns
    /// an Err value, the SQL statement will fail, even if a result had been set before the
    /// failure.
    fn value(&self, context: &Context) -> Result<()>;
}

/// Implement an application-defined aggregate window function.
///
/// The function can be registered with a database connection using
/// [Connection::create_aggregate_function].
pub trait AggregateFunction<UserData>: FromUserData<UserData> {
    /// Assign the default value of the aggregate function to the context using
    /// [Context::set_result].
    ///
    /// This method is called when the aggregate function is invoked over an empty set of
    /// rows. The default implementation is equivalent to
    /// `Self::from_user_data(user_data).value(context)`.
    fn default_value(user_data: &UserData, context: &Context) -> Result<()>
    where
        Self: Sized,
    {
        Self::from_user_data(user_data).value(context)
    }

    /// Add a new row to the aggregate.
    fn step(&mut self, context: &Context, args: &mut [&mut ValueRef]) -> Result<()>;

    /// Assign the current value of the aggregate function to the context using
    /// [Context::set_result]. If no result is set, SQL NULL is returned. If the function returns
    /// an Err value, the SQL statement will fail, even if a result had been set before the
    /// failure.
    fn value(&self, context: &Context) -> Result<()>;

    /// Remove the oldest presently aggregated row.
    ///
    /// The args are the same that were passed to [AggregateFunction::step] when this row
    /// was added.
    fn inverse(&mut self, context: &Context, args: &mut [&mut ValueRef]) -> Result<()>;
}

impl<U, F: Default> FromUserData<U> for F {
    fn from_user_data(_: &U) -> F {
        F::default()
    }
}

impl<U, T: AggregateFunction<U>> LegacyAggregateFunction<U> for T {
    fn default_value(user_data: &U, context: &Context) -> Result<()> {
        <T as AggregateFunction<U>>::default_value(user_data, context)
    }

    fn step(&mut self, context: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
        <T as AggregateFunction<U>>::step(self, context, args)
    }

    fn value(&self, context: &Context) -> Result<()> {
        <T as AggregateFunction<U>>::value(self, context)
    }
}

#[derive(Debug, Clone)]
pub struct FunctionOptions {
    n_args: i32,
    flags: i32,
}

impl Default for FunctionOptions {
    fn default() -> Self {
        FunctionOptions::default()
    }
}

impl FunctionOptions {
    pub const fn default() -> Self {
        FunctionOptions {
            n_args: -1,
            flags: 0,
        }
    }

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
    pub const fn set_n_args(mut self, n_args: i32) -> Self {
        assert!(n_args >= -1 && n_args < 128, "n_args invalid");
        self.n_args = n_args;
        self
    }

    /// Enable or disable the deterministic flag. This flag indicates that the function is
    /// pure. It must have no side effects and the value must be determined solely its the
    /// parameters.
    ///
    /// The SQLite query planner is able to perform additional optimizations on
    /// deterministic functions, so use of this flag is recommended where possible.
    pub const fn set_deterministic(mut self, val: bool) -> Self {
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
    /// Requires SQLite 3.31.0. On earlier versions of SQLite, this function is a harmless no-op.
    pub const fn set_risk_level(
        #[cfg_attr(not(modern_sqlite), allow(unused_mut))] mut self,
        level: RiskLevel,
    ) -> Self {
        let _ = level;
        #[cfg(modern_sqlite)]
        {
            self.flags |= match level {
                RiskLevel::Innocuous => ffi::SQLITE_INNOCUOUS,
                RiskLevel::DirectOnly => ffi::SQLITE_DIRECTONLY,
            };
            self.flags &= match level {
                RiskLevel::Innocuous => !ffi::SQLITE_DIRECTONLY,
                RiskLevel::DirectOnly => !ffi::SQLITE_INNOCUOUS,
            };
        }
        self
    }
}

impl Connection {
    /// Create a stub function that always fails.
    ///
    /// This API makes sure a global version of a function with a particular name and
    /// number of parameters exists. If no such function exists before this API is called,
    /// a new function is created. The implementation of the new function always causes an
    /// exception to be thrown. So the new function is not good for anything by itself. Its
    /// only purpose is to be a placeholder function that can be overloaded by a virtual
    /// table.
    ///
    /// For more information, see [vtab::FindFunctionVTab](super::vtab::FindFunctionVTab).
    pub fn create_overloaded_function(&self, name: &str, opts: &FunctionOptions) -> Result<()> {
        let guard = self.lock();
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        unsafe {
            Error::from_sqlite_desc(
                ffi::sqlite3_overload_function(self.as_mut_ptr(), name.as_ptr() as _, opts.n_args),
                guard,
            )
        }
    }

    /// Create a new scalar function. The function will be invoked with a [Context] and an array of
    /// [ValueRef] objects. The function is required to set its output using [Context::set_result].
    /// If no result is set, SQL NULL is returned. If the function returns an Err value, the SQL
    /// statement will fail, even if a result had been set.
    ///
    /// The passed function can be a closure, however the lifetime of the closure must be
    /// `'static` due to limitations in the Rust borrow checker. The
    /// [Self::create_scalar_function_object] function is an alternative that allows using an
    /// alternative lifetime.
    ///
    /// # Compatibility
    ///
    /// On versions of SQLite earlier than 3.7.3, this function will leak the function and
    /// all bound variables. This is because these versions of SQLite did not provide the
    /// ability to specify a destructor function.
    pub fn create_scalar_function<F>(
        &self,
        name: &str,
        opts: &FunctionOptions,
        func: F,
    ) -> Result<()>
    where
        F: Fn(&Context, &mut [&mut ValueRef]) -> Result<()> + 'static,
    {
        self.create_scalar_function_object(name, &opts, ScalarClosure(func))
    }

    /// Create a new scalar function using a struct. This function is identical to
    /// [Self::create_scalar_function], but uses a trait object instead of a closure. This enables
    /// creating scalar functions that maintain references with a lifetime smaller than `'static`.
    pub fn create_scalar_function_object<'db, F>(
        &'db self,
        name: &str,
        opts: &FunctionOptions,
        func: F,
    ) -> Result<()>
    where
        F: ScalarFunction<'db>,
    {
        let guard = self.lock();
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let func = Box::new(func);
        unsafe {
            Error::from_sqlite_desc(
                sqlite3_match_version! {
                    3_007_003 => ffi::sqlite3_create_function_v2(
                        self.as_mut_ptr(),
                        name.as_ptr() as _,
                        opts.n_args,
                        opts.flags,
                        Box::into_raw(func) as _,
                        Some(stubs::call_scalar::<F>),
                        None,
                        None,
                        Some(ffi::drop_boxed::<F>),
                    ),
                    _ => ffi::sqlite3_create_function(
                        self.as_mut_ptr(),
                        name.as_ptr() as _,
                        opts.n_args,
                        opts.flags,
                        Box::into_raw(func) as _,
                        Some(stubs::call_scalar::<F>),
                        None,
                        None,
                    ),
                },
                guard,
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
    /// On versions of SQLite earlier than 3.7.3, this function will leak the user data.
    /// This is because these versions of SQLite did not provide the ability to specify a
    /// destructor function.
    pub fn create_legacy_aggregate_function<U, F: LegacyAggregateFunction<U>>(
        &self,
        name: &str,
        opts: &FunctionOptions,
        user_data: U,
    ) -> Result<()> {
        let guard = self.lock();
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let user_data = Box::new(user_data);
        unsafe {
            Error::from_sqlite_desc(
                sqlite3_match_version! {
                    3_007_003 => ffi::sqlite3_create_function_v2(
                        self.as_mut_ptr(),
                        name.as_ptr() as _,
                        opts.n_args,
                        opts.flags,
                        Box::into_raw(user_data) as _,
                        None,
                        Some(stubs::aggregate_step::<U, F>),
                        Some(stubs::aggregate_final::<U, F>),
                        Some(ffi::drop_boxed::<U>),
                    ),
                    _ => ffi::sqlite3_create_function(
                        self.as_mut_ptr(),
                        name.as_ptr() as _,
                        opts.n_args,
                        opts.flags,
                        Box::into_raw(user_data) as _,
                        None,
                        Some(stubs::aggregate_step::<U, F>),
                        Some(stubs::aggregate_final::<U, F>),
                    ),
                },
                guard,
            )
        }
    }

    /// Create a new aggregate function.
    ///
    /// # Compatibility
    ///
    /// Window functions require SQLite 3.25.0. On earlier versions of SQLite, this
    /// function will automatically fall back to
    /// [create_legacy_aggregate_function](Connection::create_legacy_aggregate_function).
    pub fn create_aggregate_function<U, F: AggregateFunction<U>>(
        &self,
        name: &str,
        opts: &FunctionOptions,
        user_data: U,
    ) -> Result<()> {
        sqlite3_match_version! {
            3_025_000 => {
                let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
                let user_data = Box::new(user_data);
                let guard = self.lock();
                unsafe {
                    Error::from_sqlite_desc(ffi::sqlite3_create_window_function(
                        self.as_mut_ptr(),
                        name.as_ptr() as _,
                        opts.n_args,
                        opts.flags,
                        Box::into_raw(user_data) as _,
                        Some(stubs::aggregate_step::<U, F>),
                        Some(stubs::aggregate_final::<U, F>),
                        Some(stubs::aggregate_value::<U, F>),
                        Some(stubs::aggregate_inverse::<U, F>),
                        Some(ffi::drop_boxed::<U>),
                    ), guard)
                }
            },
            _ => self.create_legacy_aggregate_function::<U, F>(name, opts, user_data),
        }
    }

    /// Remove an application-defined scalar or aggregate function. The name and n_args
    /// parameters must match the values used when the function was created.
    pub fn remove_function(&self, name: &str, n_args: i32) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let guard = self.lock();
        unsafe {
            Error::from_sqlite_desc(
                ffi::sqlite3_create_function(
                    self.as_mut_ptr(),
                    name.as_ptr() as _,
                    n_args,
                    0,
                    null_mut(),
                    None,
                    None,
                    None,
                ),
                guard,
            )
        }
    }

    /// Register a new collating sequence.
    pub fn create_collation<F: Fn(&str, &str) -> Ordering>(
        &self,
        name: &str,
        func: F,
    ) -> Result<()> {
        let name = unsafe { CString::from_vec_unchecked(name.as_bytes().into()) };
        let func = Box::into_raw(Box::new(func));
        let guard = self.lock();
        unsafe {
            let rc = ffi::sqlite3_create_collation_v2(
                self.as_mut_ptr(),
                name.as_ptr() as _,
                ffi::SQLITE_UTF8,
                func as _,
                Some(stubs::compare::<F>),
                Some(ffi::drop_boxed::<F>),
            );
            if rc != ffi::SQLITE_OK {
                // The xDestroy callback is not called if the
                // sqlite3_create_collation_v2() function fails.
                drop(Box::from_raw(func));
            }
            Error::from_sqlite_desc(rc, guard)
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
        let guard = self.lock();
        unsafe {
            Error::from_sqlite_desc(
                ffi::sqlite3_collation_needed(
                    self.as_mut_ptr(),
                    Box::into_raw(func) as _,
                    Some(stubs::collation_needed::<F>),
                ),
                guard,
            )
        }
    }
}
