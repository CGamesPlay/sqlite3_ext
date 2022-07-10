#![cfg(all(test, feature = "static"))]

use crate::test_helpers::prelude::*;

#[test]
fn basic() -> Result<()> {
    let h = TestHelpers::new();
    h.db.execute("CREATE TABLE tbl(a TEXT,b,c)", []).unwrap();
    h.db.execute("INSERT INTO tbl VALUES ('a1', 'b1', 'c1')", [])
        .unwrap();
    let conn = h.sqlite3_ext();
    let mut stmt = conn.prepare("SELECT a AS a_alias FROM tbl")?;
    let mut rows = stmt.query()?;
    let mut num_rows = 0;
    while let Some(mut r) = rows.next()? {
        assert_eq!(r.col(0).get_str()?, Some("a1"));
        assert_eq!(r.col(0).name()?, "a_alias");
        assert_eq!(r.col(0).database_name()?, Some("main"));
        assert_eq!(r.col(0).table_name()?, Some("tbl"));
        assert_eq!(r.col(0).origin_name()?, Some("a"));
        assert_eq!(r.col(0).decltype()?, Some("TEXT"));
        num_rows += 1;
    }
    assert_eq!(num_rows, 1);
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
