//! Provides FallibleIteratorMut and FallibleIterator.
//!
//! The FallibleIterator trait is a re-export from [fallible_iterator].
pub use fallible_iterator::FallibleIterator;

/// Provides a FallibleIterator over mutable references.
///
/// Ordinarily a [FallibleIterator] iterates over owned items, which makes this trait
/// incompatible with it. The [map](Self::map) method allows converting this trait into a
/// FallibleIterator, and it's also possible to use with a `while let` loop:
///
/// ```
/// use sqlite3_ext::FallibleIteratorMut;
///
/// fn dump<I: FallibleIteratorMut>(mut it: I) -> Result<(), I::Error>
/// where
///     I::Item: std::fmt::Debug,
/// {
///     while let Some(x) = it.next()? {
///         println!("{:?}", x);
///     }
///     Ok(())
/// }
/// ```
pub trait FallibleIteratorMut {
    /// The type of item being iterated.
    type Item;
    /// The type of error that can be returned by this iterator.
    type Error;

    /// Works like [FallibleIterator::next], except instead of returning `Self::Item`, it
    /// returns `&mut Self::Item`.
    fn next(&mut self) -> Result<Option<&mut Self::Item>, Self::Error>;

    /// See [Iterator::size_hint].
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }

    /// Convert this iterator into a [FallibleIterator] by applying a function to each
    /// element.
    #[inline]
    fn map<F, B>(&mut self, f: F) -> Map<Self, F>
    where
        Self: Sized,
        F: FnMut(&mut Self::Item) -> Result<B, Self::Error>,
    {
        Map { it: self, f }
    }
}

pub struct Map<'a, I, F> {
    it: &'a mut I,
    f: F,
}

impl<'a, I, F, B> FallibleIterator for Map<'a, I, F>
where
    I: FallibleIteratorMut,
    F: FnMut(&mut I::Item) -> Result<B, I::Error>,
{
    type Item = B;
    type Error = I::Error;

    #[inline]
    fn next(&mut self) -> Result<Option<B>, I::Error> {
        match self.it.next() {
            Ok(Some(v)) => Ok(Some((self.f)(v)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.it.size_hint()
    }
}
