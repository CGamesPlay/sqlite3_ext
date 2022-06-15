use crdb::bindings::*;
use crdb::types::*;

struct EponymousOnlyVTab {}

impl VTab for EponymousOnlyVTab {
    fn connect() {
        todo!()
    }
    fn best_index() {
        todo!()
    }
    fn open() {
        todo!()
    }
}

#[no_mangle]
pub unsafe extern "C" fn init_eponymous_only_vtab(
    db: *mut ffi::sqlite3,
    err_msg: *mut *mut std::os::raw::c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> std::os::raw::c_int {
    ffi::init_api_routines(api);
    let conn = Connection::from(db);
    match init_eponymous_only_vtab_impl(&conn) {
        Ok(_) => ffi::SQLITE_OK,
        Err(err) => {
            if let Some(ptr) = ffi::sqlite3_str(&err.to_string()) {
                *err_msg = ptr;
            }
            ffi::SQLITE_ERROR
        }
    }
}

fn init_eponymous_only_vtab_impl(db: &Connection) -> Result<()> {
    let vtab = VTabModule::new("eponymous_only_vtab", EponymousOnlyVTab {});
    Ok(())
}

#[test]
fn eponymous_only_vtab() -> Result<()> {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    Ok(())
}
