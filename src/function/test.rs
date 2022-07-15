#![cfg(all(test, feature = "static"))]
use crate::test_helpers::prelude::*;

struct Agg {
    sep: &'static str,
    acc: Vec<String>,
}

impl FromUserData<&'static str> for Agg {
    fn from_user_data(val: &&'static str) -> Self {
        Agg {
            sep: *val,
            acc: vec![],
        }
    }
}

impl AggregateFunction<&'static str> for Agg {
    type Output = String;

    fn step(&mut self, _: &Context, args: &mut [&mut ValueRef]) -> Result<()> {
        let a: &mut ValueRef = args[0];
        self.acc.push((a).get_str()?.unwrap_or("").to_owned());
        Ok(())
    }

    fn value(&self, _: &Context) -> Self::Output {
        self.acc.join(self.sep)
    }

    fn inverse(&mut self, _: &Context, _: &mut [&mut ValueRef]) -> Result<()> {
        self.acc.remove(0);
        Ok(())
    }
}

#[test]
fn user_data_scalar() -> Result<()> {
    let h = TestHelpers::new();
    let opts = FunctionOptions::default()
        .set_deterministic(true)
        .set_risk_level(RiskLevel::Innocuous)
        .set_n_args(0);
    let user_data = "foo";
    h.db.create_scalar_function("user_data", &opts, move |_, _| user_data)?;

    let ret =
        h.db.query_row("SELECT user_data()", (), |r| r[0].to_owned())?;
    assert_eq!(ret, Value::Text("foo".to_owned()));

    Ok(())
}

#[test]
fn user_data_aggregate() -> Result<()> {
    let h = TestHelpers::new();
    let opts = FunctionOptions::default()
        .set_deterministic(true)
        .set_risk_level(RiskLevel::Innocuous)
        .set_n_args(1);
    h.db.create_aggregate_function::<_, Agg>("join_str", &opts, "|")?;

    let ret = h.db.query_row(
        "SELECT join_str(column1) FROM ( VALUES ('a'), ('1'), (NULL) )",
        (),
        |r| r[0].to_owned(),
    )?;
    assert_eq!(ret, Value::Text("a|1|".to_owned()));

    Ok(())
}

#[test]
fn aux_data() -> Result<()> {
    let h = TestHelpers::new();
    let opts = FunctionOptions::default()
        .set_deterministic(true)
        .set_risk_level(RiskLevel::Innocuous)
        .set_n_args(2);
    // Returns the number of times that the first argument has been passed to the function.
    h.db.create_scalar_function("aux_data", &opts, |context, _| -> i64 {
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
    })?;

    let ret: Vec<i64> =
        h.db.prepare("SELECT aux_data('foo', column1) FROM ( VALUES ((1)), (('a')), ((NULL)) )")?
            .query(())?
            .map(|row| Ok(row[0].get_i64()))
            .collect()?;
    assert_eq!(ret, vec![1, 2, 3]);
    Ok(())
}

#[test]
fn collation() -> Result<()> {
    let h = TestHelpers::new();
    h.db.set_collation_needed_func(|name| {
        if name == "rot13" {
            let _ = h.db.create_collation(name, |a, b| {
                fn rot13(c: char) -> char {
                    match c {
                        'A'..='M' | 'a'..='m' => ((c as u8) + 13) as char,
                        'N'..='Z' | 'n'..='z' => ((c as u8) - 13) as char,
                        _ => c,
                    }
                }
                a.chars().map(rot13).cmp(b.chars().map(rot13))
            });
        }
    })?;

    let sql = "SELECT column1 FROM ( VALUES (('A')), (('N')), (('M')), (('Z')) ) ORDER BY column1 COLLATE rot13";
    let ret: Vec<String> =
        h.db.prepare(sql)?
            .query(())?
            .map(|row| Ok(row[0].get_str()?.unwrap().to_owned()))
            .collect()?;
    assert_eq!(
        ret,
        vec![
            "N".to_owned(),
            "Z".to_owned(),
            "A".to_owned(),
            "M".to_owned()
        ]
    );
    Ok(())
}
