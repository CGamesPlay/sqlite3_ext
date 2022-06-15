pub use crate::rusqlite::auto_register;
use bindings::{ffi::*, Connection};
use types::*;

pub mod bindings;
mod rusqlite;
pub mod types;
mod vtab;

#[no_mangle]
pub unsafe extern "C" fn sqlite3_crdb_init(
    db: *mut sqlite3,
    err_msg: *mut *mut std::os::raw::c_char,
    api: *mut sqlite3_api_routines,
) -> std::os::raw::c_int {
    init_api_routines(api);
    let conn = Connection::from(db);
    match crdb_init(&conn) {
        Ok(_) => SQLITE_OK,
        Err(err) => {
            if let Some(ptr) = sqlite3_str(&err.to_string()) {
                *err_msg = ptr;
            }
            SQLITE_ERROR
        }
    }
}

fn crdb_init(db: &Connection) -> Result<()> {
    vtab::create_module(db)?;
    println!(
        "Extension loaded! SQLite {}",
        bindings::sqlite3_libversion(),
    );
    Ok(())
}
