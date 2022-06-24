use bigdecimal::BigDecimal;
use sqlite3_ext::{function::*, *};
use std::{cmp::Ordering, str::FromStr};

fn process_value(a: &ValueRef) -> Result<Option<BigDecimal>> {
    if a.value_type() == ValueType::Null {
        Ok(None)
    } else {
        Ok(Some(
            BigDecimal::from_str(a.get_str()?).map_err(|_| Error::InvalidConversion)?,
        ))
    }
}

fn process_args(args: &[&ValueRef]) -> Result<Vec<Option<BigDecimal>>> {
    args.iter()
        .copied()
        .map(process_value)
        .collect::<Result<_>>()
}

macro_rules! scalar_method {
    ($name:ident as ( $a:ident, $b:ident ) -> $ty:ty => $ret:expr) => {
        fn $name(_: &Context, args: &[&ValueRef]) -> Result<Option<$ty>> {
            let mut args = process_args(args)?.into_iter();
            let a = args.next().unwrap_or(None);
            let b = args.next().unwrap_or(None);
            if let (Some($a), Some($b)) = (a, b) {
                Ok(Some($ret))
            } else {
                Ok(None)
            }
        }
    };
}

scalar_method!(decimal_add as (a, b) -> String => format!("{}", (a + b).normalized()));
scalar_method!(decimal_sub as (a, b) -> String => format!("{}", (a - b).normalized()));
scalar_method!(decimal_mul as (a, b) -> String => format!("{}", (a * b).normalized()));
scalar_method!(decimal_cmp as (a, b) -> i32 => {
    match a.cmp(&b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
});

struct Sum {
    cur: Result<BigDecimal>,
}

impl Default for Sum {
    fn default() -> Self {
        Sum {
            cur: Ok(BigDecimal::default()),
        }
    }
}

impl AggregateFunction for Sum {
    type Return = Option<String>;
    const DEFAULT_VALUE: Option<String> = None;

    fn step(&mut self, _context: &Context, args: &[&ValueRef]) {
        let cur = match &self.cur {
            Ok(x) => x,
            Err(_) => return,
        };
        match process_value(args.first().unwrap()) {
            Ok(Some(x)) => {
                self.cur = Ok(cur + x);
            }
            Ok(None) => (),
            Err(x) => {
                self.cur = Err(x);
            }
        };
    }

    fn value(&self, _context: &Context) -> Result<Self::Return> {
        match &self.cur {
            Ok(x) => Ok(Some(format!("{}", x.normalized()))),
            Err(e) => Err(e.clone()),
        }
    }

    fn inverse(&mut self, _context: &Context, args: &[&ValueRef]) {
        let cur = match &self.cur {
            Ok(x) => x,
            Err(_) => return,
        };
        match process_value(args.first().unwrap()) {
            Ok(Some(x)) => {
                self.cur = Ok(cur - x);
            }
            Ok(None) => (),
            Err(x) => {
                self.cur = Err(x);
            }
        };
    }
}

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    db.create_scalar_function("decimal_add", 2, 0, decimal_add)?;
    db.create_scalar_function("decimal_sub", 2, 0, decimal_sub)?;
    db.create_scalar_function("decimal_mul", 2, 0, decimal_mul)?;
    db.create_scalar_function("decimal_cmp", 2, 0, decimal_cmp)?;
    db.create_scalar_function("decimal_cmp", 2, 0, decimal_cmp)?;
    db.create_aggregate_function::<Sum>("decimal_sum", 1, 0)?;
    // decimal collating sequence
    Ok(())
}

#[cfg(all(test, feature = "static"))]
mod test {
    use super::*;
    use rusqlite;

    fn setup() -> rusqlite::Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open_in_memory()?;
        init(Connection::from_rusqlite(&conn))?;
        Ok(conn)
    }

    fn case<T: rusqlite::types::FromSql + std::fmt::Debug + PartialEq>(
        data: Vec<(&str, T)>,
    ) -> rusqlite::Result<()> {
        let conn = setup()?;
        let (sql, expected): (Vec<&str>, Vec<T>) = data.into_iter().unzip();
        let sql = format!("SELECT {}", sql.join(", "));
        println!("{}", sql);
        let ret: Vec<T> = conn.query_row(&sql, [], |r| {
            (0..expected.len())
                .map(|i| r.get::<_, T>(i))
                .collect::<rusqlite::Result<_>>()
        })?;
        assert_eq!(ret, expected);
        Ok(())
    }

    #[test]
    fn decimal_add() -> rusqlite::Result<()> {
        case(vec![
            (
                "decimal_add('1000000000000000', '0.0000000000000001')",
                Some("1000000000000000.0000000000000001".to_owned()),
            ),
            ("decimal_add(NULL, '0')", None),
            ("decimal_add('0', NULL)", None),
            ("decimal_add(NULL, NULL)", None),
        ])
    }

    #[test]
    fn decimal_sub() -> rusqlite::Result<()> {
        case(vec![
            (
                "decimal_sub('1000000000000000', '0.0000000000000001')",
                Some("999999999999999.9999999999999999".to_owned()),
            ),
            ("decimal_sub(NULL, '0')", None),
            ("decimal_sub('0', NULL)", None),
            ("decimal_sub(NULL, NULL)", None),
        ])
    }

    #[test]
    fn decimal_mul() -> rusqlite::Result<()> {
        case(vec![
            (
                "decimal_mul('1000000000000000', '0.0000000000000001')",
                Some("0.1".to_owned()),
            ),
            ("decimal_mul(NULL, '0')", None),
            ("decimal_mul('0', NULL)", None),
            ("decimal_mul(NULL, NULL)", None),
        ])
    }

    #[test]
    fn decimal_cmp() -> rusqlite::Result<()> {
        case(vec![
            ("decimal_cmp('1', '-1')", Some(1)),
            ("decimal_cmp('-1', '1')", Some(-1)),
            ("decimal_cmp('1', '1')", Some(0)),
            ("decimal_cmp(NULL, '0')", None),
            ("decimal_cmp('0', NULL)", None),
            ("decimal_cmp(NULL, NULL)", None),
        ])
    }

    fn aggregate_case<T: rusqlite::types::FromSql + std::fmt::Debug + PartialEq>(
        expr: &str,
        data: Vec<&str>,
        expected: Vec<T>,
    ) -> rusqlite::Result<()> {
        let conn = setup()?;
        let sql = format!(
            "SELECT {} FROM ( VALUES {} )",
            expr,
            data.iter()
                .map(|s| format!("({})", s))
                .collect::<Vec<String>>()
                .join(", ")
        );
        println!("{}", sql);
        let ret: Vec<T> = conn
            .prepare(&sql)?
            .query_map([], |r| r.get::<_, T>(0))?
            .into_iter()
            .collect::<rusqlite::Result<_>>()?;
        assert_eq!(ret, expected);
        Ok(())
    }

    #[test]
    fn decimal_sum() -> rusqlite::Result<()> {
        aggregate_case(
            "decimal_sum(column1)",
            vec!["1000000000000000", "0.0000000000000001", "1"],
            vec![Some("1000000000000001.0000000000000001".to_owned())],
        )?;
        aggregate_case(
            "decimal_sum(column1)",
            vec!["1", "NULL"],
            vec![Some("1".to_owned())],
        )?;
        aggregate_case(
            "decimal_sum(column1)",
            vec!["NULL"],
            vec![Some("0".to_owned())],
        )?;
        case(vec![("decimal_sum(NULL)", Some("0".to_owned()))])?;
        case(vec![("decimal_sum(1) WHERE 1 = 0", None as Option<String>)])?;
        aggregate_case(
            "decimal_sum(column1) OVER ( ROWS 1 PRECEDING )",
            vec![
                "1000000000000000",
                "0.0000000000000001",
                "NULL",
                "NULL",
                "1",
            ],
            vec![
                Some("1000000000000000".to_owned()),
                Some("1000000000000000.0000000000000001".to_owned()),
                Some("0.0000000000000001".to_owned()),
                Some("0".to_owned()),
                Some("1".to_owned()),
            ],
        )?;
        Ok(())
    }
}
