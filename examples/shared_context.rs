//! Example showing how to create a stateful object for an entire database connection.

use sqlite3_ext::{function::*, vtab::*, *};
use std::{cell::RefCell, rc::Rc};

/// State shared by all functions and virtual tables in the module.
#[derive(Debug)]
pub struct SharedState<'db> {
    query_count: RefCell<i64>,
    /// We are able to maintain a reference to the Connection.
    #[allow(unused)]
    db: &'db Connection,
}

impl<'db> SharedState<'db> {
    pub fn new(db: &'db Connection) -> Self {
        SharedState {
            query_count: RefCell::default(),
            db,
        }
    }

    pub fn query_count(&self) -> i64 {
        return *self.query_count.borrow();
    }

    pub fn increment(&self) {
        *self.query_count.borrow_mut() += 1;
    }
}

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    let state = Rc::new(SharedState::new(db));
    register_function(db, state.clone())?;
    register_vtab(db, state.clone())?;
    Ok(())
}

fn register_function(db: &Connection, state: Rc<SharedState>) -> Result<()> {
    db.create_scalar_function_object("query_count", &QUERY_COUNT_OPTS, QueryCount { state })
}

fn register_vtab(db: &Connection, state: Rc<SharedState>) -> Result<()> {
    db.create_module("my_vtab", MyVTab::module(), state.clone())
}

#[sqlite3_ext_fn(n_args = 0, risk_level = Innocuous)]
struct QueryCount<'db> {
    pub state: Rc<SharedState<'db>>,
}

impl<'db> ScalarFunction<'db> for QueryCount<'db> {
    fn call(&self, ctx: &Context, _: &mut [&mut ValueRef]) -> Result<()> {
        ctx.set_result(self.state.query_count())
    }
}

#[sqlite3_ext_vtab(StandardModule)]
struct MyVTab<'vtab> {
    pub state: Rc<SharedState<'vtab>>,
}

impl<'vtab> VTab<'vtab> for MyVTab<'vtab> {
    type Aux = Rc<SharedState<'vtab>>;
    type Cursor = MyCursor<'vtab>;

    fn connect(_: &VTabConnection, ctx: &Self::Aux, _: &[&str]) -> Result<(String, Self)> {
        let vtab = MyVTab { state: ctx.clone() };
        let sql = "CREATE TABLE x ( col )".to_string();
        Ok((sql, vtab))
    }

    fn best_index(&self, _: &mut IndexInfo) -> Result<()> {
        Ok(())
    }

    fn open(&'vtab self) -> Result<Self::Cursor> {
        Ok(MyCursor::new(self))
    }
}

impl<'vtab> CreateVTab<'vtab> for MyVTab<'vtab> {
    fn create(_: &VTabConnection, ctx: &Self::Aux, _: &[&str]) -> Result<(String, Self)> {
        let vtab = MyVTab { state: ctx.clone() };
        let sql = "CREATE TABLE x ( col )".to_string();
        Ok((sql, vtab))
    }

    fn destroy(self) -> DisconnectResult<Self> {
        Ok(())
    }
}

struct MyCursor<'vtab> {
    vtab: &'vtab MyVTab<'vtab>,
}

impl<'vtab> MyCursor<'vtab> {
    pub fn new(vtab: &'vtab MyVTab) -> Self {
        MyCursor { vtab }
    }
}

impl<'vtab> VTabCursor for MyCursor<'vtab> {
    fn filter(&mut self, _: i32, _: Option<&str>, _: &mut [&mut ValueRef]) -> Result<()> {
        self.vtab.state.increment();
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        Ok(())
    }

    fn eof(&mut self) -> bool {
        true
    }

    fn column(&mut self, _: usize, _: &ColumnContext) -> Result<()> {
        unreachable!()
    }

    fn rowid(&mut self) -> Result<i64> {
        unreachable!()
    }
}

#[cfg(test)]
#[test]
fn test() -> Result<()> {
    let db = Database::open(":memory:")?;
    init(&db)?;
    db.execute("CREATE VIRTUAL TABLE tbl USING my_vtab ()", ())?;

    let query_count = db.query_row("SELECT query_count()", (), |r| Ok(r[0].get_i64()))?;
    assert_eq!(query_count, 0);

    db.query_row("SELECT COUNT(*) FROM tbl", (), |_| Ok(()))?;
    let query_count = db.query_row("SELECT query_count()", (), |r| Ok(r[0].get_i64()))?;
    assert_eq!(query_count, 1);

    Ok(())
}
