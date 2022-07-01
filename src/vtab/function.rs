use super::{
    super::{
        ffi,
        function::{Context, InternalContext, ToContextResult},
        value::*,
    },
    ConstraintOp, VTab,
};
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    marker::PhantomData,
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
    list: RefCell<Vec<Pin<Box<dyn VTabFunction<'vtab, T> + 'vtab>>>>,
}

pub(crate) trait VTabFunction<'vtab, T: VTab<'vtab>> {
    fn n_args(&self) -> i32;
    fn name(&self) -> &Cow<'vtab, str>;
    fn constraint(&self) -> Option<ConstraintOp>;
    fn bind(&self, vtab: &'vtab T) -> (CFunc, *mut ::std::os::raw::c_void);
}

impl<'vtab, T: VTab<'vtab> + ?Sized> Default for VTabFunctionList<'vtab, T> {
    fn default() -> Self {
        Self {
            list: RefCell::new(Vec::new()),
        }
    }
}

impl<'vtab, T: VTab<'vtab> + 'vtab> VTabFunctionList<'vtab, T> {
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
    pub fn add<R: ToContextResult + 'vtab, F: Fn(&Context, &mut [&mut ValueRef]) -> R + 'vtab>(
        &self,
        n_args: i32,
        name: impl Into<Cow<'vtab, str>>,
        constraint: Option<ConstraintOp>,
        func: F,
    ) {
        self.list
            .borrow_mut()
            .push(VTabFunctionFree::new(n_args, name, constraint, func));
    }

    /// Add a method to the list.
    ///
    /// This function works similarly to [add](VTabFunctionList::add), except the function
    /// will receive the virtual table as the first parameter.
    pub fn add_method<
        R: ToContextResult + 'vtab,
        F: Fn(&'vtab T, &Context, &mut [&mut ValueRef]) -> R + 'vtab,
    >(
        &self,
        n_args: i32,
        name: impl Into<Cow<'vtab, str>>,
        constraint: Option<ConstraintOp>,
        func: F,
    ) {
        self.list
            .borrow_mut()
            .push(VTabFunctionMethod::new(n_args, name, constraint, func));
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
        let found = [n_args, -1].into_iter().find_map(|n_args| {
            list.iter()
                .find(|f| f.n_args() == n_args && f.name() == name)
        });
        found.map(|r| (r.bind(vtab), r.constraint()))
    }
}

macro_rules! declare_vtab_function {
    (
        $name:ident,
        ( $t:ident, $($ty:tt)* ),
        { $($field_name:ident: $field_type:ty),* },
        { $($field_init:ident: $field_ctor:expr)* },
        | $self:tt, $vtab:tt | $expr:expr,
        $func:ident
     ) => {
        struct $name<'vtab, $t: VTab<'vtab>, R: ToContextResult, F: $($ty)*> {
            n_args: i32,
            name: Cow<'vtab, str>,
            constraint: Option<ConstraintOp>,
            func: F,
            $($field_name: $field_type),*
        }

        impl<'vtab, $t: VTab<'vtab> + 'vtab, R: ToContextResult + 'vtab, F: $($ty)* + 'vtab>
            $name<'vtab, $t, R, F>
        {
            pub fn new(
                n_args: i32,
                name: impl Into<Cow<'vtab, str>>,
                constraint: Option<ConstraintOp>,
                func: F,
            ) -> Pin<Box<dyn VTabFunction<'vtab, $t> + 'vtab>> {
                assert!((-1..128).contains(&n_args), "n_args invalid");
                if let Some(c) = &constraint {
                    c.assert_valid_function_constraint();
                }
                Box::pin($name {
                    n_args,
                    name: name.into(),
                    constraint,
                    func,
                    $($field_init: $field_ctor),*
                })
            }
        }

        impl<'vtab, $t: VTab<'vtab>, R: ToContextResult, F: $($ty)*> VTabFunction<'vtab, $t>
            for $name<'vtab, $t, R, F>
        {
            fn n_args(&self) -> i32 {
                self.n_args
            }

            fn name(&self) -> &Cow<'vtab, str> {
                &self.name
            }

            fn constraint(&self) -> Option<ConstraintOp> {
                self.constraint
            }

            fn bind(&self, $vtab: &'vtab $t) -> (CFunc, *mut ::std::os::raw::c_void) {
                let $self = self;
                $expr;
                ($func::<$t, R, F>, self as *const _ as _)
            }
        }
    };
}

declare_vtab_function!(
    VTabFunctionFree,
    (T, Fn(&Context, &mut [&mut ValueRef]) -> R),
    { phantom: PhantomData<T> },
    { phantom: PhantomData },
    |_, _| (),
    call_vtab_free
);
declare_vtab_function!(
    VTabFunctionMethod,
    (T, Fn(&'vtab T, &Context, &mut [&mut ValueRef]) -> R),
    { vtab: Cell<Option<&'vtab T>> },
    { vtab: Cell::new(None) },
    |s, vtab| s.vtab.set(Some(vtab)),
    call_vtab_method
);

unsafe extern "C" fn call_vtab_free<
    'vtab,
    T: VTab<'vtab> + 'vtab,
    R: ToContextResult,
    F: Fn(&Context, &mut [&mut ValueRef]) -> R,
>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let ic = InternalContext::from_ptr(context);
    let vtab_function = ic.user_data::<VTabFunctionFree<'vtab, T, R, F>>();
    let ctx = Context::from_ptr(context);
    let args = slice::from_raw_parts_mut(argv as *mut &mut ValueRef, argc as _);
    let ret = (vtab_function.func)(ctx, args);
    ic.set_result(ret);
}

unsafe extern "C" fn call_vtab_method<
    'vtab,
    T: VTab<'vtab> + 'vtab,
    R: ToContextResult,
    F: Fn(&'vtab T, &Context, &mut [&mut ValueRef]) -> R,
>(
    context: *mut ffi::sqlite3_context,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let ic = InternalContext::from_ptr(context);
    let vtab_function = ic.user_data::<VTabFunctionMethod<'vtab, T, R, F>>();
    let ctx = Context::from_ptr(context);
    let args = slice::from_raw_parts_mut(argv as *mut &mut ValueRef, argc as _);
    let ret = (vtab_function.func)(vtab_function.vtab.get().unwrap(), ctx, args);
    ic.set_result(ret);
}
