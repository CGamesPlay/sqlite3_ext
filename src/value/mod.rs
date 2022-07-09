use super::{ffi, sqlite3_match_version, types::*};
pub use blob::*;
pub use passed_ref::*;
use std::{marker::PhantomData, ptr, slice, str};
pub use unsafe_ptr::*;
pub use value_list::*;

mod blob;
mod passed_ref;
mod test;
mod unsafe_ptr;
mod value_list;

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ValueType {
    Integer,
    Float,
    Text,
    Blob,
    Null,
}

/// Stores a protected SQL value. SQLite always owns all value objects, so there is no way to directly
/// create one.
///
/// "Protected" means that SQLite holds a mutex for the lifetime of the reference.
///
/// SQLite automatically converts data to any requested type where possible. This conversion is
/// typically done in-place, which is why many of the conversion methods of this type require
/// `&mut`.
#[repr(transparent)]
pub struct ValueRef {
    base: ffi::sqlite3_value,
    // Values are not safe to send between threads.
    phantom: PhantomData<*const ffi::sqlite3_value>,
}

/// Stores an SQLite-compatible value owned by Rust code.
#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Text(String),
    Blob(Blob),
    Null,
}

impl ValueRef {
    #[cfg_attr(not(modern_sqlite), allow(unused))]
    pub(crate) unsafe fn from_ptr<'a>(p: *mut ffi::sqlite3_value) -> &'a mut ValueRef {
        &mut *(p as *mut ValueRef)
    }

    /// Get the underlying SQLite handle.
    ///
    /// # Safety
    ///
    /// Invoking SQLite methods on the returned value maay invalidate existing references
    /// previously returned by this object. This is safe as long as a mutable reference to
    /// this ValueRef is held.
    pub unsafe fn as_ptr(&self) -> *mut ffi::sqlite3_value {
        &self.base as *const ffi::sqlite3_value as _
    }

    pub fn value_type(&self) -> ValueType {
        unsafe {
            match ffi::sqlite3_value_type(self.as_ptr() as _) {
                ffi::SQLITE_INTEGER => ValueType::Integer,
                ffi::SQLITE_FLOAT => ValueType::Float,
                ffi::SQLITE_TEXT => ValueType::Text,
                ffi::SQLITE_BLOB => ValueType::Blob,
                ffi::SQLITE_NULL => ValueType::Null,
                _ => unreachable!(),
            }
        }
    }

    /// Convenience method equivalent to `self.value_type() == ValueType::Null`.
    pub fn is_null(&self) -> bool {
        self.value_type() == ValueType::Null
    }

    /// Return true if the value is unchanged by an UPDATE operation. Specifically, this method is guaranteed to return true if all of the following are true:
    ///
    /// - this ValueRef is a parameter to an [UpdateVTab](crate::vtab::UpdateVTab) method;
    /// - during the corresponding call to
    ///   [VTabCursor::column](crate::vtab::VTabCursor::column),
    ///   [ColumnContext::nochange](crate::vtab::ColumnContext::nochange) returned true; and
    /// - the column method failed with [Error::NoChange](crate::Error::NoChange).
    ///
    /// If this method returns true under these circumstances, then the value will appear to be SQL NULL, and the UpdateVTab method
    /// must not change the underlying value.
    ///
    /// Requires SQLite 3.22.0. On earlier versions of SQLite, this function will always
    /// return false.
    pub fn nochange(&self) -> bool {
        sqlite3_match_version! {
            3_022_000 => (unsafe { ffi::sqlite3_value_nochange(self.as_ptr()) } != 0),
            _ => false,
        }
    }

    pub fn get_i64(&self) -> i64 {
        unsafe { ffi::sqlite3_value_int64(self.as_ptr()) }
    }

    pub fn get_f64(&self) -> f64 {
        unsafe { ffi::sqlite3_value_double(self.as_ptr()) }
    }

    /// Interpret this value as a BLOB.
    pub fn get_blob(&mut self) -> Result<Option<&[u8]>> {
        unsafe {
            let data = ffi::sqlite3_value_blob(self.as_ptr());
            let len = ffi::sqlite3_value_bytes(self.as_ptr());
            if data.is_null() {
                if self.value_type() == ValueType::Null {
                    return Ok(None);
                } else {
                    return Err(SQLITE_NOMEM);
                }
            } else {
                Ok(Some(slice::from_raw_parts(data as _, len as _)))
            }
        }
    }

    /// Get the bytes of this BLOB value.
    ///
    /// # Safety
    ///
    /// If the type of this value is not BLOB, the behavior of this function is undefined.
    pub unsafe fn get_blob_unchecked(&self) -> &[u8] {
        let len = ffi::sqlite3_value_bytes(self.as_ptr());
        let data = ffi::sqlite3_value_blob(self.as_ptr());
        slice::from_raw_parts(data as _, len as _)
    }

    /// Interpret the value as `Option<&str>`.
    ///
    /// This method will fail if SQLite runs out of memory while converting the value, or
    /// if the value has invalid UTF-8. The returned value is `None` if the underlying
    /// value is SQL NULL.
    pub fn get_str(&mut self) -> Result<Option<&str>> {
        Ok(self.get_blob()?.map(|b| str::from_utf8(b)).transpose()?)
    }

    /// Get the underlying TEXT value.
    ///
    /// This method will fail if the value has invalid UTF-8.
    ///
    /// # Safety
    ///
    /// If the type of this value is not TEXT, the behavior of this function is undefined.
    pub unsafe fn get_str_unchecked(&self) -> Result<&str> {
        Ok(str::from_utf8(self.get_blob_unchecked())?)
    }

    // Caller is responsible for enforcing Rust pointer aliasing rules.
    unsafe fn get_ref_internal<T: 'static>(&self) -> Option<&mut PassedRef<T>> {
        sqlite3_match_version! {
            3_020_000 => (ffi::sqlite3_value_pointer(self.as_ptr(), POINTER_TAG) as *mut PassedRef<T>).as_mut(),
            _ => None,
        }
    }

    /// Get the [PassedRef] stored in this value.
    ///
    /// This is a safe way of passing arbitrary Rust objects through SQLite, however it
    /// requires SQLite 3.20.0 to work. On older versions of SQLite, this function will
    /// always return None.
    ///
    /// The mutable version is [get_mut_ref](Self::get_mut_ref).
    ///
    /// Requires SQLite 3.20.0. On earlier versions of SQLite, this function will always
    /// return None.
    pub fn get_ref<T: 'static>(&self) -> Option<&T> {
        unsafe { self.get_ref_internal::<T>() }
            .map(|x| PassedRef::get(x))
            .unwrap_or(None)
    }

    /// Mutable version of [get_ref](ValueRef::get_ref).
    ///
    /// Requires SQLite 3.20.0. On earlier versions of SQLite, this function will always
    /// return None.
    pub fn get_mut_ref<T: 'static>(&mut self) -> Option<&mut T> {
        unsafe { self.get_ref_internal::<T>() }
            .map(PassedRef::get_mut)
            .unwrap_or(None)
    }
}

impl std::fmt::Debug for ValueRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self.value_type() {
            ValueType::Integer => f.debug_tuple("Integer").field(&self.get_i64()).finish(),
            ValueType::Float => f.debug_tuple("Float").field(&self.get_f64()).finish(),
            ValueType::Text => f
                .debug_tuple("Text")
                .field(unsafe { &self.get_str_unchecked() })
                .finish(),
            ValueType::Blob => f
                .debug_tuple("Blob")
                .field(unsafe { &self.get_blob_unchecked() })
                .finish(),
            ValueType::Null => {
                if let Some(r) = unsafe { self.get_ref_internal::<()>() } {
                    f.debug_tuple("Null").field(&r).finish()
                } else {
                    f.debug_tuple("Null").finish()
                }
            }
        }
    }
}

macro_rules! value_from {
    ($ty:ty as ($x:ident) => $impl:expr) => {
        impl From<$ty> for Value {
            fn from($x: $ty) -> Value {
                $impl
            }
        }
    };
}

value_from!(i32 as (x) => Value::Integer(x as _));
value_from!(i64 as (x) => Value::Integer(x));
value_from!(f64 as (x) => Value::Float(x));
value_from!(String as (x) => Value::Text(x));
value_from!(Blob as (x) => Value::Blob(x));
value_from!(() as (_x) => Value::Null);
