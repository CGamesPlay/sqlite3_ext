use super::*;
use std::{
    ffi::c_void,
    mem::transmute,
    ops::Deref,
    os::raw::{c_char, c_int},
};

type CEntry = unsafe extern "C" fn(
    db: *mut ffi::sqlite3,
    err_msg: *mut *mut c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> c_int;

/// Represents an SQLite-compatible extension entry point.
///
/// Because the original Rust function is the [Deref] target for Extension, it can be called
/// from Rust easily.
///
/// # Examples
///
/// ```no_run
/// use sqlite3_ext::*;
///
/// #[sqlite3_ext_main]
/// fn vfs_init(db: &Connection) -> Result<bool> {
///     // Automatically load this extension on future connections.
///     Extension::auto(&per_db_init)?;
///     // Load this extension on this connection.
///     per_db_init(db)?;
///     // Keep extension loaded after this connection ends.
///     Ok(true)
/// }
///
/// #[sqlite3_ext_init]
/// fn per_db_init(db: &Connection) -> Result<bool> {
///     Ok(false)
/// }
/// ```
#[repr(C)]
pub struct Extension {
    c_entry: unsafe extern "C" fn(),
    init: fn(&Connection) -> Result<bool>,
}

impl Extension {
    /// Construct an Extension from parts.
    ///
    /// You generally want to use [sqlite3_ext_init] instead of this function.
    pub const fn new(c_entry: CEntry, init: fn(&Connection) -> Result<bool>) -> Self {
        unsafe {
            Extension {
                c_entry: transmute(c_entry as *mut c_void),
                init,
            }
        }
    }

    /// Register this extension as an automatic extension.
    ///
    /// The provided method will be invoked on all database connections opened in the
    /// future. For more information, consult the SQLite documentation for
    /// `sqlite3_auto_extension`.
    pub fn auto(ext: &'static Self) -> Result<()> {
        unsafe {
            Error::from_sqlite(ffi::sqlite3_auto_extension(Some(ext.c_entry)))?;
        }
        Ok(())
    }

    /// Remove all registered automatic extensions.
    ///
    /// For more information, consule the SQLite documentation for
    /// `sqlite3_reset_auto_extension`.
    pub fn reset_auto() {
        unsafe {
            ffi::sqlite3_reset_auto_extension();
        }
    }

    /// Remove a previously-registered automatic extension.
    ///
    /// For more information, consult the SQLite documentation for
    /// `sqlite3_cancel_auto_extension`.
    pub fn cancel_auto(ext: &'static Self) -> bool {
        unsafe { ffi::sqlite3_cancel_auto_extension(Some(ext.c_entry)) != 0 }
    }
}

impl Deref for Extension {
    type Target = fn(&Connection) -> Result<bool>;

    fn deref(&self) -> &fn(&Connection) -> Result<bool> {
        &self.init
    }
}
