use std::{
    alloc::{alloc, dealloc, realloc, Layout},
    ffi::c_void,
    mem::{align_of, forget, size_of},
    ptr::{copy_nonoverlapping, read_unaligned, write_unaligned, NonNull},
    slice,
};

const SIZEI: isize = size_of::<usize>() as _;
const SIZEU: usize = size_of::<usize>();

const _: () = {
    assert!(align_of::<u8>() == 1);
};

fn blob_layout(len: usize) -> Layout {
    // Safe because align is 1.
    unsafe { Layout::from_size_align_unchecked(len + SIZEU, 1) }
}

/// Represents an owned BLOB object.
///
/// This container allows BLOB data to be passed to SQLite without copying.
#[repr(transparent)]
pub struct Blob {
    data: NonNull<u8>,
}

impl Blob {
    fn alloc(len: usize) -> Blob {
        let data = unsafe { NonNull::new_unchecked(alloc(blob_layout(len))) };
        let mut ret = Blob { data };
        ret.set_len(len);
        ret
    }

    fn realloc(&mut self, new_len: usize) {
        let layout = blob_layout(self.len());
        self.set_len(new_len);
        self.data = unsafe { NonNull::new_unchecked(realloc(self.data.as_ptr(), layout, new_len)) };
    }

    /// Shorten the BLOB, keeping the first len elements and dropping the rest.
    ///
    /// If len is greater than the BLOB's current length, this has no effect.
    pub fn truncate(&mut self, len: usize) {
        if len < self.len() {
            self.realloc(len);
        }
    }

    /// Consumes the BLOB, returning a pointer to the data.
    ///
    /// After calling this function, the caller is responsible for freeing the memory
    /// previously managed by Blob. The easiest way to do this is by passing
    /// [ffi::drop_blob](crate::ffi::drop_blob) to SQLite when this value is consumed.
    pub fn into_raw(self) -> *mut c_void {
        let ret = unsafe { self.data.as_ptr().offset(SIZEI).cast() };
        forget(self);
        ret
    }

    /// Construct a BLOB from a raw pointer.
    ///
    /// After calling this function, the raw pointer is owned by the resulting Blob.
    ///
    /// # Safety
    ///
    /// It is undefined behavior to call this method on anything other than a pointer that
    /// was returned by [Blob::into_raw].
    pub unsafe fn from_raw(ptr: *mut c_void) -> Blob {
        Blob {
            data: NonNull::new_unchecked(ptr.cast::<u8>().offset(-SIZEI)),
        }
    }

    fn set_len(&mut self, len: usize) {
        unsafe { write_unaligned(self.data.cast().as_ptr(), len) };
    }

    /// Return the length of the BLOB.
    pub fn len(&self) -> usize {
        unsafe { read_unaligned(self.data.cast::<usize>().as_ptr()) }
    }

    /// Get the underlying BLOB data.
    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.data.as_ptr().offset(SIZEI), self.len()) }
    }

    /// Mutably get the underlying BLOB data.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.data.as_ptr().offset(SIZEI), self.len()) }
    }
}

impl Clone for Blob {
    fn clone(&self) -> Self {
        let mut ret = Blob::alloc(self.len());
        ret.as_mut_slice().copy_from_slice(self.as_slice());
        ret
    }
}

impl PartialEq for Blob {
    fn eq(&self, other: &Blob) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Drop for Blob {
    fn drop(&mut self) {
        unsafe { dealloc(self.data.as_ptr(), blob_layout(self.len())) }
    }
}

impl From<&[u8]> for Blob {
    fn from(val: &[u8]) -> Self {
        let mut ret = Self::alloc(val.len());
        ret.as_mut_slice()[..val.len()].copy_from_slice(val);
        ret
    }
}

impl<const N: usize> From<[u8; N]> for Blob {
    fn from(val: [u8; N]) -> Self {
        Self::from(&val[..])
    }
}

impl<const N: usize> From<&[u8; N]> for Blob {
    fn from(val: &[u8; N]) -> Self {
        let ret = Self::alloc(N);
        unsafe { copy_nonoverlapping(val.as_ptr(), ret.data.as_ptr().offset(SIZEI), N) };
        ret
    }
}

impl std::fmt::Debug for Blob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_tuple("Blob").field(&self.as_slice()).finish()
    }
}

#[cfg(test)]
mod test {
    use super::Blob;

    #[test]
    fn debug() {
        let blob = Blob::from([1, 2, 3, 4]);
        assert_eq!(format!("{blob:?}"), "Blob([1, 2, 3, 4])");
    }

    #[test]
    fn truncate() {
        let mut blob = Blob::from([1, 2, 3, 4]);
        assert_eq!(blob.as_slice(), [1, 2, 3, 4]);
        blob.truncate(2);
        assert_eq!(blob.as_slice(), [1, 2]);
    }

    #[test]
    fn into_raw() {
        let ptr;
        {
            let blob = Blob::from([1, 2, 3, 4]);
            assert_eq!(blob.as_slice(), [1, 2, 3, 4]);
            ptr = blob.into_raw();
        }
        let blob = unsafe { Blob::from_raw(ptr) };
        assert_eq!(blob.as_slice(), [1, 2, 3, 4]);
    }
}
