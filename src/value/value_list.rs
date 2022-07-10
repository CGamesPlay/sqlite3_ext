use super::ValueRef;
use crate::{ffi, sqlite3_match_version, sqlite3_require_version, types::*};
use std::ptr;

/// Represents a list of values from SQLite. SQLite provides these when a virtual table is
/// processing all values of an IN constraint simultaneously, see
/// [IndexInfoConstraint::set_value_list_wanted](crate::vtab::IndexInfoConstraint::set_value_list_wanted)
/// for more information.
///
/// This struct is not an Iterator by itself. There are two recommended ways to interact with
/// it. The first is a `while let`:
///
/// ```no_run
/// use sqlite3_ext::{ValueRef, ValueList, Result};
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
/// Alternatively, the [mapped](Self::mapped) method turns this struct into an [Iterator]:
///
/// ```no_run
/// use sqlite3_ext::{ValueRef, ValueList, FromValue, Result};
///
/// fn filter_list(list: &mut ValueRef) -> Result<()> {
///     let list: Vec<Option<String>> = ValueList::from_value_ref(list)?
///         .mapped(|x| Ok(x.get_str()?.map(String::from)))
///         .collect::<Result<_>>()?;
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

    /// Retrieve the next value in the ValueList. Note that the returned value is bound to
    /// the lifetime of self, and it is therefore not possible to use the returned value
    /// after a subsequent call to next. See [mapped](Self::mapped) for a more ergonomic
    /// alternative.
    pub fn next(&mut self) -> Result<Option<&mut ValueRef>> {
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

    /// Convert this ValueList into a proper [Iterator]. Iterator proceeds by invoking the
    /// provided function on each ValueRef, which returns a [Result]. See [the struct
    /// summary](Self) for an example.
    pub fn mapped<R, F: FnMut(&mut ValueRef) -> Result<R>>(
        self,
        f: F,
    ) -> MappedValues<'list, R, F> {
        MappedValues { base: self, f }
    }
}

/// An iterator over the mapped values in a [ValueList].
///
/// F is used to transform the borrowed [ValueRefs](ValueRef) into an owned type.
pub struct MappedValues<'list, R, F: FnMut(&mut ValueRef) -> Result<R>> {
    base: ValueList<'list>,
    f: F,
}

impl<'list, R, F: FnMut(&mut ValueRef) -> Result<R>> Iterator for MappedValues<'list, R, F> {
    type Item = Result<R>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.base.next() {
            Err(x) => Some(Err(x)),
            Ok(Some(x)) => Some((self.f)(x)),
            Ok(None) => None,
        }
    }
}
