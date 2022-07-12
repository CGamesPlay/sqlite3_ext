#![cfg(all(test, feature = "static"))]

use crate::query::{Statement, ToParam};
use crate::test_helpers::prelude::*;

#[test]
fn basic() -> Result<()> {
    #[derive(Debug, PartialEq)]
    struct Row {
        value: Option<String>,
        name: String,
        database_name: Option<String>,
        table_name: Option<String>,
        origin_name: Option<String>,
        decltype: Option<String>,
    }
    let h = TestHelpers::new();
    let conn = h.sqlite3_ext();
    conn.execute("CREATE TABLE tbl(a TEXT,b,c)", ())?;
    conn.execute("INSERT INTO tbl VALUES ('a1', 'b1', 'c1')", ())?;
    let ret: Vec<Row> = conn
        .prepare("SELECT a AS a_alias FROM tbl")?
        .query(())?
        .map(|r| {
            Ok(Row {
                value: r.col(0).get_str()?.map(String::from),
                name: r.col(0).name()?.to_owned(),
                database_name: r.col(0).database_name()?.map(String::from),
                table_name: r.col(0).table_name()?.map(String::from),
                origin_name: r.col(0).origin_name()?.map(String::from),
                decltype: r.col(0).decltype()?.map(String::from),
            })
        })
        .collect()?;
    assert_eq!(
        ret,
        vec![Row {
            value: Some("a1".to_owned()),
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
fn invalid_execute() {
    let h = TestHelpers::new();
    let conn = h.sqlite3_ext();
    let err = conn.execute("SELECT 1", ());
    assert_eq!(err, Err(SQLITE_MISUSE));
}

#[test]
fn params() -> Result<()> {
    let h = TestHelpers::new();
    let conn = h.sqlite3_ext();
    let mut stmt = conn.prepare("VALUES (?), (?), (?), (?), (?), (?), (?)")?;
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
        .map(|r| r.col(0).to_owned())
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
    let conn = h.sqlite3_ext();
    let ret: Vec<Value> = conn
        .prepare("VALUES (?), (?), (?), (?), (?)")?
        .query([
            Value::Integer(1),
            Value::Float(std::f64::consts::PI),
            Value::Text("owned string".to_owned()),
            Value::Blob(Blob::from([255, 254, 253])),
            Value::Null,
        ])?
        .map(|r| r.col(0).to_owned())
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
    let conn = h.sqlite3_ext();
    let ret: Vec<i32> = conn
        .prepare("VALUES (?), (?), (?)")?
        .query(|stmt: &mut Statement| {
            for x in 1..=3i64 {
                x.bind_param(stmt, x as _)?;
            }
            Ok(())
        })?
        .map(|r| Ok(r.col(0).get_i32()))
        .collect()?;
    assert_eq!(ret, vec![1, 2, 3]);
    Ok(())
}

#[test]
fn named_params() -> Result<()> {
    let h = TestHelpers::new();
    let conn = h.sqlite3_ext();
    let mut stmt = conn.prepare("VALUES (:first_value), (?), (:second_value), (?)")?;

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
        .map(|r| Ok(r.col(0).get_i32()))
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
    let conn = h.sqlite3_ext();

    conn.create_scalar_function(
        "extract",
        &FunctionOptions::default().set_n_args(1),
        |_, args| args[0].get_ref::<MyStruct>().unwrap().s.to_owned(),
    )?;
    let s = MyStruct {
        s: "string from passed ref".to_owned(),
    };
    let ret: String = conn.query_row("SELECT extract(?)", params!(PassedRef::new(s)), |r| {
        Ok(r.col(0).get_str()?.unwrap().to_owned())
    })?;
    assert_eq!(ret, "string from passed ref".to_owned());
    Ok(())
}

#[test]
fn reuse_statement() -> Result<()> {
    let h = TestHelpers::new();
    let conn = h.sqlite3_ext();
    let mut stmt = conn.prepare("SELECT ?")?;

    let ret = stmt.query_row([1], |r| Ok(r.col(0).get_i32()))?;
    assert_eq!(ret, 1);
    let ret = stmt.query_row([2], |r| Ok(r.col(0).get_i32()))?;
    assert_eq!(ret, 2);
    // Ensure that bindings were cleared out
    let ret = stmt.query_row((), |r| Ok(r.col(0).to_owned()?))?;
    assert_eq!(ret, Value::Null);
    Ok(())
}
