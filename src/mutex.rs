use crate::{ffi, Connection};
use std::ops::Deref;

impl Connection {
    pub(crate) fn lock(&self) -> SQLiteMutexGuard<'_, Connection> {
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
