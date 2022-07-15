use bigdecimal::BigDecimal;
use sqlite3_ext::{function::*, *};
use std::{cmp::Ordering, str::FromStr};

// NULL maps to None
// Valid BigDecimal maps to Some(x)
// Otherwise Some(0)
fn process_value(a: &mut ValueRef) -> Result<Option<BigDecimal>> {
    Ok(a.get_str()?
        .map(|a| BigDecimal::from_str(a).unwrap_or_else(|_| BigDecimal::default())))
}

fn process_args(args: &mut [&mut ValueRef]) -> Result<Vec<Option<BigDecimal>>> {
    args.into_iter().map(|x| process_value(*x)).collect()
}

macro_rules! scalar_method {
    ($name:ident as ( $a:ident, $b:ident ) -> $ty:ty => $ret:expr) => {
        fn $name(_: &Context, args: &mut [&mut ValueRef]) -> Result<Option<$ty>> {
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

#[derive(Default)]
struct Sum {
    cur: BigDecimal,
}

impl AggregateFunction<()> for Sum {
    type Output = Option<String>;

    fn default_value(_: &(), _: &Context) -> Self::Output {
        None
    }

    fn step(&mut self, _: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
        if let Some(x) = process_value(*args.first_mut().unwrap())? {
            self.cur += x;
        }
        Ok(())
    }

    fn value(&self, _: &Context) -> Self::Output {
        Some(format!("{}", self.cur.normalized()))
    }

    fn inverse(&mut self, _: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
        if let Some(x) = process_value(*args.first_mut().unwrap())? {
            self.cur -= x;
        }
        Ok(())
    }
}

fn decimal_collation(a: &str, b: &str) -> Ordering {
    let a = BigDecimal::from_str(a).unwrap_or_else(|_| BigDecimal::default());
    let b = BigDecimal::from_str(b).unwrap_or_else(|_| BigDecimal::default());
    a.cmp(&b)
}

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    let opts = FunctionOptions::default()
        .set_risk_level(RiskLevel::Innocuous)
        .set_deterministic(true)
        .set_n_args(2);
    db.create_scalar_function("decimal_add", &opts, decimal_add)?;
    db.create_scalar_function("decimal_sub", &opts, decimal_sub)?;
    db.create_scalar_function("decimal_mul", &opts, decimal_mul)?;
    db.create_scalar_function("decimal_cmp", &opts, decimal_cmp)?;
    let opts = opts.set_n_args(1);
    db.create_aggregate_function::<_, Sum>("decimal_sum", &opts, ())?;
    db.create_collation("decimal", decimal_collation)?;
    Ok(())
}

#[cfg(all(test, feature = "static"))]
mod test {
    use super::*;

    fn setup() -> Result<Database> {
        let conn = Database::open_in_memory()?;
        init(&conn)?;
        Ok(conn)
    }

    fn case(data: Vec<(&str, Value)>) -> Result<()> {
        let conn = setup()?;
        let (sql, expected): (Vec<&str>, Vec<Value>) = data.into_iter().unzip();
        let sql = format!("SELECT {}", sql.join(", "));
        println!("{}", sql);
        let ret: Vec<Value> = conn.query_row(&sql, (), |r| {
            (0..expected.len())
                .map(|i| r[i].to_owned())
                .collect::<Result<_>>()
        })?;
        assert_eq!(ret, expected);
        Ok(())
    }

    #[test]
    fn decimal_add() -> Result<()> {
        case(vec![
            (
                "decimal_add('1000000000000000', '0.0000000000000001')",
                Value::Text("1000000000000000.0000000000000001".to_owned()),
            ),
            ("decimal_add(NULL, '0')", Value::Null),
            ("decimal_add('0', NULL)", Value::Null),
            ("decimal_add(NULL, NULL)", Value::Null),
            ("decimal_add('invalid', 2)", Value::Text("2".to_owned())),
        ])
    }

    #[test]
    fn decimal_sub() -> Result<()> {
        case(vec![
            (
                "decimal_sub('1000000000000000', '0.0000000000000001')",
                Value::Text("999999999999999.9999999999999999".to_owned()),
            ),
            ("decimal_sub(NULL, '0')", Value::Null),
            ("decimal_sub('0', NULL)", Value::Null),
            ("decimal_sub(NULL, NULL)", Value::Null),
            ("decimal_sub('invalid', 2)", Value::Text("-2".to_owned())),
        ])
    }

    #[test]
    fn decimal_mul() -> Result<()> {
        case(vec![
            (
                "decimal_mul('1000000000000000', '0.0000000000000001')",
                Value::Text("0.1".to_owned()),
            ),
            ("decimal_mul(NULL, '0')", Value::Null),
            ("decimal_mul('0', NULL)", Value::Null),
            ("decimal_mul(NULL, NULL)", Value::Null),
            ("decimal_mul('invalid', 2)", Value::Text("0".to_owned())),
        ])
    }

    #[test]
    fn decimal_cmp() -> Result<()> {
        case(vec![
            ("decimal_cmp('1', '-1')", Value::Integer(1)),
            ("decimal_cmp('-1', '1')", Value::Integer(-1)),
            ("decimal_cmp('1', '1')", Value::Integer(0)),
            ("decimal_cmp(NULL, '0')", Value::Null),
            ("decimal_cmp('0', NULL)", Value::Null),
            ("decimal_cmp(NULL, NULL)", Value::Null),
        ])
    }

    fn aggregate_case(expr: &str, data: Vec<&str>, expected: Vec<Value>) -> Result<()> {
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
        let ret: Vec<Value> = conn
            .prepare(&sql)?
            .query(())?
            .map(|r| r[0].to_owned())
            .collect()?;
        assert_eq!(ret, expected);
        Ok(())
    }

    #[test]
    fn decimal_sum() -> Result<()> {
        aggregate_case(
            "decimal_sum(column1)",
            vec!["1000000000000000", "0.0000000000000001", "1"],
            vec![Value::Text("1000000000000001.0000000000000001".to_owned())],
        )?;
        aggregate_case(
            "decimal_sum(column1)",
            vec!["1", "NULL"],
            vec![Value::Text("1".to_owned())],
        )?;
        aggregate_case(
            "decimal_sum(column1)",
            vec!["NULL"],
            vec![Value::Text("0".to_owned())],
        )?;
        case(vec![("decimal_sum(NULL)", Value::Text("0".to_owned()))])?;
        case(vec![(
            "decimal_sum('invalid')",
            Value::Text("0".to_owned()),
        )])?;
        case(vec![("decimal_sum(1) WHERE 1 = 0", Value::Null)])?;
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
                Value::Text("1000000000000000".to_owned()),
                Value::Text("1000000000000000.0000000000000001".to_owned()),
                Value::Text("0.0000000000000001".to_owned()),
                Value::Text("0".to_owned()),
                Value::Text("1".to_owned()),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn collation() -> Result<()> {
        let conn = setup()?;
        let ret: Vec<String> = conn
            .prepare(
                "SELECT column1 FROM ( VALUES (('1')), (('0100')), (('.1')) ) ORDER BY column1 COLLATE decimal",
            )?
            .query(())?.map(|row| Ok(row[0].get_str()?.unwrap().to_owned()))
            .collect()?;
        assert_eq!(
            ret,
            vec![".1".to_owned(), "1".to_owned(), "0100".to_owned()]
        );
        Ok(())
    }
}
