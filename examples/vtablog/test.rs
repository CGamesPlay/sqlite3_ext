use super::*;
use indoc::indoc;
use lazy_static::lazy_static;
use pretty_assertions::assert_eq;
use regex::Regex;
use rusqlite;
use std::str::from_utf8;

fn setup() -> rusqlite::Result<(rusqlite::Connection, Rc<RefCell<Vec<u8>>>)> {
    let conn = rusqlite::Connection::open_in_memory()?;
    let out = Rc::new(RefCell::new(vec![]));
    init(Connection::from_rusqlite(&conn), out.clone())?;
    conn.execute(
        "CREATE VIRTUAL TABLE temp.log USING vtablog(schema='CREATE TABLE x(a,b,c)', rows=3)",
        [],
    )?;
    Ok((conn, out))
}

fn patch_best_index(mut input: String) -> String {
    lazy_static! {
        static ref ESTIMATED_ROWS: Regex = Regex::new("estimated_rows: Ok\\([^)]+\\)").unwrap();
        static ref SCAN_FLAGS: Regex = Regex::new("scan_flags: Ok\\([^)]+\\)").unwrap();
        static ref COLUMNS_USED: Regex = Regex::new("columns_used: Ok\\([^)]+\\)").unwrap();
    }
    input = sqlite3_require_version!(3_008_002, input, {
        ESTIMATED_ROWS
            .replace_all(&input, "estimated_rows: Err(VersionNotSatisfied(3008002))")
            .to_string()
    });
    input = sqlite3_require_version!(3_009_000, input, {
        SCAN_FLAGS
            .replace_all(&input, "scan_flags: Err(VersionNotSatisfied(3009000))")
            .to_string()
    });
    input = sqlite3_require_version!(3_010_000, input, {
        COLUMNS_USED
            .replace(&input, "columns_used: Err(VersionNotSatisfied(3010000))")
            .to_string()
    });
    input
}

#[test]
fn read() -> rusqlite::Result<()> {
    let (conn, out) = setup()?;
    let ret = conn
        .prepare("SELECT * FROM log")?
        .query_map([], |row| {
            Ok(vec![
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ])
        })?
        .into_iter()
        .collect::<rusqlite::Result<Vec<Vec<String>>>>()?;
    drop(conn);
    assert_eq!(
        ret,
        (0..3)
            .map(|i| vec![format!("a{}", i), format!("b{}", i), format!("c{}", i)])
            .collect::<Vec<Vec<String>>>()
    );
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = patch_best_index(indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        best_index(tab=100, index_info=IndexInfo { constraints: [], order_by: [], constraint_usage: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98, estimated_rows: Ok(25), scan_flags: Ok(0), columns_used: Ok(7) })
        open(tab=100, cursor=101)
        filter(tab=100, cursor=101, args=[])
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> "a0"
        column(tab=100, cursor=101, idx=1) -> "b0"
        column(tab=100, cursor=101, idx=2) -> "c0"
        next(tab=100, cursor=101)
          rowid 0 -> 1
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> "a1"
        column(tab=100, cursor=101, idx=1) -> "b1"
        column(tab=100, cursor=101, idx=2) -> "c1"
        next(tab=100, cursor=101)
          rowid 1 -> 2
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> "a2"
        column(tab=100, cursor=101, idx=1) -> "b2"
        column(tab=100, cursor=101, idx=2) -> "c2"
        next(tab=100, cursor=101)
          rowid 2 -> 3
        eof(tab=100, cursor=101) -> true
        drop(tab=100, cursor=101)
        drop(tab=100)
    "#}.to_owned());
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn insert() -> rusqlite::Result<()> {
    let (conn, out) = setup()?;
    conn.execute("INSERT INTO log VALUES ( 1, 2, 3 ), (4, 5, 6)", [])?;
    drop(conn);
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        begin(tab=100, transaction=102)
        insert(tab=100, args=[ValueRef::Null, ValueRef::Integer(1), ValueRef::Integer(2), ValueRef::Integer(3)])
        insert(tab=100, args=[ValueRef::Null, ValueRef::Integer(4), ValueRef::Integer(5), ValueRef::Integer(6)])
        sync(tab=100, transaction=102)
        commit(tab=100, transaction=102)
        drop_transaction(tab=100, transaction=102)
        drop(tab=100)
    "#};
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn update() -> rusqlite::Result<()> {
    let (conn, out) = setup()?;
    conn.execute("UPDATE log SET a = b WHERE rowid = 1", [])?;
    drop(conn);
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = patch_best_index(indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: -1, op: Eq, usable: true }], order_by: [], constraint_usage: [IndexInfoConstraintUsage { argv_index: 0, omit: false }], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98, estimated_rows: Ok(25), scan_flags: Ok(0), columns_used: Ok(18446744073709551615) })
        begin(tab=100, transaction=102)
        open(tab=100, cursor=101)
        filter(tab=100, cursor=101, args=[])
        eof(tab=100, cursor=101) -> false
        rowid(tab=100, cursor=101) -> 0
        next(tab=100, cursor=101)
          rowid 0 -> 1
        eof(tab=100, cursor=101) -> false
        rowid(tab=100, cursor=101) -> 1
        column(tab=100, cursor=101, idx=1) -> "b1"
        column(tab=100, cursor=101, idx=1) -> "b1"
        column(tab=100, cursor=101, idx=2) -> "c1"
        rowid(tab=100, cursor=101) -> 1
        rowid(tab=100, cursor=101) -> 1
        next(tab=100, cursor=101)
          rowid 1 -> 2
        eof(tab=100, cursor=101) -> false
        rowid(tab=100, cursor=101) -> 2
        next(tab=100, cursor=101)
          rowid 2 -> 3
        eof(tab=100, cursor=101) -> true
        update(tab=100, rowid=ValueRef::Integer(1), args=[ValueRef::Integer(1), ValueRef::Text(Ok("b1")), ValueRef::Text(Ok("b1")), ValueRef::Text(Ok("c1"))]
        drop(tab=100, cursor=101)
        sync(tab=100, transaction=102)
        commit(tab=100, transaction=102)
        drop_transaction(tab=100, transaction=102)
        drop(tab=100)
    "#}.to_owned());
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn delete() -> rusqlite::Result<()> {
    let (conn, out) = setup()?;
    conn.execute("DELETE FROM log WHERE a = 'a1'", [])?;
    drop(conn);
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = patch_best_index(indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: 0, op: Eq, usable: true }], order_by: [], constraint_usage: [IndexInfoConstraintUsage { argv_index: 0, omit: false }], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98, estimated_rows: Ok(25), scan_flags: Ok(0), columns_used: Ok(1) })
        begin(tab=100, transaction=102)
        open(tab=100, cursor=101)
        filter(tab=100, cursor=101, args=[])
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> "a0"
        next(tab=100, cursor=101)
          rowid 0 -> 1
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> "a1"
        rowid(tab=100, cursor=101) -> 1
        next(tab=100, cursor=101)
          rowid 1 -> 2
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> "a2"
        next(tab=100, cursor=101)
          rowid 2 -> 3
        eof(tab=100, cursor=101) -> true
        delete(tab=100, rowid=ValueRef::Integer(1))
        drop(tab=100, cursor=101)
        sync(tab=100, transaction=102)
        commit(tab=100, transaction=102)
        drop_transaction(tab=100, transaction=102)
        drop(tab=100)
    "#}.to_owned());
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn rename() -> rusqlite::Result<()> {
    let (conn, out) = setup()?;
    conn.execute("ALTER TABLE log RENAME to newname", [])?;
    conn.execute("DROP TABLE newname", [])?;
    drop(conn);
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        rename(tab=100, name="newname")
        drop(tab=100)
        connect(tab=200, args=["vtablog", "temp", "newname", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        destroy(tab=200)
        drop(tab=200)
    "#};
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn shadow_name() -> rusqlite::Result<()> {
    sqlite3_require_version!(3_026_000, {}, {
        return Ok(());
    });
    let (conn, out) = setup()?;
    conn.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    match conn.execute("CREATE TABLE log_shadow (a, b, c)", []) {
        Err(_) => (),
        _ => panic!("expected error, got ok"),
    }
    drop(conn);
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        drop(tab=100)
    "#};
    assert_eq!(out, expected);
    Ok(())
}
