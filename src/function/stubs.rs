use super::{
    super::{ffi, value::*},
    *,
};
use std::{
    cmp::Ordering,
    ffi::{c_void, CStr},
    slice,
    str::from_utf8_unchecked,
};

pub unsafe extern "C" fn call_scalar<'a, F>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) where
    F: ScalarFunction<'a>,
{
    let func = &mut *(ffi::sqlite3_user_data(context) as *mut F);
    let ctx = Context::from_ptr(context);
    let args = slice::from_raw_parts_mut(argv as *mut &mut ValueRef, argc as _);
    if let Err(e) = func.call(ctx, args) {
        ctx.set_result(e).unwrap();
    }
}

pub unsafe extern "C" fn aggregate_step<U, F: LegacyAggregateFunction<U>>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let ac = AggregateContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ac.get_or_insert_with(F::from_user_data).unwrap();
    let args = slice::from_raw_parts_mut(argv as *mut &mut ValueRef, argc as _);
    if let Err(e) = agg.step(ctx, args) {
        ctx.set_result(e).unwrap();
    }
}

pub unsafe extern "C" fn aggregate_final<U, F: LegacyAggregateFunction<U>>(
    context: *mut ffi::sqlite3_context,
) {
    let ac = AggregateContext::<U, F>::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let ret = match ac.take() {
        Some(agg) => agg.value(ctx),
        None => F::default_value(ac.user_data(), ctx),
    };
    if let Err(e) = ret {
        ctx.set_result(e).unwrap();
    }
}

#[cfg(modern_sqlite)]
pub unsafe extern "C" fn aggregate_value<U, F: AggregateFunction<U>>(
    context: *mut ffi::sqlite3_context,
) {
    let ac = AggregateContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ac.get_or_insert_with(F::from_user_data).unwrap();
    if let Err(e) = agg.value(ctx) {
        ctx.set_result(e).unwrap();
    }
}

#[cfg(modern_sqlite)]
pub unsafe extern "C" fn aggregate_inverse<U, F: AggregateFunction<U>>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let ac = AggregateContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ac.get_or_insert_with(F::from_user_data).unwrap();
    let args = slice::from_raw_parts_mut(argv as *mut &mut ValueRef, argc as _);
    if let Err(e) = agg.inverse(ctx, args) {
        ctx.set_result(e).unwrap();
    }
}

pub unsafe extern "C" fn compare<F: Fn(&str, &str) -> Ordering>(
    func: *mut c_void,
    len_a: i32,
    bytes_a: *const c_void,
    len_b: i32,
    bytes_b: *const c_void,
) -> i32 {
    let func = &*(func as *const F);
    let a = from_utf8_unchecked(slice::from_raw_parts(bytes_a as *const u8, len_a as _));
    let b = from_utf8_unchecked(slice::from_raw_parts(bytes_b as *const u8, len_b as _));
    match func(a, b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

pub unsafe extern "C" fn collation_needed<F: Fn(&str)>(
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
