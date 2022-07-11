#![cfg(all(test, feature = "static"))]

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
    conn.execute("CREATE TABLE tbl(a TEXT,b,c)", ()).unwrap();
    conn.execute("INSERT INTO tbl VALUES ('a1', 'b1', 'c1')", ())
        .unwrap();
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
fn params() -> Result<()> {
    let h = TestHelpers::new();
    let conn = h.sqlite3_ext();
    let ret: Vec<Value> = conn
        .prepare("VALUES (?), (?), (?), (?), (?), (?)")?
        .query(params!(
            1,
            std::f64::consts::PI,
            "a string",
            "owned string".to_owned(),
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
            Value::Null,
            Value::Null,
        ]
    );
    Ok(())
}

#[test]
fn sql() -> Result<()> {
    let h = TestHelpers::new();
    let conn = h.sqlite3_ext();
    let stmt = conn.prepare("SELECT 1")?;
    assert_eq!(stmt.sql(), Ok("SELECT 1"));
    Ok(())
}
