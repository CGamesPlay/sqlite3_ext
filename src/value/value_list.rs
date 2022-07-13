use super::ValueRef;
use crate::{ffi, sqlite3_match_version, sqlite3_require_version, types::*, FallibleIteratorMut};
use std::ptr;

/// Represents a list of values from SQLite.
///
/// SQLite provides these when a virtual table is processing all values of an IN constraint
/// simultaneously, see
/// [IndexInfoConstraint::set_value_list_wanted](crate::vtab::IndexInfoConstraint::set_value_list_wanted)
/// for more information.
///
/// This struct is not an Iterator by itself. There are two recommended ways to interact with
/// it. The first is a `while let`:
///
/// ```no_run
/// use sqlite3_ext::*;
///
/// fn filter_list(list: &mut ValueRef) -> Result<()> {
///     let mut list = ValueList::from_value_ref(list)?;
///     while let Some(x) = list.next()? {
///         println!("value is {:?}", x);
///     }
///     Ok(())
/// }
/// ```
///
/// Alternatively, the [map](Self::map) method turns this struct into a [FallibleIterator]:
///
/// ```no_run
/// use sqlite3_ext::*;
///
/// fn filter_list(list: &mut ValueRef) -> Result<()> {
///     let list: Vec<Option<String>> = ValueList::from_value_ref(list)?
///         .map(|x| Ok(x.get_str()?.map(String::from)))
///         .collect()?;
///     println!("values are {:?}", list);
///     Ok(())
/// }
/// ```
pub struct ValueList<'list> {
    #[cfg_attr(not(modern_sqlite), allow(unused))]
    base: &'list mut ValueRef,
    #[cfg_attr(not(modern_sqlite), allow(unused))]
    pending: Option<Option<ptr::NonNull<ffi::sqlite3_value>>>,
}

impl<'list> ValueList<'list> {
    /// Attempt to create a ValueList from a ValueRef.
    ///
    /// # Safety
    ///
    /// The [SQLite documentation](https://www.sqlite.org/c3ref/vtab_in_first.html) states
    /// that using this method outside of the
    /// [VTabCursor::filter](crate::vtab::VTabCursor::filter) method is "undefined and
    /// probably harmful". However, since the feature's introduction, the underlying
    /// mechanism has always (as of SQLite 3.38.5) used the [pointer passing
    /// interface](https://www.sqlite.org/bindptr.html) and is therefore be safe to use
    /// with any ValueRef (although such a use will result in an Err).
    ///
    /// Requires SQLite 3.38.0.
    pub fn from_value_ref(base: &'list mut ValueRef) -> Result<Self> {
        let _ = base;
        sqlite3_require_version!(3_038_000, unsafe {
            let mut first: *mut ffi::sqlite3_value = ptr::null_mut();
            Error::from_sqlite(ffi::sqlite3_vtab_in_first(base.as_ptr(), &mut first as _))?;
            Ok(Self {
                base,
                pending: Some(ptr::NonNull::new(first)),
            })
        })
    }
}

impl FallibleIteratorMut for ValueList<'_> {
    type Item = ValueRef;
    type Error = Error;

    fn next(&mut self) -> Result<Option<&mut Self::Item>> {
        sqlite3_match_version! {
            3_038_000 => match self.pending.take() {
                Some(Some(x)) => Ok(Some(unsafe { ValueRef::from_ptr(x.as_ptr()) })),
                Some(None) => Ok(None),
                None => {
                    let mut ret: *mut ffi::sqlite3_value = ptr::null_mut();
                    unsafe {
                        Error::from_sqlite(ffi::sqlite3_vtab_in_next(
                            self.base.as_ptr(),
                            &mut ret as _,
                        ))?;
                        if ret.is_null() {
                            Ok(None)
                        } else {
                            Ok(Some(ValueRef::from_ptr(ret)))
                        }
                    }
                }
            },
            _ => unreachable!(),
        }
    }
}
