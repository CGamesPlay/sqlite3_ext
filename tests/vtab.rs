use sqlite3_ext::{function::*, vtab::*, *};

struct TestVTab<'vtab> {
    functions: VTabFunctionList<'vtab, Self>,
}
struct TestCursor {
    eof: bool,
}

impl<'vtab> TestVTab<'vtab> {
    fn connect_create() -> Result<(String, Self)> {
        let functions = VTabFunctionList::default();
        functions.add_method(1, "my_func", None, Self::custom_method);
        Ok((
            "CREATE TABLE x ( value INTEGER NOT NULL )".to_owned(),
            TestVTab { functions },
        ))
    }

    fn custom_method(&self, _: &Context, _: &mut [&mut ValueRef]) -> bool {
        true
    }
}

impl<'vtab> VTab<'vtab> for TestVTab<'vtab> {
    type Aux = ();
    type Cursor = TestCursor;

    fn connect(
        _db: &mut VTabConnection,
        _aux: &Self::Aux,
        _args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create()
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        assert_eq!(index_info.distinct_mode(), DistinctMode::Ordered);
        Ok(())
    }

    fn open(&mut self) -> Result<Self::Cursor> {
        Ok(TestCursor { eof: false })
    }
}

impl<'vtab> CreateVTab<'vtab> for TestVTab<'vtab> {
    fn create(
        _db: &mut VTabConnection,
        _aux: &Self::Aux,
        _args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create()
    }

    fn destroy(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<'vtab> FindFunctionVTab<'vtab> for TestVTab<'vtab> {
    fn functions(&self) -> &VTabFunctionList<'vtab, Self> {
        &self.functions
    }
}

impl VTabCursor for TestCursor {
    type ColumnType = ();

    fn filter(
        &mut self,
        _index_num: i32,
        _index_str: Option<&str>,
        _args: &mut [&mut ValueRef],
    ) -> Result<()> {
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        self.eof = true;
        Ok(())
    }

    fn eof(&self) -> bool {
        self.eof
    }

    fn column(&self, _: usize) {}

    fn rowid(&self) -> Result<i64> {
        Ok(0)
    }
}

#[test]
fn eponymous_only_vtab() -> rusqlite::Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    Connection::from_rusqlite(&conn).create_module(
        "eponymous_only_vtab",
        EponymousOnlyModule::<TestVTab>::new().unwrap(),
        (),
    )?;
    let err = conn
        .execute("CREATE VIRTUAL TABLE tbl USING eponymous_only_vtab()", [])
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
        EponymousModule::<TestVTab>::new(),
        (),
    )?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING eponymous_vtab()", [])?;
    conn.query_row("SELECT COUNT(*) FROM eponymous_vtab", [], |_| Ok(()))?;
    conn.query_row("SELECT COUNT(*) FROM tbl", [], |_| Ok(()))?;
    Ok(())
}

fn setup() -> rusqlite::Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open_in_memory()?;
    Connection::from_rusqlite(&conn).create_module(
        "standard_vtab",
        StandardModule::<TestVTab>::new(),
        (),
    )?;
    conn.execute("CREATE VIRTUAL TABLE tbl USING standard_vtab()", [])?;
    Ok(conn)
}

#[test]
fn standard_vtab() -> rusqlite::Result<()> {
    let conn = setup()?;
    let err = conn
        .query_row("SELECT COUNT(*) FROM standard_vtab", [], |_| Ok(()))
        .unwrap_err();
    assert_eq!(err.to_string(), "no such table: standard_vtab");
    conn.query_row("SELECT COUNT(*) FROM tbl", [], |_| Ok(()))?;
    Ok(())
}

#[test]
fn find_function() -> rusqlite::Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    {
        let conn = Connection::from_rusqlite(&conn);
        conn.create_module(
            "standard_vtab",
            StandardModule::<TestVTab>::new().with_find_function(),
            (),
        )?;
        conn.create_overloaded_function("my_func", &FunctionOptions::default().set_n_args(1))?;
    }
    conn.execute("CREATE VIRTUAL TABLE tbl USING standard_vtab()", [])?;
    conn.query_row("SELECT value FROM tbl WHERE my_func(value)", [], |_| Ok(()))?;
    conn.query_row("SELECT value FROM tbl WHERE my_func(value)", [], |_| Ok(()))?;
    Ok(())
}
