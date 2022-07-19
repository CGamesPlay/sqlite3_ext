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

impl ValueType {
    pub(crate) fn from_sqlite(val: i32) -> ValueType {
        match val {
            ffi::SQLITE_INTEGER => ValueType::Integer,
            ffi::SQLITE_FLOAT => ValueType::Float,
            ffi::SQLITE_TEXT => ValueType::Text,
            ffi::SQLITE_BLOB => ValueType::Blob,
            ffi::SQLITE_NULL => ValueType::Null,
            _ => unreachable!(),
        }
    }
}

/// Allows access to an underlying SQLite value.
// This trait isn't really useful as a trait bound anywhere, and only serves to ensure that
// ValueRef and Column provide a similar interface. Column could Deref to Value, but this
// actually involves an internal conversion of the held value (the MEM_Static flag becomes
// MEM_Ephem), and I'm not sure what kind of performance penalty that would bring.
pub trait FromValue {
    /// Returns the data type of the ValueRef. Note that calling get methods on the
    /// ValueRef may cause a conversion to a different data type, but this is not
    /// guaranteed.
    fn value_type(&self) -> ValueType;

    /// Convenience method equivalent to `self.value_type() == ValueType::Null`.
    fn is_null(&self) -> bool {
        self.value_type() == ValueType::Null
    }

    /// Interpret this value as i32.
    fn get_i32(&self) -> i32;

    /// Interpret this value as i64.
    fn get_i64(&self) -> i64;

    /// Interpret this value as f64.
    fn get_f64(&self) -> f64;

    /// Get the bytes of this BLOB value.
    ///
    /// # Safety
    ///
    /// If the type of this value is not BLOB, the behavior of this function is undefined.
    unsafe fn get_blob_unchecked(&self) -> &[u8];

    /// Interpret this value as a BLOB.
    fn get_blob(&mut self) -> Result<Option<&[u8]>>;

    /// Get the underlying TEXT value.
    ///
    /// This method will fail if the value has invalid UTF-8.
    ///
    /// # Safety
    ///
    /// If the type of this value is not TEXT, the behavior of this function is undefined.
    unsafe fn get_str_unchecked(&self) -> Result<&str> {
        Ok(str::from_utf8(self.get_blob_unchecked())?)
    }

    /// Interpret the value as `Option<&str>`.
    ///
    /// This method will fail if SQLite runs out of memory while converting the value, or
    /// if the value has invalid UTF-8. The returned value is `None` if the underlying
    /// value is SQL NULL.
    fn get_str(&mut self) -> Result<Option<&str>> {
        Ok(self.get_blob()?.map(|b| str::from_utf8(b)).transpose()?)
    }

    /// Clone the value, returning a [Value].
    fn to_owned(&self) -> Result<Value> {
        match self.value_type() {
            ValueType::Integer => Ok(Value::from(self.get_i64())),
            ValueType::Float => Ok(Value::from(self.get_f64())),
            ValueType::Text => unsafe { Ok(Value::from(self.get_str_unchecked()?.to_owned())) },
            ValueType::Blob => unsafe { Ok(Value::from(Blob::from(self.get_blob_unchecked()))) },
            ValueType::Null => Ok(Value::Null),
        }
    }
}

/// A protected SQL value.
///
/// SQLite always owns all value objects. Consequently, this struct is never owned by Rust
/// code, but instead always borrowed. A "protected" value means that SQLite holds a mutex for
/// the lifetime of the reference.
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

impl ValueRef {
    #[cfg_attr(not(modern_sqlite), allow(unused))]
    pub(crate) unsafe fn from_ptr<'a>(p: *mut ffi::sqlite3_value) -> &'a mut ValueRef {
        &mut *(p as *mut ValueRef)
    }

    /// Get the underlying SQLite handle.
    ///
    /// # Safety
    ///
    /// Invoking SQLite methods on the returned value may invalidate existing references
    /// previously returned by this object. This is safe as long as a mutable reference to
    /// this ValueRef is held.
    pub unsafe fn as_ptr(&self) -> *mut ffi::sqlite3_value {
        &self.base as *const ffi::sqlite3_value as _
    }

    /// Attempt to convert the ValueRef to a numeric data type, and return the resulting
    /// data type. This conversion will only happen if it is losles, otherwise the
    /// underlying value will remain its original type.
    pub fn numeric_type(&mut self) -> ValueType {
        unsafe { ValueType::from_sqlite(ffi::sqlite3_value_numeric_type(self.as_ptr())) }
    }

    /// Returns true if this ValueRef originated from one of the sqlite3_bind interfaces.
    /// If it comes from an SQL literal value, or a table column, or an expression, then
    /// this method returns false.
    ///
    /// Requires SQLite 3.28.0. On earlier versions, this method always returns false.
    pub fn is_from_bind(&self) -> bool {
        sqlite3_match_version! {
            3_028_000 => unsafe { ffi::sqlite3_value_frombind(self.as_ptr()) != 0 },
            _ => false,
        }
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

impl FromValue for ValueRef {
    fn value_type(&self) -> ValueType {
        unsafe { ValueType::from_sqlite(ffi::sqlite3_value_type(self.as_ptr())) }
    }

    fn get_i32(&self) -> i32 {
        unsafe { ffi::sqlite3_value_int(self.as_ptr()) }
    }

    fn get_i64(&self) -> i64 {
        unsafe { ffi::sqlite3_value_int64(self.as_ptr()) }
    }

    fn get_f64(&self) -> f64 {
        unsafe { ffi::sqlite3_value_double(self.as_ptr()) }
    }

    unsafe fn get_blob_unchecked(&self) -> &[u8] {
        let len = ffi::sqlite3_value_bytes(self.as_ptr());
        let data = ffi::sqlite3_value_blob(self.as_ptr());
        slice::from_raw_parts(data as _, len as _)
    }

    fn get_blob(&mut self) -> Result<Option<&[u8]>> {
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

/// Stores an SQLite-compatible value owned by Rust code.
#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Text(String),
    Blob(Blob),
    Null,
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
