use super::{
    super::{
        ffi,
        function::{Context, InternalContext, ToContextResult},
        value::*,
    },
    VTab,
};
use std::{borrow::Cow, cell::Cell, mem::transmute, slice};

pub struct VTabFunctionList<'vtab, T: VTab<'vtab> + ?Sized> {
    list: Vec<VTabFunction<'vtab, T>>,
}

// For each overloaded function, the function itself is the call_vtab_function of the
// appropriate type, and the user data is a pointer to this struct.
pub(crate) struct VTabFunction<'vtab, T: VTab<'vtab> + ?Sized> {
    pub n_args: i32,
    pub name: Cow<'vtab, str>,
    pub c_func: unsafe extern "C" fn(*mut ffi::sqlite3_context, i32, *mut *mut ffi::sqlite3_value),
    pub vtab: Cell<Option<&'vtab T>>,
    pub func: fn(),
}

impl<'vtab, T: VTab<'vtab> + ?Sized> Default for VTabFunctionList<'vtab, T> {
    fn default() -> Self {
        Self { list: Vec::new() }
    }
}

impl<'vtab, T: VTab<'vtab> + ?Sized> VTabFunctionList<'vtab, T> {
    pub fn add<R: ToContextResult>(
        &mut self,
        n_args: i32,
        name: impl Into<Cow<'vtab, str>>,
        func: fn(&T, &Context, &[&ValueRef]) -> R,
    ) {
        self.list.push(VTabFunction {
            n_args,
            name: name.into(),
            c_func: call_vtab_function::<T, R>,
            vtab: Cell::new(None),
            func: unsafe { transmute(func) },
        })
    }

    pub(crate) fn find(
        &'vtab self,
        n_args: i32,
        name: &str,
    ) -> Option<&'vtab VTabFunction<'vtab, T>> {
        self.list
            .iter()
            .find(|f| f.n_args == n_args && f.name == name)
    }
}

unsafe extern "C" fn call_vtab_function<
    'vtab,
    T: VTab<'vtab> + 'vtab + ?Sized,
    R: ToContextResult,
>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let ic = InternalContext::from_ptr(context);
    let vtab_function = ic.user_data::<VTabFunction<'vtab, T>>();
    let func = transmute::<_, fn(&T, &Context, &[&ValueRef]) -> R>(vtab_function.func);
    let ctx = Context::from_ptr(context);
    let args = slice::from_raw_parts(argv as *mut &ValueRef, argc as _);
    let vtab = vtab_function.vtab.get().unwrap();
    let ret = func(vtab, ctx, args);
    ic.set_result(ret);
}
