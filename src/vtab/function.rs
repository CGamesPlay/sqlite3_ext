use super::{
    super::{
        ffi,
        function::{Context, InternalContext},
        types::*,
        value::*,
    },
    ConstraintOp, VTab,
};
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    os::raw::c_int,
    pin::Pin,
    slice,
};

type CFunc = unsafe extern "C" fn(*mut ffi::sqlite3_context, c_int, *mut *mut ffi::sqlite3_value);

/// A collection of methods overloaded by a virtual table.
///
/// This object is responsible for storing the data associated with overloaded functions. All
/// functions stored in the list must last for the entire lifetime of the virtual table.
pub struct VTabFunctionList<'vtab, T: VTab<'vtab> + ?Sized> {
    list: RefCell<Vec<Pin<Box<VTabFunction<'vtab, T>>>>>,
}

impl<'vtab, T: VTab<'vtab> + ?Sized> Default for VTabFunctionList<'vtab, T> {
    fn default() -> Self {
        Self {
            list: RefCell::new(Vec::new()),
        }
    }
}

impl<'vtab, T: VTab<'vtab> + 'vtab> VTabFunctionList<'vtab, T> {
    fn _add(
        &self,
        n_args: i32,
        name: impl Into<Cow<'vtab, str>>,
        constraint: Option<ConstraintOp>,
        func: Box<dyn Fn(&'vtab T, &InternalContext, &mut [&mut ValueRef]) + 'vtab>,
    ) {
        assert!((-1..128).contains(&n_args), "n_args invalid");
        if let Some(c) = &constraint {
            c.assert_valid_function_constraint();
        }
        self.list
            .borrow_mut()
            .push(VTabFunction::new(n_args, name, constraint, func));
    }

    /// Add a scalar function to the list.
    ///
    /// This method adds a function with the given name and n_args to the list of
    /// overloaded functions. Note that when looking for applicable overloads, a function
    /// with the correct n_args value will be selected before a function with n_args of -1.
    ///
    /// A constraint may be provided. If it is, then the constraint will be provided as an
    /// [IndexInfoConstraint](super::index_info::IndexInfoConstraint) to [VTab::best_index].
    ///
    /// The function and all closed variables must live for the duration of the virtual
    /// table.
    pub fn add<F>(
        &self,
        n_args: i32,
        name: impl Into<Cow<'vtab, str>>,
        constraint: Option<ConstraintOp>,
        func: F,
    ) where
        F: Fn(&Context, &mut [&mut ValueRef]) -> Result<()> + 'vtab,
    {
        let func = wrap_fn(func);
        self._add(n_args, name, constraint, func);
    }

    /// Add a method to the list.
    ///
    /// This function works similarly to [add](VTabFunctionList::add), except the function
    /// will receive the virtual table as the first parameter.
    pub fn add_method<F>(
        &self,
        n_args: i32,
        name: impl Into<Cow<'vtab, str>>,
        constraint: Option<ConstraintOp>,
        func: F,
    ) where
        F: Fn(&'vtab T, &Context, &mut [&mut ValueRef]) -> Result<()> + 'vtab,
    {
        let func = wrap_method(func);
        self._add(n_args, name, constraint, func);
    }

    /// Find the best overridden implementation of a function in this list. Prefer a
    /// precise number of arguments, but fall back to overloads which accept any number of
    /// arguments.
    ///
    /// This method is coupled with bind until `feature(cell_filter_map)` is ready.
    pub(crate) fn find(
        &self,
        vtab: &'vtab T,
        n_args: i32,
        name: &str,
    ) -> Option<((CFunc, *mut ::std::os::raw::c_void), Option<ConstraintOp>)> {
        let list = self.list.borrow();
        let found = [n_args, -1]
            .into_iter()
            .find_map(|n_args| list.iter().find(|f| f.n_args == n_args && f.name == name));
        found.map(|r| (r.bind(vtab), r.constraint))
    }
}

fn wrap_fn<'vtab, T, F>(
    func: F,
) -> Box<dyn Fn(&'vtab T, &InternalContext, &mut [&mut ValueRef]) + 'vtab>
where
    T: VTab<'vtab>,
    F: Fn(&Context, &mut [&mut ValueRef]) -> Result<()> + 'vtab,
{
    Box::new(
        move |_: &T, ic: &InternalContext, a: &mut [&mut ValueRef]| {
            let ctx = unsafe { Context::from_ptr(ic.as_ptr()) };
            if let Err(e) = func(ctx, a) {
                ctx.set_result(e).unwrap();
            }
        },
    )
}

fn wrap_method<'vtab, T, F>(
    func: F,
) -> Box<dyn Fn(&'vtab T, &InternalContext, &mut [&mut ValueRef]) + 'vtab>
where
    T: VTab<'vtab>,
    F: Fn(&'vtab T, &Context, &mut [&mut ValueRef]) -> Result<()> + 'vtab,
{
    Box::new(
        move |t: &T, ic: &InternalContext, a: &mut [&mut ValueRef]| {
            let ctx = unsafe { Context::from_ptr(ic.as_ptr()) };
            if let Err(e) = func(t, ctx, a) {
                ctx.set_result(e).unwrap();
            }
        },
    )
}

struct VTabFunction<'vtab, T: VTab<'vtab> + ?Sized> {
    n_args: i32,
    name: Cow<'vtab, str>,
    constraint: Option<ConstraintOp>,
    vtab: Cell<Option<&'vtab T>>,
    func: Box<dyn Fn(&'vtab T, &InternalContext, &mut [&mut ValueRef]) + 'vtab>,
}

impl<'vtab, T: VTab<'vtab>> VTabFunction<'vtab, T> {
    pub fn new(
        n_args: i32,
        name: impl Into<Cow<'vtab, str>>,
        constraint: Option<ConstraintOp>,
        func: Box<dyn Fn(&'vtab T, &InternalContext, &mut [&mut ValueRef]) + 'vtab>,
    ) -> Pin<Box<Self>> {
        Box::pin(Self {
            n_args,
            name: name.into(),
            constraint,
            vtab: Cell::new(None),
            func,
        })
    }

    pub fn bind(&self, vtab: &'vtab T) -> (CFunc, *mut ::std::os::raw::c_void) {
        self.vtab.set(Some(vtab));
        (call_vtab_method::<T>, self as *const Self as *mut Self as _)
    }

    pub fn invoke(&self, ic: &InternalContext, a: &mut [&mut ValueRef]) {
        (*self.func)(self.vtab.get().unwrap(), ic, a);
    }
}

unsafe extern "C" fn call_vtab_method<'vtab, T>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) where
    T: VTab<'vtab> + 'vtab,
{
    let ic = InternalContext::from_ptr(context);
    let vtab_function = ic.user_data::<VTabFunction<'vtab, T>>();
    let args = slice::from_raw_parts_mut(argv as *mut &mut ValueRef, argc as _);
    vtab_function.invoke(ic, args);
}
