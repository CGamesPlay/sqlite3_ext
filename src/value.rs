use super::{ffi, types::*};
use std::{slice, str};

#[derive(Debug, PartialEq)]
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
}

/// Stores an SQLite-compatible value owned by Rust code.
#[derive(Debug)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Text(String),
    Blob(Vec<u8>),
    Null,
}

impl ValueRef {
    fn as_ptr(&self) -> *mut ffi::sqlite3_value {
        &self.base as *const ffi::sqlite3_value as _
    }

    /*
    /// Create a copy of the referenced value.
    pub fn to_value(&self) -> Value {
        match self.value_type() {
            ValueType::Integer => Value::Integer(self.get_i64()),
            ValueType::Float => Value::Float(self.get_f64()),
            ValueType::Text => todo!(),
            ValueType::Blob => todo!(),
            ValueType::Null => Value::Null,
        }
    }
    */

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
                    return Err(Error::no_memory());
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
    /// If the underlying value is not a BLOB, the behavior of this function is undefined.
    pub unsafe fn get_blob_unchecked(&self) -> &[u8] {
        let len = ffi::sqlite3_value_bytes(self.as_ptr());
        let data = ffi::sqlite3_value_blob(self.as_ptr());
        slice::from_raw_parts(data as _, len as _)
    }

    /// Interpret the result as `Option<&str>`.
    ///
    /// This method will fail if SQLite runs out of memory while converting the value, or
    /// if the value has invalid UTF-8. The returned value is `None` if the underlying
    /// value is SQL NULL.
    pub fn get_str(&mut self) -> Result<Option<&str>> {
        self.get_blob()?
            .map(|b| str::from_utf8(b))
            .transpose()
            .map_err(Error::Utf8Error)
    }

    /// Get the underlying TEXT value.
    ///
    /// # Safety
    ///
    /// If the underlying value is not TEXT, the behavior of this function is undefined.
    pub unsafe fn get_str_unchecked(&self) -> &str {
        str::from_utf8_unchecked(self.get_blob_unchecked())
    }
}

impl std::fmt::Debug for ValueRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self.value_type() {
            ValueType::Integer => f
                .debug_tuple("ValueRef::Integer")
                .field(&self.get_i64())
                .finish(),
            ValueType::Float => f
                .debug_tuple("ValueRef::Float")
                .field(&self.get_f64())
                .finish(),
            ValueType::Text => f
                .debug_tuple("ValueRef::Text")
                .field(unsafe { &self.get_str_unchecked() })
                .finish(),
            ValueType::Blob => f
                .debug_tuple("ValueRef::Blob")
                .field(unsafe { &self.get_blob_unchecked() })
                .finish(),
            ValueType::Null => f.debug_tuple("ValueRef::Null").finish(),
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
value_from!(Vec<u8> as (x) => Value::Blob(x));
value_from!(() as (_x) => Value::Null);
