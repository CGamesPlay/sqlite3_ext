use sqlite3_ext::{vtab::*, *};

pub trait TestHooks: Sized {
    fn connect_create<'a>(&'a self, _vtab: &mut TestVTab<'a, Self>) {}

    fn best_index<'a>(
        &'a self,
        _vtab: &TestVTab<'a, Self>,
        _index_info: &mut IndexInfo,
    ) -> Result<()> {
        Ok(())
    }

    fn filter<'a>(
        &self,
        _cursor: &mut TestVTabCursor<'a, Self>,
        _args: &mut [&mut ValueRef],
    ) -> Result<()> {
        Ok(())
    }
}

pub fn setup<Hooks: TestHooks>(hooks: &Hooks) -> Result<Database> {
    let conn = Database::open(":memory:")?;
    conn.create_module("vtab", TestVTab::module(), hooks)?;
    conn.execute(
        "CREATE VIRTUAL TABLE tbl USING vtab(schema='CREATE TABLE x(a,b,c)', rows=3)",
        (),
    )?;
    Ok(conn)
}

#[sqlite3_ext_vtab(StandardModule, FindFunctionVTab)]
pub struct TestVTab<'vtab, Hooks: TestHooks + 'vtab> {
    hooks: &'vtab Hooks,
    pub functions: VTabFunctionList<'vtab, Self>,
    num_rows: i64,
}

pub struct TestVTabCursor<'vtab, Hooks: TestHooks + 'vtab> {
    vtab: &'vtab TestVTab<'vtab, Hooks>,
    rowid: i64,
}

impl<'vtab, Hooks: TestHooks + 'vtab> TestVTab<'vtab, Hooks> {
    fn connect_create(hooks: &'vtab Hooks) -> Result<(String, Self)> {
        let mut vtab = TestVTab {
            hooks: hooks.clone(),
            functions: VTabFunctionList::default(),
            num_rows: 3,
        };
        hooks.connect_create(&mut vtab);
        Ok(("CREATE TABLE x(a, b, c)".to_owned(), vtab))
    }
}

impl<'vtab, Hooks: TestHooks + 'vtab> VTab<'vtab> for TestVTab<'vtab, Hooks> {
    type Aux = &'vtab Hooks;
    type Cursor = TestVTabCursor<'vtab, Hooks>;

    fn connect(_: &VTabConnection, aux: &'vtab Self::Aux, _: &[&str]) -> Result<(String, Self)> {
        Self::connect_create(aux)
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        self.hooks.best_index(&self, index_info)
    }

    fn open(&'vtab self) -> Result<Self::Cursor> {
        let ret = TestVTabCursor {
            vtab: self,
            rowid: 0,
        };
        Ok(ret)
    }
}

impl<'vtab, Hooks: TestHooks + 'vtab> CreateVTab<'vtab> for TestVTab<'vtab, Hooks> {
    fn create(_: &VTabConnection, aux: &'vtab Self::Aux, _: &[&str]) -> Result<(String, Self)> {
        Self::connect_create(aux)
    }

    fn destroy(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<'vtab, Hooks: TestHooks + 'vtab> FindFunctionVTab<'vtab> for TestVTab<'vtab, Hooks> {
    fn functions(&self) -> &VTabFunctionList<'vtab, Self> {
        &self.functions
    }
}

impl<'vtab, Hooks: TestHooks + 'vtab> VTabCursor for TestVTabCursor<'vtab, Hooks> {
    fn filter(&mut self, _: i32, _: Option<&str>, args: &mut [&mut ValueRef]) -> Result<()> {
        self.rowid = 0;
        self.vtab.hooks.filter(self, args)
    }

    fn next(&mut self) -> Result<()> {
        self.rowid += 1;
        Ok(())
    }

    fn eof(&mut self) -> bool {
        self.rowid >= self.vtab.num_rows
    }

    fn column(&mut self, idx: usize, ctx: &ColumnContext) -> Result<()> {
        const ALPHABET: &[u8] = "abcdefghijklmnopqrstuvwxyz".as_bytes();
        let ret = match () {
            _ if ctx.nochange() => Err(Error::NoChange),
            _ => Ok(ALPHABET
                .get(idx)
                .map(|l| format!("{}{}", *l as char, self.rowid))
                .unwrap_or_else(|| format!("{{{}}}{}", idx, self.rowid))),
        };
        ctx.set_result(ret)
    }

    fn rowid(&mut self) -> Result<i64> {
        Ok(self.rowid)
    }
}
