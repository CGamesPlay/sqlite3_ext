use super::{ffi, types::*};
use std::ffi::CStr;

#[derive(Debug, PartialEq)]
pub enum ValueType {
    Integer,
    Float,
    Text,
    Blob,
    Null,
}

/// Stores a SQL value. SQLite always owns all value objects, so there is no way to directly
/// create one.
#[repr(transparent)]
pub struct Value {
    base: ffi::sqlite3_value,
}

impl Value {
    fn as_ptr(&self) -> *mut ffi::sqlite3_value {
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

    pub fn get_i64(&self) -> i64 {
        unsafe { ffi::sqlite3_value_int64(self.as_ptr()) }
    }

    pub fn get_cstr(&self) -> Result<&CStr> {
        let ret = unsafe { ffi::sqlite3_value_text(self.as_ptr()) as *const i8 };
        if ret.is_null() {
            return Err(Error::InvalidConversion);
        }
        let ret = unsafe { CStr::from_ptr(ret) };
        // XXX - check for out of memory
        Ok(ret)
    }

    pub fn get_str(&self) -> Result<&str> {
        self.get_cstr()?.to_str().map_err(|e| Error::Utf8Error(e))
    }

    // XXX - need to figure out how to make this safe. Presently, value_text and value_blob
    // could both be called, but the reference returned by the first one would be
    // invalidated by the second call.
    //
    // Since any value method can result in a type conversion, which puts the value into an
    // indeterminate state, perhaps the get methods should move self?
}

impl From<&Value> for i64 {
    fn from(val: &Value) -> i64 {
        val.get_i64()
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self.value_type() {
            ValueType::Integer => f
                .debug_tuple("Value::Integer")
                .field(&self.get_i64())
                .finish(),
            ValueType::Float => todo!(),
            ValueType::Text => f.debug_tuple("Value::Text").field(&self.get_str()).finish(),
            ValueType::Blob => todo!(),
            ValueType::Null => f.debug_tuple("Value::Null").finish(),
        }
    }
}
