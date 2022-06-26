use sqlite3_ext::{function::*, *};

fn user_data(context: &Context<&'static str>, _: &[&ValueRef]) -> &'static str {
    context.user_data()
}

// Returns the number of times that the first argument has been passed to the function.
fn aux_data(context: &Context<()>, _: &[&ValueRef]) -> i64 {
    match context.aux_data::<i64>(0) {
        Some(x) => {
            *x += 1;
            *x
        }
        None => {
            context.set_aux_data(0, 1i64);
            1
        }
    }
}

#[derive(Default)]
struct Agg {
    acc: Vec<String>,
}

impl AggregateFunction for Agg {
    type UserData = &'static str;
    type Output = String;

    fn step(&mut self, _: &Context<Self::UserData>, args: &[&ValueRef]) -> Result<()> {
        self.acc.push(args[0].get_str()?.unwrap_or("").to_owned());
        Ok(())
    }

    fn value(&self, context: &Context<Self::UserData>) -> Self::Output {
        self.acc.join(context.user_data())
    }

    fn inverse(&mut self, _: &Context<Self::UserData>, _: &[&ValueRef]) -> Result<()> {
        self.acc.remove(0);
        Ok(())
    }
}

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    let opts = FunctionOptions::default()
        .set_risk_level(RiskLevel::Innocuous)
        .set_deterministic(true)
        .set_n_args(0);
    db.create_scalar_function("user_data_foo", &opts, user_data, "foo")?;
    db.create_scalar_function("user_data_bar", &opts, user_data, "bar")?;
    let opts = opts.set_n_args(2);
    db.create_scalar_function("aux_data", &opts, aux_data, ())?;
    let opts = opts.set_n_args(1);
    db.create_aggregate_function::<Agg>("join_str", &opts, "|")?;
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
    fn user_data() -> rusqlite::Result<()> {
        case(vec![
            ("user_data_foo()", Some("foo".to_owned())),
            ("user_data_bar()", Some("bar".to_owned())),
        ])?;
        case(vec![(
            "join_str(column1) FROM ( VALUES ('a'), ('1'), (NULL) )",
            Some("a|1|".to_owned()),
        )])?;
        Ok(())
    }

    #[test]
    fn aux_data() -> rusqlite::Result<()> {
        let conn = setup()?;
        let ret: Vec<i64> = conn
            .prepare("SELECT aux_data('foo', column1) FROM ( VALUES ((1)), (('a')), ((NULL)) )")?
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<_>>()?;
        assert_eq!(ret, vec![1, 2, 3]);
        Ok(())
    }
}
