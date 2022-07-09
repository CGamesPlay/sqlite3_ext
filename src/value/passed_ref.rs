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

#[cfg(all(modern_sqlite, test, feature = "static"))]
mod test {
    use crate::test_helpers::prelude::*;

    #[test]
    fn get_ref() {
        let h = TestHelpers::new();
        #[derive(PartialEq, Debug)]
        struct MyStruct {
            s: String,
        }
        let owned_struct = MyStruct {
            s: "input string".to_owned(),
        };
        h.with_value(PassedRef::new(owned_struct), |val| {
            assert_eq!(val.value_type(), ValueType::Null);
            assert_eq!(
                val.get_ref::<MyStruct>(),
                Some(&MyStruct {
                    s: "input string".to_owned()
                })
            );
            let mut dbg = format!("{:?}", val);
            dbg.replace_range(38..(dbg.len() - 9), "XXX");
            assert_eq!(dbg, "Null(PassedRef { type_id: TypeId { t: XXX }, .. })");
            Ok(())
        });
    }

    #[test]
    fn get_ref_invalid() {
        let h = TestHelpers::new();
        h.with_value(PassedRef::new(0i32), |val| {
            assert_eq!(val.value_type(), ValueType::Null);
            assert_eq!(val.get_ref::<String>(), None);
            Ok(())
        });
    }
}
