//! Wrappers for creating virtual tables.

use super::*;
use crate::{ffi, sqlite3_match_version, sqlite3_require_version, Connection};
use sealed::sealed;
use std::{ffi::CString, marker::PhantomData};

union ModuleBytes {
    bytes: [u8; std::mem::size_of::<ffi::sqlite3_module>()],
    module: ffi::sqlite3_module,
}

// We use this empty module hack to avoid specifying all of the fields for the module here. In
// general, we present the most modern API that we can, but use Result types to indicate when a
// feature is not available due to the runtime SQLite version. When statically linking, we
// emulate the same behavior, but we have to be a bit more cautious, since we are using the
// libsqlite3_sys presented API, which might otherwise cause compilation errors.
const EMPTY_MODULE: ffi::sqlite3_module = unsafe {
    ModuleBytes {
        bytes: [0_u8; std::mem::size_of::<ffi::sqlite3_module>()],
    }
    .module
};

#[cfg(modern_sqlite)]
fn set_version(m: &mut ffi::sqlite3_module, val: i32) {
    m.iVersion = std::cmp::max(m.iVersion, val);
}

/// Handle to the module and aux data, so that it can be properly dropped when the module is
/// unloaded.
pub(super) struct Handle<'vtab, T: VTab<'vtab>> {
    pub vtab: ffi::sqlite3_module,
    pub aux: T::Aux,
}

impl<'vtab, T: VTab<'vtab>> Handle<'vtab, T> {
    pub unsafe fn from_ptr<'a>(ptr: *mut c_void) -> &'a Self {
        &*(ptr as *mut Self)
    }
}

/// A virtual table module.
///
/// You generally do not need to use this trait directly, see
/// [sqlite_ext_vtab](::sqlite3_ext_macro::sqlite3_ext_vtab) for details on how to use this.
#[sealed]
pub trait Module<'vtab, T: VTab<'vtab> + 'vtab>
where
    Self: Sized,
{
    #[doc(hidden)]
    fn module(&mut self) -> &mut ffi::sqlite3_module;

    #[doc(hidden)]
    fn with_update(mut self) -> Self
    where
        T: UpdateVTab<'vtab>,
    {
        self.module().xUpdate = Some(stubs::vtab_update::<T>);
        self
    }

    #[doc(hidden)]
    fn with_transactions(mut self) -> Self
    where
        T: TransactionVTab<'vtab>,
    {
        self.with_initial_transaction();
        let mut m = self.module();
        m.xBegin = Some(stubs::vtab_begin::<T>);
        m.xSync = Some(stubs::vtab_sync::<T>);
        m.xCommit = Some(stubs::vtab_commit::<T>);
        sqlite3_match_version! {
            3_007_007 => {
                set_version(m, 2);
                m.xRollback = Some(stubs::vtab_rollback::<T>);
                m.xSavepoint = Some(stubs::vtab_savepoint::<T>);
                m.xRelease = Some(stubs::vtab_release::<T>);
                m.xRollbackTo = Some(stubs::vtab_rollback_to::<T>);
            }
            _ => (),
        }
        self
    }

    #[doc(hidden)]
    fn with_initial_transaction(&mut self)
    where
        T: TransactionVTab<'vtab>;

    #[doc(hidden)]
    fn with_find_function(mut self) -> Self
    where
        T: FindFunctionVTab<'vtab>,
    {
        self.module().xFindFunction = Some(stubs::vtab_find_function::<T>);
        self
    }

    #[doc(hidden)]
    fn with_rename(mut self) -> Self
    where
        T: RenameVTab<'vtab>,
    {
        self.module().xRename = Some(stubs::vtab_rename::<T>);
        self
    }
}

macro_rules! module_base {
    ($(#[$attr:meta])* $name:ident < $ty:ident > { $($extra:tt)* }) => {
        $(#[$attr])*
        pub struct $name<'vtab, T: VTab<'vtab>> {
            base: ffi::sqlite3_module,
            phantom: PhantomData<&'vtab T>,
        }

        #[sealed]
        impl<'vtab, T: $ty<'vtab>> Module<'vtab, T> for $name<'vtab, T> {
            fn module(&mut self) -> &mut ffi::sqlite3_module {
                &mut self.base
            }

            $($extra)*
        }
    };
}

module_base!(
    /// Declare a virtual table.
    ///
    /// See [sqlite_ext_vtab](::sqlite3_ext_macro::sqlite3_ext_vtab) for details on how to
    /// use this.
    StandardModule<CreateVTab> {
    fn with_initial_transaction(&mut self)
    where
        T: TransactionVTab<'vtab>,
    {
        // This is a standard table, so we need to override this.
        self.base.xCreate = Some(stubs::vtab_create_transaction::<T>);
    }
});

module_base!(
    /// Describes an eponymous virtual table.
    ///
    /// For this module, the virtual table is available ambiently in the database, but
    /// CREATE VIRTUAL TABLE can also be used to instantiate the table with alternative
    /// parameters.
    ///
    /// See [sqlite_ext_vtab](::sqlite3_ext_macro::sqlite3_ext_vtab) for details on how to
    /// use this.
    EponymousModule<VTab> {
    fn with_initial_transaction(&mut self)
    where
        T: TransactionVTab<'vtab>,
    {
        // This is an eponymous table, so we need to override both methods together.
        self.base.xConnect = Some(stubs::vtab_connect_transaction::<T>);
        self.base.xCreate = Some(stubs::vtab_connect_transaction::<T>);
    }
});

module_base!(
    /// Declare an eponymous-only virtual table.
    ///
    /// For this virtual table, CREATE VIRTUAL TABLE is forbidden, but the table is
    /// ambiently available under the module name.
    ///
    /// This feature requires SQLite 3.9.0 or above. Older versions of SQLite do not
    /// support eponymous virtual tables, meaning they require at least one CREATE VIRTUAL
    /// TABLE statement to be used. If supporting these versions of SQLite is desired, you
    /// can use [StandardModule] and return an error if there is an attempt to instantiate
    /// the virtual table more than once.
    ///
    /// See [sqlite_ext_vtab](::sqlite3_ext_macro::sqlite3_ext_vtab) for details on how to
    /// use this.
    EponymousOnlyModule<VTab> {
    fn with_initial_transaction(&mut self)
    where
        T: TransactionVTab<'vtab>,
    {
        // CREATE VIRTUAL TABLE will never be called on this table.
    }
});

impl<'vtab, T: CreateVTab<'vtab>> StandardModule<'vtab, T> {
    #[doc(hidden)]
    pub fn new() -> Self {
        #[cfg_attr(not(modern_sqlite), allow(unused_mut))]
        let mut ret = StandardModule {
            base: ffi::sqlite3_module {
                xCreate: Some(stubs::vtab_create::<T>),
                xConnect: Some(stubs::vtab_connect::<T>),
                xBestIndex: Some(stubs::vtab_best_index::<T>),
                xDisconnect: Some(stubs::vtab_disconnect::<T>),
                xDestroy: Some(stubs::vtab_destroy::<T>),
                xOpen: Some(stubs::vtab_open::<T>),
                xClose: Some(stubs::vtab_close::<T>),
                xFilter: Some(stubs::vtab_filter::<T>),
                xNext: Some(stubs::vtab_next::<T>),
                xEof: Some(stubs::vtab_eof::<T>),
                xColumn: Some(stubs::vtab_column::<T>),
                xRowid: Some(stubs::vtab_rowid::<T>),
                ..EMPTY_MODULE
            },
            phantom: PhantomData,
        };
        sqlite3_match_version! {
            3_026_000 => {
                if T::SHADOW_NAMES.len() > 0 {
                    set_version(&mut ret.base, 3);
                    ret.base.xShadowName = Some(stubs::vtab_shadow_name::<T>);
                }
            }
            _ => (),
        }
        ret
    }
}

impl<'vtab, T: VTab<'vtab>> EponymousModule<'vtab, T> {
    #[doc(hidden)]
    pub fn new() -> Self {
        EponymousModule {
            base: ffi::sqlite3_module {
                xCreate: Some(stubs::vtab_connect::<T>),
                xConnect: Some(stubs::vtab_connect::<T>),
                xBestIndex: Some(stubs::vtab_best_index::<T>),
                xDisconnect: Some(stubs::vtab_disconnect::<T>),
                xDestroy: Some(stubs::vtab_disconnect::<T>),
                xOpen: Some(stubs::vtab_open::<T>),
                xClose: Some(stubs::vtab_close::<T>),
                xFilter: Some(stubs::vtab_filter::<T>),
                xNext: Some(stubs::vtab_next::<T>),
                xEof: Some(stubs::vtab_eof::<T>),
                xColumn: Some(stubs::vtab_column::<T>),
                xRowid: Some(stubs::vtab_rowid::<T>),
                ..EMPTY_MODULE
            },
            phantom: PhantomData,
        }
    }
}

impl<'vtab, T: VTab<'vtab>> EponymousOnlyModule<'vtab, T> {
    #[doc(hidden)]
    pub fn new() -> Result<Self> {
        sqlite3_require_version!(
            3_009_000,
            Ok(EponymousOnlyModule {
                base: ffi::sqlite3_module {
                    xConnect: Some(stubs::vtab_connect::<T>),
                    xBestIndex: Some(stubs::vtab_best_index::<T>),
                    xDisconnect: Some(stubs::vtab_disconnect::<T>),
                    xDestroy: Some(stubs::vtab_disconnect::<T>),
                    xOpen: Some(stubs::vtab_open::<T>),
                    xClose: Some(stubs::vtab_close::<T>),
                    xFilter: Some(stubs::vtab_filter::<T>),
                    xNext: Some(stubs::vtab_next::<T>),
                    xEof: Some(stubs::vtab_eof::<T>),
                    xColumn: Some(stubs::vtab_column::<T>),
                    xRowid: Some(stubs::vtab_rowid::<T>),
                    ..EMPTY_MODULE
                },
                phantom: PhantomData,
            })
        )
    }
}

impl Connection {
    /// Register the provided virtual table module with this connection.
    pub fn create_module<'db: 'vtab, 'vtab, T: VTab<'vtab> + 'vtab, M: Module<'vtab, T> + 'vtab>(
        &'db self,
        name: &str,
        mut vtab: M,
        aux: T::Aux,
    ) -> Result<()>
    where
        T::Aux: 'db,
    {
        let name = CString::new(name).unwrap();
        let vtab = vtab.module().clone();
        let handle = Box::new(Handle::<'vtab, T> { vtab, aux });
        let guard = self.lock();
        Error::from_sqlite_desc(
            unsafe {
                ffi::sqlite3_create_module_v2(
                    self.as_mut_ptr(),
                    name.as_ptr() as _,
                    &handle.vtab,
                    Box::into_raw(handle) as _,
                    Some(ffi::drop_boxed::<Handle<T>>),
                )
            },
            guard,
        )
    }
}
