use sqlite3_ext::{function::*, vtab::*, *};

#[derive(Debug, PartialEq)]
struct TestData {
    rowid: i64,
    num: i32,
}

struct EponymousOnlyVTab<'vtab> {
    data: &'vtab Vec<i32>,
}

impl<'vtab> VTab<'vtab> for EponymousOnlyVTab<'vtab> {
    type Aux = Vec<i32>;
    type Cursor = EponymousOnlyCursor<'vtab>;

    fn connect(
        _db: &mut Connection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        println!("=== xConnect with {:?}", args);
        match aux {
            Some(data) => Ok((
                "CREATE TABLE x ( value INTEGER NOT NULL )".to_owned(),
                EponymousOnlyVTab { data },
            )),
            None => Err(Error::Module("no data provided".to_owned())),
        }
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        println!("=== xBestIndex with {:?}", index_info);
        Ok(())
    }

    fn open(&mut self) -> Result<Self::Cursor> {
        println!("=== xOpen");
        Ok(EponymousOnlyCursor {
            iter: self.data.iter(),
            current: None,
        })
    }
}

impl Drop for EponymousOnlyVTab<'_> {
    fn drop(&mut self) {
        println!("=== xDisconnect");
    }
}

struct EponymousOnlyCursor<'vtab> {
    iter: std::slice::Iter<'vtab, i32>,
    current: Option<&'vtab i32>,
}

impl VTabCursor for EponymousOnlyCursor<'_> {
    fn filter(&mut self, index_num: usize, index_str: Option<&str>, args: &[Value]) -> Result<()> {
        println!(
            "=== xFilter with {}, {:?}, {:?}",
            index_num, index_str, args
        );
        self.current = self.iter.next();
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        self.current = self.iter.next();
        Ok(())
    }

    fn eof(&self) -> bool {
        println!("=== xEof");
        match self.current {
            Some(_) => false,
            None => true,
        }
    }

    fn column(&self, context: &mut Context, i: usize) -> Result<()> {
        println!("=== xColumn with {:?}, {}", context, i);
        if let Some(i) = self.current {
            context.set_result(*i);
        }
        Ok(())
    }

    fn rowid(&self) -> Result<i64> {
        println!("=== xRowid");
        self.current
            .ok_or(Error::Sqlite(ffi::SQLITE_MISUSE))
            .map(|x| (x - 1) as _)
    }
}

impl Drop for EponymousOnlyCursor<'_> {
    fn drop(&mut self) {
        println!("=== xClose");
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
        Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
    )?;
    println!("SQLite version {}", sqlite3_libversion());
    Ok(())
}

#[test]
fn eponymous_only_vtab() -> rusqlite::Result<()> {
    sqlite3_auto_extension(init_eponymous_only_vtab).unwrap();
    let conn = rusqlite::Connection::open_in_memory()?;
    let results: Vec<TestData> = conn
        .prepare("SELECT rowid, * FROM eponymous_only_vtab")?
        .query_map([], |row| {
            Ok(TestData {
                rowid: row.get(0)?,
                num: row.get(1)?,
            })
        })?
        .into_iter()
        .collect::<rusqlite::Result<_>>()?;
    assert_eq!(
        results,
        vec! {
            TestData{ rowid: 0,num: 1 },
            TestData{ rowid: 1, num: 2 },
            TestData{ rowid: 2, num: 3 },
            TestData{ rowid: 3, num: 4 },
            TestData{ rowid: 4, num: 5 },
            TestData{ rowid: 5, num: 6 },
            TestData{ rowid: 6, num: 7 },
            TestData{ rowid: 7, num: 8 },
            TestData{ rowid: 8, num: 9 },
            TestData{ rowid: 9, num: 10 },
        }
    );
    Ok(())
}
