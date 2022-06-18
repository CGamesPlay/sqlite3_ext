use super::ffi;

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
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self.value_type() {
            ValueType::Null => f.debug_tuple("Value::Null").finish(),
            ValueType::Integer => f
                .debug_tuple("Value::Integer")
                .field(&self.get_i64())
                .finish(),
            _ => todo!(),
        }
    }
}
