//! Test cases for the different table types (eponymous, eponymous-only, standard).
use sqlite3_ext::{vtab::*, *};

struct TestVTab;
struct TestCursor;

impl TestVTab {
    fn connect_create() -> Result<(String, Self)> {
        Ok((
            "CREATE TABLE x ( value INTEGER NOT NULL )".to_owned(),
            TestVTab,
        ))
    }
}

impl VTab<'_> for TestVTab {
    type Aux = ();
    type Cursor = TestCursor;

    fn connect(_db: &VTabConnection, _aux: &Self::Aux, _args: &[&str]) -> Result<(String, Self)> {
        Self::connect_create()
    }

    fn best_index(&self, _index_info: &mut IndexInfo) -> Result<()> {
        Ok(())
    }

    fn open(&self) -> Result<Self::Cursor> {
        Ok(TestCursor)
    }
}

impl CreateVTab<'_> for TestVTab {
    fn create(_db: &VTabConnection, _aux: &Self::Aux, _args: &[&str]) -> Result<(String, Self)> {
        Self::connect_create()
    }

    fn destroy(&mut self) -> Result<()> {
        Ok(())
    }
}

impl VTabCursor<'_> for TestCursor {
    fn filter(
        &mut self,
        _index_num: i32,
        _index_str: Option<&str>,
        _args: &mut [&mut ValueRef],
    ) -> Result<()> {
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        unreachable!()
    }

    fn eof(&self) -> bool {
        true
    }

    fn column(&self, _: usize, _: &ColumnContext) -> Result<()> {
        unreachable!()
    }

    fn rowid(&self) -> Result<i64> {
        unreachable!()
    }
}

#[test]
#[cfg(modern_sqlite)]
fn eponymous_only() -> Result<()> {
    let conn = Database::open(":memory:")?;
    conn.create_module(
        "eponymous_only_vtab",
        EponymousOnlyModule::<TestVTab>::new().unwrap(),
        (),
    )?;
    let err = conn
        .execute("CREATE VIRTUAL TABLE tbl USING eponymous_only_vtab()", ())
        .unwrap_err();
    assert_eq!(err.to_string(), "no such module: eponymous_only_vtab");
    conn.query_row("SELECT COUNT(*) FROM eponymous_only_vtab", (), |_| Ok(()))?;
    Ok(())
}

#[test]
fn eponymous() -> Result<()> {
    let conn = Database::open(":memory:")?;
    conn.create_module("eponymous_vtab", EponymousModule::<TestVTab>::new(), ())?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING eponymous_vtab()", ())?;
    conn.query_row("SELECT COUNT(*) FROM eponymous_vtab", (), |_| Ok(()))?;
    conn.query_row("SELECT COUNT(*) FROM tbl", (), |_| Ok(()))?;
    Ok(())
}

#[test]
fn standard() -> Result<()> {
    let conn = Database::open(":memory:")?;
    conn.create_module("standard_vtab", StandardModule::<TestVTab>::new(), ())?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING standard_vtab()", ())?;
    let err = conn
        .query_row("SELECT COUNT(*) FROM standard_vtab", (), |_| Ok(()))
        .unwrap_err();
    assert_eq!(err.to_string(), "no such table: standard_vtab");
    conn.query_row("SELECT COUNT(*) FROM tbl", (), |_| Ok(()))?;
    Ok(())
}
