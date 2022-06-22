use bigdecimal::BigDecimal;
use sqlite3_ext::{function::*, *};
use std::{cmp::Ordering, str::FromStr};

fn process_args(args: &[&Value]) -> Result<Vec<Option<BigDecimal>>> {
    args.iter()
        .map(|a| {
            if a.value_type() == ValueType::Null {
                Ok(None)
            } else {
                Ok(Some(
                    BigDecimal::from_str(a.get_str()?).map_err(|_| Error::InvalidConversion)?,
                ))
            }
        })
        .collect::<Result<_>>()
}

macro_rules! scalar_method {
    ($name:ident as ( $a:ident, $b:ident ) => $ret:expr) => {
        fn $name(context: &mut Context, args: &[&Value]) -> Result<()> {
            let mut args = process_args(args)?.into_iter();
            let a = args.next().unwrap_or(None);
            let b = args.next().unwrap_or(None);
            if let (Some($a), Some($b)) = (a, b) {
                context.set_result($ret)
            } else {
                context.set_result(())
            }
        }
    };
}

// decimal_sum
// decimal collating sequence

scalar_method!(decimal_add as (a, b) => format!("{}", (a + b).normalized()));
scalar_method!(decimal_sub as (a, b) => format!("{}", (a - b).normalized()));
scalar_method!(decimal_mul as (a, b) => format!("{}", (a * b).normalized()));
scalar_method!(decimal_cmp as (a, b) => {
    match a.cmp(&b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
});

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    db.create_scalar_function("decimal_add", 2, 0, decimal_add)?;
    db.create_scalar_function("decimal_sub", 2, 0, decimal_sub)?;
    db.create_scalar_function("decimal_mul", 2, 0, decimal_mul)?;
    db.create_scalar_function("decimal_cmp", 2, 0, decimal_cmp)?;
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
}
