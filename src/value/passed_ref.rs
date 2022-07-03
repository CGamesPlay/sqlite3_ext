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
/// object from an application-defined function has no effect.
pub struct PassedRef {
    type_id: TypeId,
    value: Box<dyn Any>,
}

impl PassedRef {
    pub fn new<T: 'static>(val: T) -> PassedRef {
        PassedRef {
            type_id: val.type_id(),
            value: Box::new(val),
        }
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        if TypeId::of::<T>() == self.type_id {
            self.value.downcast_ref()
        } else {
            None
        }
    }

    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        if TypeId::of::<T>() == self.type_id {
            self.value.downcast_mut()
        } else {
            None
        }
    }
}

impl std::fmt::Debug for PassedRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("PassedRef")
            .field("type_id", &self.type_id)
            .finish_non_exhaustive()
    }
}
