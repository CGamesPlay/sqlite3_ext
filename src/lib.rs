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
    ffi::handle_result(crdb_init(&conn), err_msg)
}

fn crdb_init(db: &Connection) -> Result<()> {
    db.create_module("crdb", eponymous_only_module::<vtab::CrdbVTab>(), None)?;
    Ok(())
}
