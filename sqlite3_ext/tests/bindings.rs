use rusqlite::Result;
use sqlite3_ext::*;

#[derive(Debug, PartialEq)]
struct TestData {
    num: i32,
}

struct EponymousOnlyVTab {}

impl VTab for EponymousOnlyVTab {
    type Aux = ();

    fn connect(&self) {
        todo!()
    }
    fn best_index(&self) {
        todo!()
    }
    fn open(&self) {
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

fn init_eponymous_only_vtab_impl(db: &Connection) -> sqlite3_ext::Result<()> {
    db.create_module("tbl", eponymous_only_module::<EponymousOnlyVTab>(), None)?;
    Ok(())
}

#[test]
fn eponymous_only_vtab() -> Result<()> {
    sqlite3_auto_extension(init_eponymous_only_vtab).unwrap();
    let conn = rusqlite::Connection::open_in_memory()?;
    let results: Vec<TestData> = conn
        .prepare("SELECT * FROM tbl")?
        .query_map([], |row| Ok(TestData { num: row.get(0)? }))?
        .into_iter()
        .collect::<Result<_>>()?;
    assert_eq!(
        results,
        vec! {
            TestData{ num: 42 }
        }
    );
    Ok(())
}
