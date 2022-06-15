use sqlite3_ext::ffi;
use sqlite3_ext::*;

mod vtab;

#[no_mangle]
pub unsafe extern "C" fn sqlite3_crdb_init(
    db: *mut ffi::sqlite3,
    err_msg: *mut *mut std::os::raw::c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> std::os::raw::c_int {
    ffi::init_api_routines(api);
    let conn = Connection::from(db);
    match crdb_init(&conn) {
        Ok(_) => ffi::SQLITE_OK,
        Err(err) => {
            if let Some(ptr) = ffi::sqlite3_str(&err.to_string()) {
                *err_msg = ptr;
            }
            ffi::SQLITE_ERROR
        }
    }
}

fn crdb_init(db: &Connection) -> Result<()> {
    db.create_module("crdb", eponymous_only_module::<vtab::CrdbVTab>(), None)?;
    Ok(())
}
