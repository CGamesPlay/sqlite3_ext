//! A named stack reference.
//!
//! This module exposes [StackRef], a reference to a value stored higher up in the stack.
//!
//! If mutability is required, StackRef can be combined with [Cell](std::cell::Cell) or
//! [RefCell](std::cell::RefCell).
//!
//! # Example
//!
//! ```
//! use sqlite3_ext::{stack_ref, stack_ref::StackRef};
//!
//! // Declare the variable that we want to share.
//! stack_ref!(static VAL: &u32);
//!
//! fn produce() {
//!     let myvar = 42;
//!     // Set the value of the shared variable for the duration of the closure.
//!     VAL.with_value(&myvar, consume);
//! }
//!
//! fn consume() {
//!     // Consumers see the value as &'static u32
//!     assert_eq!(*VAL, 42)
//! }
//! # produce();
//! ```

use std::{
    cell::Cell,
    marker::PhantomData,
    ops::Deref,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
    ptr,
    thread::LocalKey,
};

/// A named stack reference.
///
/// The object is initialized using the [stack_ref!] macro.
///
/// See the [module-level documentation](self) for more details.
pub struct StackRef<T: ?Sized> {
    storage: &'static LocalKey<Cell<usize>>,
    phantom: PhantomData<T>,
}

impl<T: ?Sized> StackRef<T> {
    #[doc(hidden)]
    pub const fn new(storage: &'static LocalKey<Cell<usize>>) -> Self {
        Self {
            storage,
            phantom: PhantomData,
        }
    }

    /// Invoke the function while this StackRef references a particular value.
    ///
    /// It's safe to call this method recursively. The value of the StackRef is always the
    /// value provided furthest down in the stack.
    ///
    /// # Panics
    ///
    /// If this method is used within the destructor for a thread-local variable, then this
    /// function **may** panic. See [std::thread::LocalKey] for more details.
    pub fn with_value<F: FnOnce() -> R, R>(&self, val: &T, f: F) -> R {
        let addr_of_val = ptr::addr_of!(*val) as *const () as usize;
        let prev_value = self.storage.with(|cell| cell.replace(addr_of_val));
        // Safety - this is safe because we use resume_unwind if this method panics.
        let ret = catch_unwind(AssertUnwindSafe(f));
        self.storage.with(|cell| cell.set(prev_value));
        match ret {
            Ok(x) => x,
            Err(e) => resume_unwind(e),
        }
    }
}

impl<T: 'static> Deref for StackRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let addr_of_val = self.storage.with(Cell::get);
        assert!(addr_of_val != 0, "StackRef has no provider");
        // Safety - this is safe because an existing ref to this address is already
        // held above us on the stack.
        unsafe { ptr::read((&addr_of_val) as *const usize as *const &T) }
    }
}

unsafe impl<T: ?Sized> Sync for StackRef<T> {}

/// Declare one or more [StackRefs](StackRef).
///
/// See the [module-level documentation](self) for more details.
///
/// # Syntax
///
/// ```
/// # use sqlite3_ext::stack_ref;
/// use std::cell::Cell;
///
/// // Accepts a single declaration:
/// stack_ref!(static VAL: &u32);
///
/// // Attributes and visibility may be specified:
/// #[allow(unused)]
/// stack_ref!(pub static PUBLIC_REF: &u32);
///
/// // Accepts multiple declarations:
/// stack_ref! {
///     static VAR_ONE: &String;
///     #[allow(unused)]
///     pub static VAR_TWO: &Cell<usize>;
/// }
///
/// // Allows double references:
/// stack_ref!(static STR: &&str);
/// ```
#[macro_export]
macro_rules! stack_ref {
    // empty (base case for the recursion)
    () => {};

    // process multiple declarations
    ($(#[$attr:meta])* $vis:vis static $name:ident: &$t:ty; $($rest:tt)*) => (
        $crate::stack_ref!($(#[$attr])* $vis static $name: &$t);
        $crate::stack_ref!($($rest)*);
    );

    // multiple declaration with &&
    ($(#[$attr:meta])* $vis:vis static $name:ident: &&$t:ty; $($rest:tt)*) => (
        $crate::stack_ref!($(#[$attr])* $vis static $name: & &$t);
        $crate::stack_ref!($($rest)*);
    );

    // handle && for double references
    ($(#[$attr:meta])* $vis:vis static $name:ident: &&$t:ty) => (
        $crate::stack_ref!($(#[$attr])* $vis static $name: & &$t);
    );

    // handle a single declaration
    ($(#[$attr:meta])* $vis:vis static $name:ident: &$t:ty) => (
        ::paste::paste!{
            thread_local!(static [<$name _STORAGE>]: ::std::cell::Cell<usize> = ::std::cell::Cell::new(0));
            $(#[$attr])*
            $vis static $name: $crate::stack_ref::StackRef<$t> = $crate::stack_ref::StackRef::new(&[<$name _STORAGE>]);
        }
    );
}

#[cfg(test)]
mod test {
    stack_ref! {
        static VAL: &u32;
        static UNSIZED: &&str;
        // This one is only used to verify that Sync + Send are not required
        #[allow(unused)]
        static RC: &*mut u32;
    }

    #[test]
    fn basic() {
        fn consume() {
            assert_eq!(*VAL, 42)
        }

        fn produce() {
            let myvar = 42;
            VAL.with_value(&myvar, consume);
        }

        produce();
    }

    #[test]
    fn recursive() {
        fn consume() {
            assert_eq!(*VAL, 84)
        }

        fn modify() {
            let modified = 84;
            VAL.with_value(&modified, consume);
            assert_eq!(*VAL, 42)
        }

        fn produce() {
            let myvar = 42;
            VAL.with_value(&myvar, modify);
        }

        produce();
    }

    #[test]
    fn test_unsized() {
        fn consume() {
            assert_eq!(*UNSIZED, "input string");
        }

        fn produce() {
            let val = "input string";
            UNSIZED.with_value(&val, consume);
        }

        produce();
    }
}
