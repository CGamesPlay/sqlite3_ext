use crate::{ffi, Connection};
use std::ops::Deref;

impl Connection {
    /// Locks the mutex associated with this database connection. If multiple SQLite APIs need to
    /// be used and there is a chance that this Connection may be used from multiple threads, this
    /// method should be used to lock the Connection to the calling thread. The returned mutex
    /// guard will unlock the mutex when it is dropped, and derefs to the original connection.
    ///
    /// This method has no effect if SQLite is not operating in [serialized threading
    /// mode](https://www.sqlite.org/threadsafe.html).
    pub fn lock(&self) -> SQLiteMutexGuard<'_, Connection> {
        let mutex = unsafe { ffi::sqlite3_db_mutex(self.as_mut_ptr()) };
        unsafe { ffi::sqlite3_mutex_enter(mutex) };
        SQLiteMutexGuard { mutex, data: self }
    }
}

pub struct SQLiteMutexGuard<'a, T> {
    mutex: *mut ffi::sqlite3_mutex,
    data: &'a T,
}

impl<T> Drop for SQLiteMutexGuard<'_, T> {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_mutex_leave(self.mutex) }
    }
}

impl<T> Deref for SQLiteMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.data
    }
}
