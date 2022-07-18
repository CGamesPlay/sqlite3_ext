use crate::{ffi, sqlite3_match_version, types::*, value::*};
use std::{
    mem::{size_of, zeroed},
    ptr::write_unaligned,
};

/// Pass arbitrary pointers through SQLite as BLOBs.
///
/// Using this technique to pass pointers through SQLite is insecure and error-prone. A much
/// better solution is available via [PassedRef]. Pointers passed through this
/// interface require manual memory management, for example using [Box::into_raw] or
/// [std::mem::forget].
///
/// # Examples
///
/// This example uses static memory to avoid memory management.
///
/// ```no_run
/// use sqlite3_ext::{UnsafePtr, function::Context, Result, ValueRef};
///
/// const VAL: &str = "static memory";
/// const SUBTYPE: u8 = 'S' as _;
///
/// fn produce_ptr(ctx: &Context, args: &mut [&mut ValueRef]) -> UnsafePtr<str> {
///     UnsafePtr::new(VAL, SUBTYPE)
/// }
///
/// fn consume_ptr(ctx: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
///     let val: &str = unsafe { &*UnsafePtr::from_value_ref(args[0], SUBTYPE)?.get() };
///     assert_eq!(val, "static memory");
///     Ok(())
/// }
/// ```
///
/// This example uses a boxed value to manage memory:
///
/// ```no_run
/// use sqlite3_ext::{UnsafePtr, function::Context, Result, ValueRef};
///
/// const SUBTYPE: u8 = 'S' as _;
///
/// fn produce_ptr(ctx: &Context, args: &mut [&mut ValueRef]) -> UnsafePtr<u64> {
///     let val = Box::new(100u64);
///     UnsafePtr::new(Box::into_raw(val), SUBTYPE)
/// }
///
/// fn consume_ptr(ctx: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
///     let val: Box<u64> =
///         unsafe { Box::from_raw(UnsafePtr::from_value_ref(args[0], SUBTYPE)?.get_mut()) };
///     assert_eq!(*val, 100);
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct UnsafePtr<T: ?Sized> {
    #[cfg_attr(not(modern_sqlite), allow(unused))]
    pub(crate) subtype: u8,
    ptr: *const T,
}

impl<T: ?Sized> UnsafePtr<T> {
    /// Create a new UnsafePtr with the given subtype.
    ///
    /// Subtype verification requires SQLite 3.9.0. On earlier versions of SQLite, the
    /// subtype field is ignored.
    pub fn new(ptr: *const T, subtype: u8) -> Self {
        assert!(subtype != 0, "subtype must not be 0");
        Self { subtype, ptr }
    }

    /// Retrieve an UnsafePtr from a ValueRef.
    ///
    /// The subtype provided to this method must match the subtype originally provided to
    /// UnsafeRef.
    ///
    /// This method will fail if the value cannot be interpreted as a pointer. It will
    /// create a null pointer if the value is SQL NULL.
    ///
    /// Subtype verification requires SQLite 3.9.0. On earlier versions of SQLite, the
    /// subtype field is ignored.
    pub fn from_value_ref(val: &mut ValueRef, subtype: u8) -> Result<Self> {
        unsafe {
            let len = ffi::sqlite3_value_bytes(val.as_ptr()) as usize;
            let subtype_match = sqlite3_match_version! {
                3_009_000 => ffi::sqlite3_value_subtype(val.as_ptr()) as u8 == subtype,
                _ => subtype == subtype, // suppress unused warning on subtype
            };
            if len == 0 {
                Ok(UnsafePtr {
                    ptr: zeroed(),
                    subtype,
                })
            } else if len != size_of::<&T>() || !subtype_match {
                Err(SQLITE_MISMATCH)
            } else {
                let bits = ffi::sqlite3_value_blob(val.as_ptr()) as *const *const T;
                let ret = ptr::read_unaligned::<*const T>(bits);
                Ok(UnsafePtr { ptr: ret, subtype })
            }
        }
    }

    /// Get the stored pointer.
    pub fn get(&self) -> *const T {
        self.ptr
    }

    /// Get the stored pointer, mutably.
    pub fn get_mut(&mut self) -> *mut T {
        self.ptr as _
    }

    pub(crate) fn into_blob(self) -> Blob {
        let len = size_of::<&T>();
        let mut vec: Vec<u8> = Vec::with_capacity(len);
        unsafe {
            vec.set_len(len);
            let ret_bytes = vec.as_mut_ptr() as *mut *const T;
            write_unaligned(ret_bytes, self.ptr);
        }
        Blob::from(vec.as_slice())
    }
}

#[cfg(all(test, feature = "static"))]
mod test {
    use crate::test_helpers::prelude::*;
    use std::mem::{size_of, size_of_val};

    const SUBTYPE: u8 = 't' as _;

    #[test]
    fn get_ptr() {
        let h = TestHelpers::new();
        let owned_string = "input string".to_owned();
        let ptr = Box::into_raw(Box::new(owned_string));
        let ptr = UnsafePtr::new(ptr, SUBTYPE);
        h.with_value(ptr, |val| {
            assert_eq!(val.value_type(), ValueType::Blob);
            let borrowed_string: Box<String> =
                unsafe { Box::from_raw(UnsafePtr::from_value_ref(val, SUBTYPE)?.get_mut()) };
            assert_eq!(*borrowed_string, "input string");
            Ok(())
        });
    }

    #[test]
    fn get_ptr_wide() {
        let h = TestHelpers::new();
        let val: &str = "static string";
        let ptr = UnsafePtr::new(val, SUBTYPE);
        assert_ne!(
            size_of_val(&ptr),
            size_of::<UnsafePtr<()>>(),
            "this isn't a wide pointer"
        );
        h.with_value(ptr, |val| {
            assert_eq!(val.value_type(), ValueType::Blob);
            let borrowed_slice: &str = unsafe { &*UnsafePtr::from_value_ref(val, SUBTYPE)?.get() };
            assert_eq!(borrowed_slice, "static string");
            Ok(())
        });
    }

    #[test]
    fn get_ptr_null() {
        let h = TestHelpers::new();
        let null: Option<i64> = None;
        h.with_value(null, |val| {
            assert_eq!(val.value_type(), ValueType::Null);
            let ptr: *const () = UnsafePtr::from_value_ref(val, SUBTYPE)?.get();
            assert!(ptr.is_null(), "ptr should be null");
            Ok(())
        });
    }

    #[test]
    fn get_ptr_invalid() {
        let h = TestHelpers::new();
        h.with_value(Blob::from([1, 2, 3]), |val| {
            assert_eq!(val.value_type(), ValueType::Blob);
            UnsafePtr::<()>::from_value_ref(val, SUBTYPE).expect_err("incorrect length");
            Ok(())
        });
    }

    #[test]
    #[cfg(modern_sqlite)]
    fn get_ptr_invalid_subtype() {
        let h = TestHelpers::new();
        let owned_string = "input string".to_owned();
        let ptr = Box::into_raw(Box::new(owned_string));
        let ptr = UnsafePtr::new(ptr, SUBTYPE);
        h.with_value(ptr, |val| {
            assert_eq!(val.value_type(), ValueType::Blob);
            UnsafePtr::<String>::from_value_ref(val, !SUBTYPE).expect_err("invalid subtype");
            Ok(())
        });
    }
}
