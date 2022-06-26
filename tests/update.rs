use helpers::*;
use sqlite3_ext::{vtab::*, *};

mod helpers;

#[derive(Debug, PartialEq)]
struct TestData {
    rowid: i64,
    num: i64,
}

struct ListVTab {
    lifecycle: VTabLifecycle,
    rows: Vec<i64>,
}

impl<'vtab> ListVTab {
    fn connect_create() -> Result<(String, Self)> {
        Ok((
            "CREATE TABLE x ( value INTEGER NOT NULL )".to_owned(),
            ListVTab {
                lifecycle: VTabLifecycle::default(),
                rows: vec![],
            },
        ))
    }
}

impl<'vtab> VTab<'vtab> for ListVTab {
    type Aux = ();
    type Cursor = ListCursor<'vtab>;

    fn connect(
        _db: &mut VTabConnection,
        aux: &'vtab Self::Aux,
        args: &[&str],
    ) -> Result<(String, Self)> {
        let (sql, mut vtab) = Self::connect_create()?;
        vtab.lifecycle.xConnect(aux, args);
        Ok((sql, vtab))
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        self.lifecycle.xBestIndex(index_info);
        Ok(())
    }

    fn open(&'vtab mut self) -> Result<Self::Cursor> {
        let lifecycle = self.lifecycle.xOpen();
        Ok(ListCursor::new(
            lifecycle,
            Box::new(self.rows.clone().into_iter()),
        ))
    }
}

impl<'vtab> CreateVTab<'vtab> for ListVTab {
    fn create(
        _db: &mut VTabConnection,
        aux: &'vtab Self::Aux,
        args: &[&str],
    ) -> Result<(String, Self)> {
        let (sql, mut vtab) = Self::connect_create()?;
        vtab.lifecycle.xCreate(aux, args);
        Ok((sql, vtab))
    }

    fn destroy(&mut self) -> Result<()> {
        self.lifecycle.xDestroy();
        Ok(())
    }
}

impl<'vtab> UpdateVTab<'vtab> for ListVTab {
    fn insert(&mut self, args: &[&ValueRef]) -> Result<i64> {
        self.lifecycle.xUpdateInsert(args);
        self.rows.push(args[1].get_i64());
        Ok((self.rows.len() - 1) as _)
    }

    fn update(&mut self, rowid: &ValueRef, args: &[&ValueRef]) -> Result<()> {
        self.lifecycle.xUpdateUpdate(rowid, args);
        let rowid = rowid.get_i64() as usize;
        self.rows[rowid] = args[1].get_i64();
        Ok(())
    }

    fn delete(&mut self, rowid: &ValueRef) -> Result<()> {
        self.lifecycle.xUpdateDelete(rowid);
        let rowid = rowid.get_i64() as usize;
        self.rows.remove(rowid);
        Ok(())
    }
}

pub struct ListCursor<'vtab> {
    lifecycle: CursorLifecycle<'vtab>,
    iter: Box<dyn Iterator<Item = i64>>,
    current: Option<(i64, i64)>,
}

impl<'vtab> ListCursor<'vtab> {
    pub fn new(lifecycle: CursorLifecycle<'vtab>, iter: Box<dyn Iterator<Item = i64>>) -> Self {
        ListCursor {
            lifecycle,
            iter,
            current: None,
        }
    }
}

impl<'vtab> VTabCursor for ListCursor<'vtab> {
    fn filter(
        &mut self,
        index_num: usize,
        index_str: Option<&str>,
        args: &[&ValueRef],
    ) -> Result<()> {
        self.lifecycle.xFilter(index_num, index_str, args);
        self.current = self.iter.next().map(|v| (0, v));
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        self.lifecycle.xNext();
        if let Some((rowid, _)) = self.current {
            self.current = self.iter.next().map(|v| (rowid + 1, v))
        }
        Ok(())
    }

    fn eof(&self) -> bool {
        self.lifecycle.xEof();
        match self.current {
            Some(_) => false,
            None => true,
        }
    }

    fn column(&self, i: usize) -> Result<Value> {
        self.lifecycle.xColumn(i);
        Ok(match self.current {
            Some((_, v)) => v.into(),
            _ => ().into(),
        })
    }

    fn rowid(&self) -> Result<i64> {
        self.lifecycle.xRowid();
        self.current
            .map(|(rowid, _)| rowid)
            .ok_or(Error::Sqlite(ffi::SQLITE_MISUSE))
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
fn update() -> rusqlite::Result<()> {
    let conn = setup()?;
    Connection::from_rusqlite(&conn).create_module(
        "vtab",
        StandardModule::<ListVTab>::new().with_update(),
        (),
    )?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING vtab", [])?;
    conn.execute("INSERT INTO tbl VALUES (100), (200)", [])?;
    check_table(
        &conn,
        "tbl",
        &vec![
            TestData { rowid: 0, num: 100 },
            TestData { rowid: 1, num: 200 },
        ],
    )?;
    conn.execute("UPDATE tbl SET value = 101 WHERE value = 100", [])?;
    check_table(
        &conn,
        "tbl",
        &vec![
            TestData { rowid: 0, num: 101 },
            TestData { rowid: 1, num: 200 },
        ],
    )?;
    conn.execute("DELETE FROM tbl WHERE value = 101", [])?;
    check_table(&conn, "tbl", &vec![TestData { rowid: 0, num: 200 }])?;
    Ok(())
}
