use super::ffi::*;
use super::types::*;

pub const VTAB_NAME: &[u8] = b"crdb\0";

pub const VTAB_MODULE: sqlite3_module = sqlite3_module {
    iVersion: 0,
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

#[repr(C)]
struct CrdbVTab {
    base: sqlite3_vtab,
}

pub unsafe fn create_module(db: *mut sqlite3) -> Result<()> {
    let rc = sqlite3_create_module_v2(
        db,
        VTAB_NAME.as_ptr() as _,
        &VTAB_MODULE,
        std::ptr::null_mut(),
        None,
    );
    match rc {
        SQLITE_OK => Ok(()),
        _ => Err(Error::Sqlite(rc)),
    }
}
