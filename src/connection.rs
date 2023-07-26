#[cfg(modern_sqlite)]
use crate::mutex::SQLiteMutexGuard;
use crate::{ffi, sqlite3_match_version, sqlite3_require_version, types::*};
use bitflags::bitflags;
#[cfg(modern_sqlite)]
use std::ptr::{null, NonNull};
use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    os::raw::c_int,
    path::Path,
    ptr::null_mut,
    thread::panicking,
};

bitflags! {
    /// These are the flags that can be passed to [Database::open_with_flags] and variants.
    #[repr(transparent)]
    pub struct OpenFlags: c_int {
        /// The database is opened in read-only mode. If the database does not already exist, an error is returned.
        const READONLY = ffi::SQLITE_OPEN_READONLY;
        /// The database is opened for reading and writing if possible, or reading only
        /// if the file is write protected by the operating system. In either case the
        /// database must already exist, otherwise an error is returned (see
        /// [Self::CREATE]).
        const READWRITE = ffi::SQLITE_OPEN_READWRITE;
        /// Create a new, empty database file when opening if it does not already
        /// exist. This only applies to [Self::READWRITE].
        const CREATE = ffi::SQLITE_OPEN_CREATE;
        /// The database will be opened as an in-memory database. The database is named
        /// by the "filename" argument for the purposes of cache-sharing, if shared
        /// cache mode is enabled, but the "filename" is otherwise ignored.
        const MEMORY = ffi::SQLITE_OPEN_MEMORY;
        /// The new database connection will use the "multi-thread" threading mode.
        /// This means that separate threads are allowed to use SQLite at the same
        /// time, as long as each thread is using a different database connection.
        ///
        /// # Safety
        ///
        /// When using this flag, you assume responsibility for verifying that the
        /// database connection will only be accessed from a single thread.
        const UNSAFE_NOMUTEX = ffi::SQLITE_OPEN_NOMUTEX;
        /// The new database connection will use the "serialized" threading mode. This
        /// means the multiple threads can safely attempt to use the same database
        /// connection at the same time. (Mutexes will block any actual concurrency,
        /// but in this mode there is no harm in trying.)
        const FULLMUTEX = ffi::SQLITE_OPEN_FULLMUTEX;
        /// The database is opened shared cache enabled, overriding the default shared
        /// cache setting provided by [ffi::sqlite3_enable_shared_cache].
        const SHAREDCACHE = ffi::SQLITE_OPEN_SHAREDCACHE;
        /// The database is opened shared cache disabled, overriding the default shared
        /// cache setting provided by [ffi::sqlite3_enable_shared_cache].
        const PRIVATECACHE = ffi::SQLITE_OPEN_PRIVATECACHE;
        /// The database connection comes up in "extended result code mode". In other
        /// words, the database behaves has if
        /// [ffi::sqlite3_extended_result_codes](db,1) where called on the database
        /// connection as soon as the connection is created. In addition to setting the
        /// extended result code mode, this flag also causes the corresponding
        /// [Database] open method to return an extended result code.
        const EXRESCODE = ffi::SQLITE_OPEN_EXRESCODE;
        /// The database filename is not allowed to be a symbolic link.
        const NOFOLLOW = ffi::SQLITE_OPEN_NOFOLLOW;

        /// This is the set of flags used when calling open methods that do not accept
        /// flags.
        const DEFAULT = ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE;
    }
}

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
                    Ok(cstr) => eprintln!("=> {cstr}"),
                    Err(e) => eprintln!("{stmt:?}: invalid SQL: {e}"),
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

#[cfg(unix)]
fn path_to_cstring(path: &Path) -> CString {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).unwrap()
}

/// Represents an owned connection to an SQLite database.
///
/// This struct is an owned version of [Connection]. When this struct is dropped, it will close
/// the underlying connection to SQLite.
pub struct Database {
    db: *mut ffi::sqlite3,
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Database> {
        let filename = path_to_cstring(path.as_ref());
        Database::_open(filename.as_c_str(), OpenFlags::DEFAULT)
    }

    pub fn open_with_flags<P: AsRef<Path>>(path: P, flags: OpenFlags) -> Result<Database> {
        let filename = path_to_cstring(path.as_ref());
        Database::_open(filename.as_c_str(), flags)
    }

    fn _open(filename: &CStr, flags: OpenFlags) -> Result<Database> {
        let mut db = MaybeUninit::uninit();
        let rc = Error::from_sqlite(unsafe {
            ffi::sqlite3_open_v2(
                filename.as_ptr() as _,
                db.as_mut_ptr(),
                flags.bits,
                null_mut(),
            )
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

    /// Gracefully close the database. This automatically happens when the Database is
    /// dropped, but a failure in drop will result in a panic, while this method provides a
    /// path for graceful error handling.
    pub fn close(mut self) -> std::result::Result<(), (Error, Database)> {
        match self._close() {
            Ok(()) => Ok(()),
            Err(e) => Err((e, self)),
        }
    }

    fn _close(&mut self) -> Result<()> {
        Error::from_sqlite(unsafe { ffi::sqlite3_close(self.db) })?;
        self.db = null_mut();
        Ok(())
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if let Err(e) = self._close() {
            if panicking() {
                eprintln!("Error while closing SQLite connection: {e:?}");
            } else {
                panic!("Error while closing SQLite connection: {e:?}");
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
