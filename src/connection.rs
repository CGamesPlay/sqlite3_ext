use crate::{ffi, sqlite3_match_version, types::*};
use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    thread::panicking,
};

/// Represents a borrowed connection to an SQLite database.
#[repr(transparent)]
pub struct Connection {
    db: ffi::sqlite3,
}

impl Connection {
    /// Convert an SQLite handle into a reference to Connection.
    ///
    /// # Safety
    ///
    /// The behavior of this method is undefined if the passed pointer is not valid.
    pub unsafe fn from_ptr<'a>(db: *mut ffi::sqlite3) -> &'a mut Connection {
        &mut *(db as *mut Connection)
    }

    /// Get the underlying SQLite handle.
    ///
    /// # Safety
    ///
    /// Using the returned pointer may cause undefined behavior in other, safe code.
    pub unsafe fn as_mut_ptr(&self) -> *mut ffi::sqlite3 {
        &self.db as *const _ as _
    }

    /// Enable or disable the "defensive" flag for the database.
    ///
    /// See
    /// [SQLITE_DBCONFIG_DEFENSIVE](https://www.sqlite.org/c3ref/c_dbconfig_defensive.html#sqlitedbconfigdefensive)
    /// for details.
    ///
    /// Requires SQLite 3.26.0. On earlier versions, this method is a no-op.
    pub fn db_config_defensive(&self, enable: bool) -> Result<()> {
        let _ = enable;
        sqlite3_match_version! {
            3_026_000 => unsafe {
                Error::from_sqlite_desc_unchecked(
                    ffi::sqlite3_db_config()(
                        self.as_mut_ptr(),
                        ffi::SQLITE_DBCONFIG_DEFENSIVE,
                        enable as i32,
                        0 as i32,
                    ),
                    self.as_mut_ptr(),
                )
            },
            _ => Ok(()),
        }
    }
}

/// Represents an owned connection to an SQLite database.
///
/// This struct is an owned version of [Connection]. When this struct is dropped, it will close
/// the underlying connection to SQLite.
pub struct Database {
    db: *mut ffi::sqlite3,
}

impl Database {
    pub fn open_in_memory() -> Result<Database> {
        const FILENAME: &[u8] = b":memory:\0";
        let mut db = MaybeUninit::uninit();
        let rc = Error::from_sqlite(unsafe {
            ffi::sqlite3_open(FILENAME.as_ptr() as _, db.as_mut_ptr())
        });
        match rc {
            Ok(()) => Ok(Database {
                db: unsafe { *db.as_ptr() },
            }),
            Err(e) => {
                if !db.as_ptr().is_null() {
                    // Panic if we can't close the database we failed to open
                    Error::from_sqlite(unsafe { ffi::sqlite3_close(*db.as_ptr()) }).unwrap();
                }
                Err(e)
            }
        }
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let rc = Error::from_sqlite(unsafe { ffi::sqlite3_close(self.db) });
        if let Err(e) = rc {
            if panicking() {
                eprintln!("Error while closing SQLite connection: {:?}", e);
            } else {
                panic!("Error while closing SQLite connection: {:?}", e);
            }
        }
    }
}

impl Deref for Database {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        unsafe { Connection::from_ptr(self.db) }
    }
}

impl DerefMut for Database {
    fn deref_mut(&mut self) -> &mut Connection {
        unsafe { Connection::from_ptr(self.db) }
    }
}
