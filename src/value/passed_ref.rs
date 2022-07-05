#![cfg(modern_sqlite)]
use std::any::{Any, TypeId};

pub(crate) const POINTER_TAG: *const i8 = b"sqlite3_ext:PassedRef\0".as_ptr() as _;

/// Pass arbitrary values through SQLite.
///
/// Values of this type can be returned by SQL functions, and later retrieved using
/// [ValueRef::get_ref](super::ValueRef::get_ref).
///
/// This mechanism relies on [std::any::Any] to ensure type safety, which requires that values
/// are `'static`.
///
/// This feature requires SQLite 3.20.0. On earlier versions of SQLite, returning a PassedRef
/// object from an application-defined function has no effect. If supporting older versions of
/// SQLite is required, [UnsafePtr](super::UnsafePtr) can be used instead.
///
/// # Examples
///
/// ```no_run
/// use sqlite3_ext::{PassedRef, function::Context, Result, ValueRef};
///
/// fn produce_ref(ctx: &Context, args: &mut [&mut ValueRef]) -> PassedRef<String> {
///     let val = "owned string".to_owned();
///     PassedRef::new(val)
/// }
///
/// fn consume_ref(ctx: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
///     let val = args[0].get_ref::<String>().unwrap();
///     assert_eq!(val, "owned string");
///     Ok(())
/// }
/// ```
#[repr(C)]
pub struct PassedRef<T: 'static> {
    type_id: TypeId,
    value: T,
}

impl<T: 'static> PassedRef<T> {
    /// Create a new PassedRef containing the value.
    pub fn new(value: T) -> PassedRef<T> {
        PassedRef {
            type_id: value.type_id(),
            value,
        }
    }

    pub(crate) fn get(&self) -> Option<&T> {
        if TypeId::of::<T>() == self.type_id {
            Some(&self.value)
        } else {
            None
        }
    }

    pub(crate) fn get_mut(&mut self) -> Option<&mut T> {
        if TypeId::of::<T>() == self.type_id {
            Some(&mut self.value)
        } else {
            None
        }
    }
}

impl<T: 'static> std::fmt::Debug for PassedRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("PassedRef")
            .field("type_id", &self.type_id)
            .finish_non_exhaustive()
    }
}
