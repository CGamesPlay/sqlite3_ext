//! Rust implementation of the generate_series table-valued function distributed with SQLite.
//!
//! For more information, consult [the SQLite documentation](https://www.sqlite.org/series.html).

use sqlite3_ext::{vtab::*, *};

const COLUMN_START: i32 = 1;
const COLUMN_STOP: i32 = 2;
const COLUMN_STEP: i32 = 3;

#[sqlite3_ext_vtab(EponymousModule)]
struct GenerateSeries {}

impl VTab<'_> for GenerateSeries {
    type Aux = ();
    type Cursor = Cursor;

    fn connect(db: &VTabConnection, _aux: &Self::Aux, _args: &[&str]) -> Result<(String, Self)> {
        db.set_risk_level(RiskLevel::Innocuous);
        Ok((
            "CREATE TABLE x ( value, start HIDDEN, stop HIDDEN, step HIDDEN )".to_owned(),
            GenerateSeries {},
        ))
    }

    /// Describe the "query plan" for this sequence.
    ///
    /// The best_index method looks for equality constraints against the hidden start,
    /// stop, and step columns, and if present, it uses those constraints to bound the
    /// sequence of generated values.  If the equality constraints are missing, it uses 0
    /// for start, [isize::MAX] for stop, and 1 for step. best_index returns a small cost
    /// when both start and stop are available, and a very large cost if either start or
    /// stop are unavailable.  This encourages the query planner to order joins such that
    /// the bounds of the series are well-defined.
    ///
    /// The query plan is represented by bits in idxNum:
    ///
    ///    (1)  start = $value  -- constraint exists
    ///    (2)  stop = $value   -- constraint exists
    ///    (4)  step = $value   -- constraint exists
    ///    (8)  output in descending order
    ///    (16) output in ascending order
    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        let mut query_plan: i32 = 0;
        let mut unusable_mask: i32 = 0;
        let mut has_start = false;
        let mut arg_index: [Option<usize>; 3] = [None, None, None];
        for (i, constraint) in index_info.constraints().enumerate() {
            if constraint.column() < COLUMN_START {
                continue;
            }
            let bit = (constraint.column() - COLUMN_START) as usize;
            assert!(bit <= 2);
            if constraint.column() == COLUMN_START {
                has_start = true;
            }
            if !constraint.usable() {
                unusable_mask |= 1 << bit;
                continue;
            } else if constraint.op() == ConstraintOp::Eq {
                query_plan |= 1 << bit;
                arg_index[bit] = Some(i);
            }
        }
        let mut constraints: Vec<_> = index_info.constraints().collect();
        let mut next_idx = 0;
        for i in 0..3 {
            if let Some(j) = arg_index[i] {
                constraints[j].set_argv_index(Some(next_idx));
                constraints[j].set_omit(true);
                next_idx += 1;
            }
        }
        if !has_start {
            return Err(Error::Module(
                "first argument to \"generate_series()\" missing or unusable".to_owned(),
            ));
        }
        if (unusable_mask & !query_plan) != 0 {
            // The start, stop, and step columns are inputs.  Therefore if there
            // are unusable constraints on any of start, stop, or step then this
            // plan is unusable
            return Err(SQLITE_CONSTRAINT);
        }
        if (query_plan & 3) == 3 {
            // Both start= and stop= boundaries are available.  This is the the
            // preferred case
            index_info.set_estimated_cost((2 - ((query_plan & 4) != 0) as isize) as f64);
            index_info.set_estimated_rows(1000);
            if let Some(order) = index_info.order_by().next() {
                if order.column() == 0 {
                    if order.desc() {
                        query_plan |= 8;
                    } else {
                        query_plan |= 16;
                    }
                    index_info.set_order_by_consumed(true);
                }
            }
        } else {
            index_info.set_estimated_rows(i64::MAX / 2);
        }
        index_info.set_index_num(query_plan);
        Ok(())
    }

    fn open(&self) -> Result<Self::Cursor> {
        Ok(Cursor::default())
    }
}

#[derive(Default, Debug)]
struct Cursor {
    desc: bool,
    rowid: i64,
    value: i64,
    min_value: i64,
    max_value: i64,
    step: i64,
}

impl VTabCursor for Cursor {
    fn filter(
        &mut self,
        query_plan: i32,
        _: Option<&str>,
        args: &mut [&mut ValueRef],
    ) -> Result<()> {
        let mut query_plan = query_plan;
        for a in args.iter() {
            // If any of the constraints have a NULL value, then return no rows.
            // See ticket https://www.sqlite.org/src/info/fac496b61722daf2
            if a.is_null() {
                self.max_value = -1;
                return Ok(());
            }
        }
        let mut args = args.into_iter();
        if query_plan & 1 != 0 {
            self.min_value = (*args.next().unwrap()).get_i64();
        }
        if query_plan & 2 != 0 {
            self.max_value = (*args.next().unwrap()).get_i64();
        } else {
            self.max_value = i64::MAX;
        }
        if query_plan & 4 != 0 {
            self.step = (*args.next().unwrap()).get_i64();
            if self.step == 0 {
                self.step = 1;
            } else if self.step < 0 {
                self.step = -self.step;
                if query_plan & 16 == 0 {
                    query_plan |= 8;
                }
            }
        } else {
            self.step = 1;
        }
        if query_plan & 8 != 0 {
            self.desc = true;
            self.value = self.max_value;
            if self.step > 0 {
                self.value -= (self.max_value - self.min_value) % self.step;
            }
        } else {
            self.desc = false;
            self.value = self.min_value;
        }
        self.rowid = 1;
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        if self.desc {
            self.value -= self.step;
        } else {
            self.value += self.step;
        }
        self.rowid += 1;
        Ok(())
    }

    fn eof(&mut self) -> bool {
        if self.desc {
            self.value < self.min_value
        } else {
            self.value > self.max_value
        }
    }

    fn column(&mut self, idx: usize, c: &ColumnContext) -> Result<()> {
        let ret = match idx as _ {
            COLUMN_START => self.min_value,
            COLUMN_STOP => self.max_value,
            COLUMN_STEP => self.step,
            _ => self.value,
        };
        c.set_result(ret)
    }

    fn rowid(&mut self) -> Result<i64> {
        Ok(self.rowid)
    }
}

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    db.create_module("generate_series", GenerateSeries::module(), ())?;
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
        let results: Vec<i64> = conn
            .prepare("SELECT value FROM generate_series(5, 100, 5)")?
            .query(())?
            .map(|row| Ok(row[0].get_i64()))
            .collect()?;
        assert_eq!(
            results,
            vec![5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95, 100]
        );
        let results: Vec<i64> = conn
            .prepare("SELECT value FROM generate_series(20) LIMIT 10")?
            .query(())?
            .map(|row| Ok(row[0].get_i64()))
            .collect()?;
        assert_eq!(results, vec![20, 21, 22, 23, 24, 25, 26, 27, 28, 29]);
        Ok(())
    }

    macro_rules! case {
        ($test_name:ident { sql: $sql:expr, expected: $expected:expr, }) => {
            #[test]
            fn $test_name() -> Result<()> {
                let conn = setup()?;
                let results = conn
                    .prepare($sql)?
                    .query(())?
                    .map(|row| Ok(row[0].get_i64()))
                    .collect();
                assert_eq!(results, $expected);
                Ok(())
            }
        };
    }

    case!(max_lt_min {
        sql: "SELECT value FROM generate_series(10, 5)",
        expected: Ok(vec![]),
    });

    case!(max_eq_min {
        sql: "SELECT value FROM generate_series(17, 17)",
        expected: Ok(vec![17]),
    });

    case!(negative_step {
        sql: "SELECT value FROM generate_series(5, 10, -2)",
        expected: Ok(vec![9, 7, 5]),
    });

    case!(order_desc {
        sql: "SELECT value FROM generate_series(5, 10) ORDER BY value DESC",
        expected: Ok(vec![10, 9, 8, 7, 6, 5]),
    });

    case!(order_asc {
        sql: "SELECT value FROM generate_series(5, 10, -2) ORDER BY value",
        expected: Ok(vec![5, 7, 9]),
    });

    case!(null_arg {
        sql: "SELECT value FROM generate_series(5, 10, NULL)",
        expected: Ok(vec![]),
    });

    case!(only_limit {
        sql: "SELECT value FROM generate_series(1) LIMIT 5",
        expected: Ok(vec![1, 2, 3, 4, 5]),
    });
}
