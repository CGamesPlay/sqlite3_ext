use super::ffi;

#[repr(transparent)]
pub struct Value {
    base: ffi::sqlite3_value,
}

impl Value {
    pub fn as_ptr(&self) -> *const ffi::sqlite3_value {
        &self.base
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        unsafe {
            match ffi::value_type(self.as_ptr() as _) {
                ffi::SQLITE_NULL => write!(f, "Value::Null"),
                _ => todo!(),
            }
        }
    }
}
