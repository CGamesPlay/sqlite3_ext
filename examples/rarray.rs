//! Rust implementation of the carray table-valued function distributed with SQLite.
//!
//! See the example usage at the end of this file.
//!
//! For more information, consult [the SQLite documentation](https://sqlite.org/carray.html).

use sqlite3_ext::{vtab::*, *};
use std::rc::Rc;

const COLUMN_POINTER: i32 = 1;

/// Passed-in arrays must be of this type.
type ArrayPointer = Rc<[Value]>;

#[sqlite3_ext_vtab(EponymousModule)]
struct Rarray {}

impl VTab<'_> for Rarray {
    type Aux = ();
    type Cursor = Cursor;

    fn connect(db: &VTabConnection, _aux: &Self::Aux, _args: &[&str]) -> Result<(String, Self)> {
        db.set_risk_level(RiskLevel::Innocuous);
        Ok((
            "CREATE TABLE x ( value, pointer HIDDEN )".to_owned(),
            Rarray {},
        ))
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        let mut has_ptr = false;
        for mut constraint in index_info.constraints() {
            if !constraint.usable() {
                continue;
            }
            if constraint.op() != ConstraintOp::Eq {
                continue;
            }
            if constraint.column() == COLUMN_POINTER {
                has_ptr = true;
                constraint.set_argv_index(Some(0));
                constraint.set_omit(true);
            }
        }
        if has_ptr {
            index_info.set_estimated_cost(1f64);
            index_info.set_estimated_rows(100);
        } else {
            index_info.set_estimated_cost(2147483647f64);
            index_info.set_estimated_rows(2147483647);
        }
        Ok(())
    }

    fn open(&self) -> Result<Self::Cursor> {
        Ok(Cursor::default())
    }
}

#[derive(Default, Debug)]
struct Cursor {
    rowid: i64,
    array: Option<ArrayPointer>,
}

impl VTabCursor for Cursor {
    fn filter(&mut self, _: i32, _: Option<&str>, args: &mut [&mut ValueRef]) -> Result<()> {
        self.rowid = 0;
        self.array = if args.len() > 0 {
            args[0].get_ref::<ArrayPointer>().cloned()
        } else {
            None
        };
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        self.rowid += 1;
        Ok(())
    }

    fn eof(&mut self) -> bool {
        self.rowid as usize >= self.array.as_ref().map(|a| a.len()).unwrap_or(0)
    }

    fn column(&mut self, idx: usize, c: &ColumnContext) -> Result<()> {
        match idx as _ {
            COLUMN_POINTER => Ok(()),
            _ => c.set_result(self.array.as_ref().map(|a| a[self.rowid as usize].clone())),
        }
    }

    fn rowid(&mut self) -> Result<i64> {
        Ok(self.rowid)
    }
}

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    // PassedRef requires SQLite 3.20.0
    sqlite3_require_version!(3_020_000)?;
    db.create_module("rarray", Rarray::module(), ())?;
    Ok(())
}

#[cfg(all(test, feature = "static"))]
mod test {
    use super::*;

    fn setup() -> Result<Database> {
        let conn = Database::open(":memory:")?;
        init(&conn)?;
        Ok(conn)
    }

    #[test]
    fn example() -> Result<()> {
        let conn = setup()?;
        let array: ArrayPointer = vec![1, 2, 3, 4].into_iter().map(Value::from).collect();
        let results: Vec<i64> = conn
            .prepare("SELECT * FROM rarray(?)")?
            .query([PassedRef::new(array)])?
            .map(|row| Ok(row[0].get_i64()))
            .collect()?;
        assert_eq!(results, vec![1, 2, 3, 4]);
        let results: i64 = conn.query_row("SELECT COUNT(*) FROM rarray(NULL)", (), |r| {
            Ok(r[0].get_i64())
        })?;
        assert_eq!(results, 0);
        Ok(())
    }
}
