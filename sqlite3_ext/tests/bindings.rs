use sqlite3_ext::{vtab::*, *};

#[derive(Debug, PartialEq)]
struct TestData {
    num: i32,
}

struct EponymousOnlyVTab {}

impl VTab for EponymousOnlyVTab {
    type Aux = ();

    fn connect(
        _db: &mut Connection,
        _aux: Option<&Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        println!("=== xConnect with {:?}", args);
        Ok((
            "CREATE TABLE x ( value INTEGER NOT NULL )".to_owned(),
            EponymousOnlyVTab {},
        ))
    }
    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        println!("=== xBestIndex with {:?}", index_info);
        Ok(())
    }

    fn open(&self) {
        todo!()
    }
}

impl Drop for EponymousOnlyVTab {
    fn drop(&mut self) {
        println!("=== xDisconnect");
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
    ffi::handle_result(init_eponymous_only_vtab_impl(&conn), err_msg)
}

fn init_eponymous_only_vtab_impl(db: &Connection) -> Result<()> {
    db.create_module(
        "eponymous_only_vtab",
        eponymous_only_module::<EponymousOnlyVTab>(),
        None,
    )?;
    println!("SQLite version {}", sqlite3_libversion());
    Ok(())
}

#[test]
fn eponymous_only_vtab() -> rusqlite::Result<()> {
    sqlite3_auto_extension(init_eponymous_only_vtab).unwrap();
    let conn = rusqlite::Connection::open_in_memory()?;
    let results: Vec<TestData> = conn
        .prepare("SELECT * FROM eponymous_only_vtab")?
        .query_map([], |row| Ok(TestData { num: row.get(0)? }))?
        .into_iter()
        .collect::<rusqlite::Result<_>>()?;
    assert_eq!(
        results,
        vec! {
            TestData{ num: 42 }
        }
    );
    Ok(())
}
