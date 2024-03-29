#![cfg(all(test, feature = "static"))]

use crate::query::{Statement, ToParam};
use crate::test_helpers::prelude::*;

#[test]
fn basic() -> Result<()> {
    #[derive(Debug, PartialEq)]
    struct Row {
        value: String,
        name: String,
        database_name: Option<String>,
        table_name: Option<String>,
        origin_name: Option<String>,
        decltype: Option<String>,
    }
    let h = TestHelpers::new();
    h.db.execute("CREATE TABLE tbl(a TEXT,b,c)", ())?;
    h.db.execute("INSERT INTO tbl VALUES ('a1', 'b1', 'c1')", ())?;
    let ret: Vec<Row> =
        h.db.prepare("SELECT a AS a_alias FROM tbl")?
            .query(())?
            .map(|r| {
                Ok(Row {
                    value: r[0].get_str()?.to_owned(),
                    name: r[0].name()?.to_owned(),
                    database_name: r[0].database_name()?.map(String::from),
                    table_name: r[0].table_name()?.map(String::from),
                    origin_name: r[0].origin_name()?.map(String::from),
                    decltype: r[0].decltype()?.map(String::from),
                })
            })
            .collect()?;
    assert_eq!(
        ret,
        vec![Row {
            value: "a1".to_owned(),
            name: "a_alias".to_owned(),
            database_name: Some("main".to_owned()),
            table_name: Some("tbl".to_owned()),
            origin_name: Some("a".to_owned()),
            decltype: Some("TEXT".to_owned()),
        }]
    );
    Ok(())
}

#[test]
fn empty_statement() {
    let h = TestHelpers::new();
    let err = h.db.prepare("").unwrap_err();
    assert_eq!(err, SQLITE_MISUSE);
}

#[test]
fn invalid_execute() {
    let h = TestHelpers::new();
    let err = h.db.execute("SELECT 1", ());
    assert_eq!(err, Err(SQLITE_MISUSE));
}

#[test]
fn params() -> Result<()> {
    let h = TestHelpers::new();
    let mut stmt = h.db.prepare("VALUES (?), (?), (?), (?), (?), (?), (?)")?;
    assert_eq!(stmt.parameter_count(), 7);
    assert_eq!(stmt.sql(), Ok("VALUES (?), (?), (?), (?), (?), (?), (?)"));

    let ret: Vec<Value> = stmt
        .query(params!(
            1,
            std::f64::consts::PI,
            "a string",
            "owned string".to_owned(),
            [254, 253, 252],
            None as Option<i64>,
            (),
        ))?
        .map(|r| r[0].to_owned())
        .collect()?;
    assert_eq!(
        ret,
        vec![
            Value::Integer(1),
            Value::Float(std::f64::consts::PI),
            Value::Text("a string".to_owned()),
            Value::Text("owned string".to_owned()),
            Value::Blob(Blob::from([254, 253, 252])),
            Value::Null,
            Value::Null,
        ]
    );
    Ok(())
}

#[test]
fn value_params() -> Result<()> {
    let h = TestHelpers::new();
    let ret: Vec<Value> =
        h.db.prepare("VALUES (?), (?), (?), (?), (?)")?
            .query([
                Value::Integer(1),
                Value::Float(std::f64::consts::PI),
                Value::Text("owned string".to_owned()),
                Value::Blob(Blob::from([255, 254, 253])),
                Value::Null,
            ])?
            .map(|r| r[0].to_owned())
            .collect()?;
    assert_eq!(
        ret,
        vec![
            Value::Integer(1),
            Value::Float(std::f64::consts::PI),
            Value::Text("owned string".to_owned()),
            Value::Blob(Blob::from([255, 254, 253])),
            Value::Null,
        ]
    );
    Ok(())
}

#[test]
fn func_params() -> Result<()> {
    let h = TestHelpers::new();
    let ret: Vec<i32> =
        h.db.prepare("VALUES (?), (?), (?)")?
            .query(|stmt: &mut Statement| {
                for x in 1..=3i64 {
                    x.bind_param(stmt, x as _)?;
                }
                Ok(())
            })?
            .map(|r| Ok(r[0].get_i32()))
            .collect()?;
    assert_eq!(ret, vec![1, 2, 3]);
    Ok(())
}

#[test]
fn named_params() -> Result<()> {
    let h = TestHelpers::new();
    let mut stmt =
        h.db.prepare("VALUES (:first_value), (?), (:second_value), (?)")?;

    let mut param_names = Vec::with_capacity(stmt.parameter_count() as _);
    for i in 1..=stmt.parameter_count() {
        param_names.push(stmt.parameter_name(i));
    }
    assert_eq!(
        param_names,
        vec!(Some(":first_value"), None, Some(":second_value"), None)
    );

    let ret: Vec<i32> = stmt
        .query(params!((":second_value", 1), 2, (":first_value", 3), 4))?
        .map(|r| Ok(r[0].get_i32()))
        .collect()?;
    assert_eq!(ret, vec![3, 2, 1, 4]);
    Ok(())
}

#[test]
#[cfg(modern_sqlite)]
fn passed_ref() -> Result<()> {
    #[derive(PartialEq, Debug)]
    struct MyStruct {
        s: String,
    }

    let h = TestHelpers::new();

    h.db.create_scalar_function(
        "extract",
        &FunctionOptions::default().set_n_args(1),
        |c, args| c.set_result(args[0].get_ref::<MyStruct>().unwrap().s.to_owned()),
    )?;
    let s = MyStruct {
        s: "string from passed ref".to_owned(),
    };
    let ret: String =
        h.db.query_row("SELECT extract(?)", params!(PassedRef::new(s)), |r| {
            Ok(r[0].get_str()?.to_owned())
        })?;
    assert_eq!(ret, "string from passed ref".to_owned());
    Ok(())
}

#[test]
fn unprotected_value() -> Result<()> {
    let h = TestHelpers::new();
    let mut stmt = h.db.prepare("SELECT zeroblob(1024)")?;
    let ret = stmt.next()?.map(|r| r[0].as_ref());
    let ret: i64 =
        h.db.query_row("SELECT length(?)", [ret], |r| Ok(r[0].get_i64()))?;
    assert_eq!(ret, 1024);
    Ok(())
}

#[test]
fn reuse_statement() -> Result<()> {
    let h = TestHelpers::new();
    let mut stmt = h.db.prepare("SELECT ?")?;

    let ret = stmt.query_row([1], |r| Ok(r[0].get_i32()))?;
    assert_eq!(ret, 1);
    let ret = stmt.query_row([2], |r| Ok(r[0].get_i32()))?;
    assert_eq!(ret, 2);
    // Ensure that bindings were cleared out
    let ret = stmt.query_row((), |r| Ok(r[0].to_owned()?))?;
    assert_eq!(ret, Value::Null);
    Ok(())
}
