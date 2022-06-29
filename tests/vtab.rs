use sqlite3_ext::{vtab::*, *};

struct StandardVTab {}
struct StandardCursor {}

impl<'vtab> StandardVTab {
    fn connect_create() -> Result<(String, Self)> {
        Ok((
            "CREATE TABLE x ( value INTEGER NOT NULL )".to_owned(),
            StandardVTab {},
        ))
    }
}

impl<'vtab> VTab<'vtab> for StandardVTab {
    type Aux = ();
    type Cursor = StandardCursor;

    fn connect(
        _db: &mut VTabConnection,
        _aux: &'vtab Self::Aux,
        _args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create()
    }

    fn best_index(&self, _: &mut IndexInfo) -> Result<()> {
        Ok(())
    }

    fn open(&'vtab mut self) -> Result<Self::Cursor> {
        Ok(StandardCursor {})
    }
}

impl<'vtab> CreateVTab<'vtab> for StandardVTab {
    fn create(
        _db: &mut VTabConnection,
        _aux: &'vtab Self::Aux,
        _args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create()
    }

    fn destroy(&mut self) -> Result<()> {
        Ok(())
    }
}

impl VTabCursor for StandardCursor {
    type ColumnType = ();

    fn filter(
        &mut self,
        _index_num: usize,
        _index_str: Option<&str>,
        _args: &[&ValueRef],
    ) -> Result<()> {
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        unreachable!()
    }

    fn eof(&self) -> bool {
        true
    }

    fn column(&self, _: usize) {
        unreachable!()
    }

    fn rowid(&self) -> Result<i64> {
        unreachable!()
    }
}

#[test]
#[cfg(any(not(feature = "static"), feature = "static_modern"))]
fn eponymous_only_vtab() -> rusqlite::Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    Connection::from_rusqlite(&conn).create_module(
        "eponymous_only_vtab",
        EponymousOnlyModule::<StandardVTab>::new().unwrap(),
        (),
    )?;
    let err = conn
        .execute(
            "CREATE VIRTUAL TABLE tbl USING eponymous_only_vtab(300)",
            [],
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "no such module: eponymous_only_vtab");
    conn.query_row("SELECT COUNT(*) FROM eponymous_only_vtab", [], |_| Ok(()))?;
    Ok(())
}

#[test]
fn eponymous_vtab() -> rusqlite::Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    Connection::from_rusqlite(&conn).create_module(
        "eponymous_vtab",
        EponymousModule::<StandardVTab>::new(),
        (),
    )?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING eponymous_vtab(200)", [])?;
    conn.query_row("SELECT COUNT(*) FROM eponymous_vtab", [], |_| Ok(()))?;
    conn.query_row("SELECT COUNT(*) FROM tbl", [], |_| Ok(()))?;
    Ok(())
}

#[test]
fn standard_vtab() -> rusqlite::Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    Connection::from_rusqlite(&conn).create_module(
        "standard_vtab",
        StandardModule::<StandardVTab>::new(),
        (),
    )?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING standard_vtab(300)", [])?;
    let err = conn
        .query_row("SELECT COUNT(*) FROM standard_vtab", [], |_| Ok(()))
        .unwrap_err();
    assert_eq!(err.to_string(), "no such table: standard_vtab");
    conn.query_row("SELECT COUNT(*) FROM tbl", [], |_| Ok(()))?;
    Ok(())
}
