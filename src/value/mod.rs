use super::{ffi, sqlite3_require_version, types::*};
pub use blob::*;
pub use passed_ref::*;
use std::{
    mem::{size_of, zeroed},
    ptr, slice, str,
};

mod blob;
mod passed_ref;
mod test;

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
    Blob(Blob),
    Null,
}

impl ValueRef {
    /// Get the underlying SQLite handle.
    pub unsafe fn as_ptr(&self) -> *mut ffi::sqlite3_value {
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
            ValueType::Null => {
                // XXX - ref
                Value::Null,
            }
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
    /// If the type of this value is not BLOB, the behavior of this function is undefined.
    pub unsafe fn get_blob_unchecked(&self) -> &[u8] {
        let len = ffi::sqlite3_value_bytes(self.as_ptr());
        let data = ffi::sqlite3_value_blob(self.as_ptr());
        slice::from_raw_parts(data as _, len as _)
    }

    /// Interpret a BLOB as `*const T`.
    ///
    /// Using this technique to pass pointers through SQLite is insecure and error-prone. A
    /// much better solution is available via [get_ref](ValueRef::get_ref). Pointers passed
    /// through this interface require manual memory management, for example using
    /// [Box::into_raw] or [std::mem::forget].
    ///
    /// This method will fail if the underlying value cannot be interpreted as a pointer.
    /// It will return a null pointer if the underlying value is NULL.
    ///
    /// # Examples
    ///
    /// This example uses static memory to avoid memory management. See
    /// [get_mut_ptr](ValueRef::get_mut_ptr) for an example that transfers ownership of a
    /// value.
    ///
    /// ```no_run
    /// use sqlite3_ext::{Blob, function::Context, Result, ValueRef};
    ///
    /// const VAL: &str = "static memory";
    ///
    /// fn produce_ptr(ctx: &Context, args: &mut [&mut ValueRef]) -> Blob {
    ///     Blob::with_ptr(VAL)
    /// }
    ///
    /// fn consume_ptr(ctx: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
    ///     let val = unsafe { args[0].get_ptr::<str>()? };
    ///     assert_eq!(val, "static memory");
    ///     Ok(())
    /// }
    /// ```
    pub fn get_ptr<T: ?Sized>(&mut self) -> Result<*const T> {
        unsafe {
            let len = ffi::sqlite3_value_bytes(self.as_ptr()) as usize;
            if len == 0 {
                Ok(zeroed())
            } else if len != size_of::<&T>() {
                Err(Error::Sqlite(ffi::SQLITE_MISMATCH))
            } else {
                let bits = ffi::sqlite3_value_blob(self.as_ptr()) as *const *const T;
                let ret = ptr::read_unaligned::<*const T>(bits);
                Ok(ret)
            }
        }
    }

    /// Interpret a BLOB as `*mut T`.
    ///
    /// See [get_ptr](ValueRef::get_ptr) for more information.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use sqlite3_ext::{Blob, function::Context, Result, ValueRef};
    ///
    /// fn produce_ptr(ctx: &Context, args: &mut [&mut ValueRef]) -> Blob {
    ///     let val = Box::new(100u64);
    ///     Blob::with_ptr(Box::into_raw(val))
    /// }
    ///
    /// fn consume_ptr(ctx: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
    ///     let val = unsafe { Box::from_raw(args[0].get_mut_ptr::<u64>()?) };
    ///     assert_eq!(*val, 100);
    ///     Ok(())
    /// }
    /// ```
    pub fn get_mut_ptr<T: ?Sized>(&mut self) -> Result<*mut T> {
        self.get_ptr::<T>().map(|p| p as *mut T)
    }

    /// Interpret the value as `Option<&str>`.
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
    /// This method will fail if the value has invalid UTF-8.
    ///
    /// # Safety
    ///
    /// If the type of this value is not TEXT, the behavior of this function is undefined.
    pub unsafe fn get_str_unchecked(&self) -> Result<&str> {
        str::from_utf8(self.get_blob_unchecked()).map_err(Error::Utf8Error)
    }

    /// # Safety
    ///
    /// Caller is responsible for enforcing Rust pointer aliasing rules.
    unsafe fn get_ref_internal(&self) -> Option<&mut PassedRef> {
        sqlite3_require_version!(
            3_020_000,
            (ffi::sqlite3_value_pointer(self.as_ptr(), POINTER_TAG) as *mut PassedRef).as_mut(),
            None
        )
    }

    /// Get the [PassedRef] stored in this value.
    ///
    /// This is a safe way of passing arbitrary Rust objects through SQLite, however it
    /// requires SQLite 3.20.0 to work. On older versions of SQLite, this function will
    /// always return None. If supporting older versions of SQLite is required,
    /// [get_ptr](ValueRef::get_ptr) can be used instead.
    ///
    /// Requires SQLite 3.20.0.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use sqlite3_ext::{PassedRef, function::Context, Result, ValueRef};
    ///
    /// fn produce_ref(ctx: &Context, args: &mut [&mut ValueRef]) -> PassedRef {
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
    pub fn get_ref<T: 'static>(&self) -> Option<&T> {
        unsafe { self.get_ref_internal() }
            .map(|x| PassedRef::get(x))
            .unwrap_or(None)
    }

    /// Mutable version of [get_ref](ValueRef::get_ref).
    ///
    /// Requires SQLite 3.20.0.
    pub fn get_ref_mut<T: 'static>(&mut self) -> Option<&mut T> {
        unsafe { self.get_ref_internal() }
            .map(PassedRef::get_mut)
            .unwrap_or(None)
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
            ValueType::Null => {
                if let Some(r) = unsafe { self.get_ref_internal() } {
                    f.debug_tuple("ValueRef::Null").field(&r).finish()
                } else {
                    f.debug_tuple("ValueRef::Null").finish()
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
