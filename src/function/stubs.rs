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

pub unsafe extern "C" fn call_scalar<F>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) where
    F: FnMut(&Context, &mut [&mut ValueRef]) -> Result<()>,
{
    let ic = InternalContext::from_ptr(context);
    let func = ic.user_data::<F>();
    let ctx = Context::from_ptr(context);
    let args = slice::from_raw_parts_mut(argv as *mut &mut ValueRef, argc as _);
    if let Err(e) = func(ctx, args) {
        ctx.set_result(e).unwrap();
    }
}

pub unsafe extern "C" fn aggregate_step<U, F: LegacyAggregateFunction<U>>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ic.aggregate_context::<U, F>().unwrap();
    let args = slice::from_raw_parts_mut(argv as *mut &mut ValueRef, argc as _);
    if let Err(e) = agg.step(ctx, args) {
        ctx.set_result(e).unwrap();
    }
}

pub unsafe extern "C" fn aggregate_final<U, F: LegacyAggregateFunction<U>>(
    context: *mut ffi::sqlite3_context,
) {
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let ret = match ic.try_aggregate_context::<U, F>() {
        Some(agg) => agg.value(ctx),
        None => F::default_value(ic.user_data(), ctx),
    };
    if let Err(e) = ret {
        ctx.set_result(e).unwrap();
    }
}

#[cfg(modern_sqlite)]
pub unsafe extern "C" fn aggregate_value<U, F: AggregateFunction<U>>(
    context: *mut ffi::sqlite3_context,
) {
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ic.aggregate_context::<U, F>().unwrap();
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
    let ic = InternalContext::from_ptr(context);
    let ctx = Context::from_ptr(context);
    let agg = ic.aggregate_context::<U, F>().unwrap();
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
