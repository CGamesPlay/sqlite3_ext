use helpers::*;
use sqlite3_ext::{function::*, vtab::*, *};

mod helpers;

#[derive(Debug, PartialEq)]
struct TestData {
    rowid: i64,
    num: i32,
}

struct StandardVTab {
    lifecycle: VTabLifecycle,
    data: Vec<i32>,
    rowid_offset: i64,
}

impl<'vtab> StandardVTab {
    fn connect_create(
        _db: &mut VTabConnection,
        aux: Option<&'vtab Vec<i32>>,
        args: &[&str],
    ) -> Result<(String, Self)> {
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
                StandardVTab {
                    lifecycle: VTabLifecycle::default(),
                    data: data.clone(),
                    rowid_offset,
                },
            )),
            None => Err(Error::Module("no data provided".to_owned())),
        }
    }
}

impl<'vtab> VTab<'vtab> for StandardVTab {
    type Aux = Vec<i32>;
    type Cursor = StandardCursor<'vtab>;

    fn connect(
        db: &mut VTabConnection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        let (sql, mut vtab) = Self::connect_create(db, aux, args)?;
        vtab.lifecycle.xConnect(aux, args);
        Ok((sql, vtab))
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        self.lifecycle.xBestIndex(index_info);
        Ok(())
    }

    fn open(&'vtab mut self) -> Result<Self::Cursor> {
        let lifecycle = self.lifecycle.xOpen();
        Ok(StandardCursor {
            lifecycle,
            vtab: self,
            iter: self.data.iter(),
            current: None,
        })
    }
}

impl<'vtab> CreateVTab<'vtab> for StandardVTab {
    fn create(
        db: &mut VTabConnection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        let (sql, mut vtab) = Self::connect_create(db, aux, args)?;
        vtab.lifecycle.xCreate(aux, args);
        Ok((sql, vtab))
    }

    fn destroy(&mut self) -> Result<()> {
        self.lifecycle.xDestroy();
        Ok(())
    }
}

impl<'vtab> RenameVTab<'vtab> for StandardVTab {
    fn rename(&mut self, name: &str) -> Result<()> {
        self.lifecycle.xRename(name);
        Ok(())
    }
}

struct StandardCursor<'vtab> {
    lifecycle: CursorLifecycle<'vtab>,
    vtab: &'vtab StandardVTab,
    iter: std::slice::Iter<'vtab, i32>,
    current: Option<&'vtab i32>,
}

impl VTabCursor for StandardCursor<'_> {
    fn filter(&mut self, index_num: usize, index_str: Option<&str>, args: &[&Value]) -> Result<()> {
        self.lifecycle.xFilter(index_num, index_str, args);
        self.current = self.iter.next();
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        self.lifecycle.xNext();
        self.current = self.iter.next();
        Ok(())
    }

    fn eof(&self) -> bool {
        self.lifecycle.xEof();
        match self.current {
            Some(_) => false,
            None => true,
        }
    }

    fn column(&self, context: &mut Context, i: usize) -> Result<()> {
        self.lifecycle.xColumn(context, i);
        if let Some(i) = self.current {
            context.set_result(*i);
        }
        Ok(())
    }

    fn rowid(&self) -> Result<i64> {
        self.lifecycle.xRowid();
        let rowid_offset = self.vtab.rowid_offset;
        self.current
            .ok_or(Error::Sqlite(ffi::SQLITE_MISUSE))
            .map(|x| *x as i64 + rowid_offset)
    }
}

fn check_table(
    conn: &rusqlite::Connection,
    table: &str,
    expected: &Vec<TestData>,
) -> rusqlite::Result<()> {
    let results: Vec<TestData> = conn
        .prepare(&format!("SELECT rowid, * FROM {}", table))?
        .query_map([], |row| {
            Ok(TestData {
                rowid: row.get(0)?,
                num: row.get(1)?,
            })
        })?
        .into_iter()
        .collect::<rusqlite::Result<_>>()?;
    assert_eq!(results, *expected);
    Ok(())
}

#[test]
#[cfg(any(not(feature = "static"), feature = "static_modern"))]
fn eponymous_only_vtab() -> rusqlite::Result<()> {
    let conn = setup()?;
    Connection::from_rusqlite(&conn).create_module(
        "eponymous_only_vtab",
        Module::<StandardVTab>::eponymous_only()?.with_rename(),
        Some(vec![10, 12, 14, 16, 18]),
    )?;
    match conn.execute(
        "CREATE VIRTUAL TABLE tbl USING eponymous_only_vtab(300)",
        [],
    ) {
        Ok(_) => panic!("created eponymous_only_vtab"),
        Err(e) => assert_eq!(format!("{}", e), "no such module: eponymous_only_vtab"),
    }
    check_table(
        &conn,
        "eponymous_only_vtab",
        &vec![
            TestData { rowid: 9, num: 10 },
            TestData { rowid: 11, num: 12 },
            TestData { rowid: 13, num: 14 },
            TestData { rowid: 15, num: 16 },
            TestData { rowid: 17, num: 18 },
        ],
    )?;
    Ok(())
}

#[test]
fn eponymous_vtab() -> rusqlite::Result<()> {
    let conn = setup()?;
    Connection::from_rusqlite(&conn).create_module(
        "eponymous_vtab",
        Module::<StandardVTab>::eponymous().with_rename(),
        Some(vec![20, 22, 24, 26, 28]),
    )?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING eponymous_vtab(200)", [])?;
    check_table(
        &conn,
        "eponymous_vtab",
        &vec![
            TestData { rowid: 19, num: 20 },
            TestData { rowid: 21, num: 22 },
            TestData { rowid: 23, num: 24 },
            TestData { rowid: 25, num: 26 },
            TestData { rowid: 27, num: 28 },
        ],
    )?;
    #[rustfmt::skip]
    let results = &vec![
        TestData { rowid: 220, num: 20 },
        TestData { rowid: 222, num: 22 },
        TestData { rowid: 224, num: 24 },
        TestData { rowid: 226, num: 26 },
        TestData { rowid: 228, num: 28 },
    ];
    check_table(&conn, "tbl", &results)?;
    conn.execute("ALTER TABLE tbl RENAME TO renamed", [])?;
    check_table(&conn, "renamed", &results)?;
    Ok(())
}

#[test]
fn standard_vtab() -> rusqlite::Result<()> {
    let conn = setup()?;
    Connection::from_rusqlite(&conn).create_module(
        "standard_vtab",
        Module::<StandardVTab>::standard().with_rename(),
        Some(vec![30, 32, 34, 36, 38]),
    )?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING standard_vtab(300)", [])?;
    match conn.prepare("SELECT * FROM standard_vtab") {
        Ok(_) => panic!("standard_vtab accessed eponymously"),
        Err(e) => assert_eq!(format!("{}", e), "no such table: standard_vtab"),
    }
    #[rustfmt::skip]
    let results = &vec![
        TestData { rowid: 330, num: 30 },
        TestData { rowid: 332, num: 32 },
        TestData { rowid: 334, num: 34 },
        TestData { rowid: 336, num: 36 },
        TestData { rowid: 338, num: 38 },
    ];
    check_table(&conn, "tbl", &results)?;
    conn.execute("ALTER TABLE tbl RENAME TO renamed", [])?;
    check_table(&conn, "renamed", &results)?;
    conn.execute("DROP TABLE renamed", [])?;
    Ok(())
}
