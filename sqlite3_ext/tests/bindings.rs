use sqlite3_ext::{function::*, vtab::*, *};
use std::sync::Once;

#[derive(Debug, PartialEq)]
struct TestData {
    rowid: i64,
    num: i32,
}

struct StandardVTab<'vtab> {
    data: &'vtab Vec<i32>,
    rowid_offset: i64,
}

impl<'vtab> VTab<'vtab> for StandardVTab<'vtab> {
    type Aux = Vec<i32>;
    type Cursor = StandardCursor<'vtab>;

    fn connect(
        _db: &mut Connection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        println!("=== xConnect with {:?}", args);
        let rowid_offset = if args.len() > 3 {
            args[3]
                .parse()
                .map_err(|_| Error::Module("cannot parse rowid_offset".to_owned()))?
        } else {
            -1
        };
        match aux {
            Some(data) => Ok((
                "CREATE TABLE x ( value INTEGER NOT NULL )".to_owned(),
                StandardVTab { data, rowid_offset },
            )),
            None => Err(Error::Module("no data provided".to_owned())),
        }
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        println!("=== xBestIndex with {:?}", index_info);
        Ok(())
    }

    fn open(&'vtab mut self) -> Result<Self::Cursor> {
        println!("=== xOpen");
        Ok(StandardCursor {
            vtab: self,
            iter: self.data.iter(),
            current: None,
        })
    }
}

impl<'vtab> CreateVTab<'vtab> for StandardVTab<'vtab> {
    fn create(
        db: &mut Connection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        print!("=== xCreate ");
        Self::connect(db, aux, args)
    }

    fn destroy(&mut self) -> Result<()> {
        println!("=== xDestroy");
        Ok(())
    }
}

impl Drop for StandardVTab<'_> {
    fn drop(&mut self) {
        println!("=== xDisconnect");
    }
}

struct StandardCursor<'vtab> {
    vtab: &'vtab StandardVTab<'vtab>,
    iter: std::slice::Iter<'vtab, i32>,
    current: Option<&'vtab i32>,
}

impl VTabCursor for StandardCursor<'_> {
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
        let rowid_offset = self.vtab.rowid_offset;
        self.current
            .ok_or(Error::Sqlite(ffi::SQLITE_MISUSE))
            .map(|x| *x as i64 + rowid_offset)
    }
}

impl Drop for StandardCursor<'_> {
    fn drop(&mut self) {
        println!("=== xClose");
    }
}

static START: Once = Once::new();

fn setup() {
    START.call_once(|| {
        sqlite3_auto_extension(init_test).unwrap();
    });
}

#[no_mangle]
pub unsafe extern "C" fn init_test(
    db: *mut ffi::sqlite3,
    err_msg: *mut *mut std::os::raw::c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> std::os::raw::c_int {
    ffi::init_api_routines(api);
    ffi::handle_result(init_test_impl(&mut Connection::from(db)), err_msg)
}

fn init_test_impl(db: &mut Connection) -> Result<()> {
    db.create_module(
        "eponymous_only_vtab",
        Module::<StandardVTab>::eponymous_only()?,
        Some(vec![10, 12, 14, 16, 18]),
    )?;
    db.create_module(
        "eponymous_vtab",
        Module::<StandardVTab>::eponymous(),
        Some(vec![20, 22, 24, 26, 28]),
    )?;
    db.create_module(
        "standard_vtab",
        Module::<StandardVTab>::standard(),
        Some(vec![30, 32, 34, 36, 38]),
    )?;
    Ok(())
}

#[test]
fn eponymous_only_vtab() -> rusqlite::Result<()> {
    setup();
    let conn = rusqlite::Connection::open_in_memory()?;
    match conn.execute(
        "CREATE VIRTUAL TABLE tbl USING eponymous_only_vtab(300)",
        [],
    ) {
        Ok(_) => panic!("created eponymous_only_vtab"),
        Err(e) => assert_eq!(format!("{}", e), "no such module: eponymous_only_vtab"),
    }
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
            TestData{ rowid: 9, num: 10 },
            TestData{ rowid: 11, num: 12 },
            TestData{ rowid: 13, num: 14 },
            TestData{ rowid: 15, num: 16 },
            TestData{ rowid: 17, num: 18 },
        }
    );
    Ok(())
}

#[test]
fn eponymous_vtab() -> rusqlite::Result<()> {
    setup();
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING eponymous_vtab(200)", [])?;
    let results: Vec<TestData> = conn
        .prepare("SELECT rowid, * FROM eponymous_vtab")?
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
            TestData{ rowid: 19, num: 20 },
            TestData{ rowid: 21, num: 22 },
            TestData{ rowid: 23, num: 24 },
            TestData{ rowid: 25, num: 26 },
            TestData{ rowid: 27, num: 28 },
        }
    );
    let results: Vec<TestData> = conn
        .prepare("SELECT rowid, * FROM tbl")?
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
            TestData{ rowid: 220, num: 20 },
            TestData{ rowid: 222, num: 22 },
            TestData{ rowid: 224, num: 24 },
            TestData{ rowid: 226, num: 26 },
            TestData{ rowid: 228, num: 28 },
        }
    );
    Ok(())
}

#[test]
fn standard_vtab() -> rusqlite::Result<()> {
    setup();
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING standard_vtab(300)", [])?;
    match conn.prepare("SELECT * FROM standard_vtab") {
        Ok(_) => panic!("standard_vtab accessed eponymously"),
        Err(e) => assert_eq!(format!("{}", e), "no such table: standard_vtab"),
    }
    let results: Vec<TestData> = conn
        .prepare("SELECT rowid, * FROM tbl")?
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
            TestData{ rowid: 330, num: 30 },
            TestData{ rowid: 332, num: 32 },
            TestData{ rowid: 334, num: 34 },
            TestData{ rowid: 336, num: 36 },
            TestData{ rowid: 338, num: 38 },
        }
    );
    conn.execute("DROP TABLE tbl", [])?;
    Ok(())
}
