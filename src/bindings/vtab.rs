use super::super::types::*;
use super::ffi;
use std::ffi::CString;

pub struct VTabModule {
    data: Box<VTabClientData>,
}

struct VTabClientData {
    name: CString,
    module: ffi::sqlite3_module,
}

impl VTabModule {
    pub fn new(name: &str, _vtab: impl VTab) -> Self {
        todo!();
    }

    pub fn register(self, db: &super::Connection) -> Result<()> {
        let data = Box::into_raw(self.data);
        let rc = unsafe {
            ffi::create_module_v2(
                db.db,
                (*data).name.as_ptr(),
                &(*data).module,
                data as _,
                Some(vtab_drop),
            )
        };
        match rc {
            ffi::SQLITE_OK => Ok(()),
            _ => Err(Error::Sqlite(rc)),
        }
    }
}

unsafe extern "C" fn vtab_drop(data: *mut std::os::raw::c_void) {
    let data: Box<VTabClientData> = Box::from_raw(data as _);
    std::mem::drop(data);
}

pub const EPONYMOUS_ONLY_MODULE: ffi::sqlite3_module = ffi::sqlite3_module {
    iVersion: 2,
    xCreate: None,
    xConnect: None,
    xBestIndex: None,
    xDisconnect: None,
    xDestroy: None,
    xOpen: None,
    xClose: None,
    xFilter: None,
    xNext: None,
    xEof: None,
    xColumn: None,
    xRowid: None,
    xUpdate: None,
    xBegin: None,
    xSync: None,
    xCommit: None,
    xRollback: None,
    xFindFunction: None,
    xRename: None,
    xSavepoint: None,
    xRelease: None,
    xRollbackTo: None,
};

/// Eponymous-only virtual table
pub trait VTab {
    fn connect();
    fn best_index();
    fn open();
}
