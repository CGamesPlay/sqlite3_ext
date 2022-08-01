#[cfg(modern_sqlite)]
use crate::mutex::SQLiteMutexGuard;
use crate::{ffi, sqlite3_match_version, sqlite3_require_version, types::*};
use std::{
    ffi::CStr,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    thread::panicking,
};
#[cfg(modern_sqlite)]
use std::{
    ffi::CString,
    os::raw::c_int,
    ptr::{null, NonNull},
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

    /// Load the extension at the given path, optionally providing a specific entry point.
    ///
    /// # Safety
    ///
    /// Loading libraries can cause undefined behavior in safe code. It is the caller's
    /// responsibility to ensure that the extension is compatible with sqlite3_ext. In
    /// particular, the caller must verify that the extension being loaded (and the entry
    /// point being invoked) is actually an SQLite extension (created with sqlite3_ext or
    /// otherwise). Never invoke this method with untrusted user-specified data.
    ///
    /// If extension loading is not enabled when this method is called and SQLite is at
    /// least version 3.13.0, then extension loading will temporarily be enabled before
    /// loading the extension, and disabled afterwards. On older versions of SQLite,
    /// extension loading must be manually enabled using unsafe ffi functions before this
    /// method can be used, see
    /// [sqlite3_enable_load_extension](https://www.sqlite.org/c3ref/enable_load_extension.html)
    /// for details.
    ///
    /// Requires SQLite 3.8.7.
    pub fn load_extension(&self, path: &str, entry: Option<&str>) -> Result<()> {
        let _ = (path, entry);
        sqlite3_require_version!(3_008_007, {
            let guard = self.lock();
            LoadExtensionGuard::new(&guard)?;
            unsafe {
                let mut err: MaybeUninit<*mut i8> = MaybeUninit::uninit();
                let path = CString::new(path)?;
                let entry = match entry {
                    Some(s) => Some(CString::new(s)?),
                    None => None,
                };
                let rc = ffi::sqlite3_load_extension(
                    guard.as_mut_ptr(),
                    path.as_ptr(),
                    entry.map_or_else(|| null(), |s| s.as_ptr()),
                    err.as_mut_ptr(),
                );
                if rc != ffi::SQLITE_OK {
                    let err = NonNull::new(err.assume_init()).and_then(|err| {
                        let ret = CStr::from_ptr(err.as_ptr()).to_str().ok().map(String::from);
                        ffi::sqlite3_free(err.as_ptr() as _);
                        ret
                    });
                    Err(Error::Sqlite(rc, err))
                } else {
                    Ok(())
                }
            }
        })
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

    /// Prints the text of all currently prepared statements to stderr. Intended for
    /// debugging.
    pub fn dump_prepared_statements(&self) {
        unsafe {
            let mut stmt = ffi::sqlite3_next_stmt(self.as_mut_ptr(), std::ptr::null_mut());
            while !stmt.is_null() {
                let cstr = CStr::from_ptr(ffi::sqlite3_sql(stmt)).to_str();
                match cstr {
                    Ok(cstr) => eprintln!("{}", cstr),
                    Err(e) => eprintln!("{:?}: invalid SQL: {}", stmt, e),
                }
                stmt = ffi::sqlite3_next_stmt(self.as_mut_ptr(), stmt);
            }
        }
    }
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
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

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
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

#[cfg(modern_sqlite)]
struct LoadExtensionGuard<'a> {
    db: &'a SQLiteMutexGuard<'a, Connection>,
    was_enabled: bool,
}

#[cfg(modern_sqlite)]
impl<'a> LoadExtensionGuard<'a> {
    pub fn new(db: &'a SQLiteMutexGuard<'a, Connection>) -> Result<Self> {
        unsafe {
            let was_enabled = sqlite3_match_version! {
                3_013_000 => {
                    let mut was_enabled: MaybeUninit<c_int> = MaybeUninit::uninit();
                    Error::from_sqlite(ffi::sqlite3_db_config()(
                        db.as_mut_ptr(),
                        ffi::SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION,
                        -1,
                        was_enabled.as_mut_ptr(),
                    ))?;
                    Error::from_sqlite(ffi::sqlite3_db_config()(
                        db.as_mut_ptr(),
                        ffi::SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION,
                        1,
                        0,
                    ))?;
                    was_enabled.assume_init() != 0
                }
                _ => true,
            };
            Ok(Self { db, was_enabled })
        }
    }
}

#[cfg(modern_sqlite)]
impl Drop for LoadExtensionGuard<'_> {
    fn drop(&mut self) {
        if !self.was_enabled {
            sqlite3_match_version! {
                3_013_000 => unsafe {
                    Error::from_sqlite(ffi::sqlite3_db_config()(
                        self.db.as_mut_ptr(),
                        ffi::SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION,
                        0,
                        0,
                    ))
                    .unwrap();
                },
                _ => (),
            }
        }
    }
}
