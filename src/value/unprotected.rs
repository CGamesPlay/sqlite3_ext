use super::ValueRef;
use crate::ffi;

/// Contains an unprotected SQLite value.
///
/// An [unprotected value object](https://www.sqlite.org/c3ref/value.html) means that the value
/// object is not protected by the SQLite mutex, and is therefore not threadsafe.
///
/// This struct is primary useful to pass the output of an SQLite query unchanged to some other
/// SQLite interface (either binding it to a query or returning it from a virtual table or
/// application-defined function). It is the most performant way to do so, as it requires no
/// copying.
#[repr(transparent)]
pub struct UnprotectedValue {
    base: *mut ffi::sqlite3_value,
}

impl UnprotectedValue {
    pub(crate) fn from_ptr(base: *mut ffi::sqlite3_value) -> Self {
        Self { base }
    }

    /// Return the underlying sqlite3_value pointer.
    #[inline]
    pub unsafe fn as_ptr(&self) -> *mut ffi::sqlite3_value {
        self.base
    }

    /// Convert this UnprotectedValue into an ordinary [ValueRef].
    ///
    /// # Safety
    ///
    /// The returned reference is not thread safe. This method is safe to use inside of
    /// virtual table methods and application-defined functions, or when SQLite is
    /// operating in single-threaded mode.
    pub unsafe fn as_value_ref(&mut self) -> &mut ValueRef {
        ValueRef::from_ptr(self.base)
    }
}
