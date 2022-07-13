use super::*;
use indoc::indoc;
use lazy_static::lazy_static;
use pretty_assertions::assert_eq;
use regex::Regex;
use std::str::from_utf8;

fn setup() -> Result<(Database, Rc<RefCell<Vec<u8>>>)> {
    let conn = Database::open_in_memory()?;
    let out = Rc::new(RefCell::new(vec![]));
    init(&conn, out.clone())?;
    conn.execute(
        "CREATE VIRTUAL TABLE temp.log USING vtablog(schema='CREATE TABLE x(a,b,c)', rows=3)",
        (),
    )?;
    Ok((conn, out))
}

#[cfg(modern_sqlite)]
lazy_static! {
    static ref IGNORED_LINES: Regex = Regex::new("(?m)^<M.*?\n").unwrap();
    static ref INCLUDED_LINES: Regex = Regex::new("(?m)^=M (.*?\n)").unwrap();
}
#[cfg(not(modern_sqlite))]
lazy_static! {
    static ref IGNORED_LINES: Regex = Regex::new("(?m)^=M.*?\n").unwrap();
    static ref INCLUDED_LINES: Regex = Regex::new("(?m)^<M (.*?\n)").unwrap();
}

fn patch_output(input: String) -> String {
    let input = IGNORED_LINES.replace_all(&input, "");
    INCLUDED_LINES.replace_all(&input, "$1").to_string()
}

#[test]
fn read() -> Result<()> {
    let (conn, out) = setup()?;
    let ret: Vec<Vec<String>> = conn
        .prepare("SELECT * FROM log WHERE a IN ('a1', 'a2')")?
        .query(())?
        .map(|row| {
            Ok(vec![
                row.col(0).get_str()?.unwrap().to_owned(),
                row.col(1).get_str()?.unwrap().to_owned(),
                row.col(2).get_str()?.unwrap().to_owned(),
            ])
        })
        .collect()?;
    drop(conn);
    assert_eq!(
        ret,
        (1..3)
            .map(|i| vec![format!("a{}", i), format!("b{}", i), format!("c{}", i)])
            .collect::<Vec<Vec<String>>>()
    );
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = patch_output(indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        <M best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: 0, op: Eq, usable: true, argv_index: None, omit: false }], order_by: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98 })
        <M best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: 0, op: Eq, usable: false, argv_index: None, omit: false }], order_by: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98 })
        =M best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: 0, op: Eq, usable: true, rhs: Err(Sqlite(12, "unknown operation")), collation: Ok("BINARY"), argv_index: None, omit: false }], order_by: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98, estimated_rows: 25, scan_flags: 0, columns_used: 7 })
        =M best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: 0, op: Eq, usable: false, rhs: Err(Sqlite(12, "unknown operation")), collation: Ok("BINARY"), argv_index: None, omit: false }], order_by: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98, estimated_rows: 25, scan_flags: 0, columns_used: 7 })
        open(tab=100, cursor=101)
        filter(tab=100, cursor=101, args=[Text(Ok("a1"))])
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a0")
        next(tab=100, cursor=101)
          rowid 0 -> 1
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a1")
        column(tab=100, cursor=101, idx=0) -> Ok("a1")
        column(tab=100, cursor=101, idx=1) -> Ok("b1")
        column(tab=100, cursor=101, idx=2) -> Ok("c1")
        next(tab=100, cursor=101)
          rowid 1 -> 2
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a2")
        next(tab=100, cursor=101)
          rowid 2 -> 3
        eof(tab=100, cursor=101) -> true
        filter(tab=100, cursor=101, args=[Text(Ok("a2"))])
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a0")
        next(tab=100, cursor=101)
          rowid 0 -> 1
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a1")
        next(tab=100, cursor=101)
          rowid 1 -> 2
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a2")
        column(tab=100, cursor=101, idx=0) -> Ok("a2")
        column(tab=100, cursor=101, idx=1) -> Ok("b2")
        column(tab=100, cursor=101, idx=2) -> Ok("c2")
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
fn insert() -> Result<()> {
    let (conn, out) = setup()?;
    conn.execute("INSERT INTO log VALUES ( 1, 2, 3 ), (4, 5, 6)", ())?;
    drop(conn);
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        begin(tab=100, transaction=102)
        update(tab=100, args=ChangeInfo { change_type: Insert, rowid: Null, args: [Null, Integer(1), Integer(2), Integer(3)], conflict_mode: Abort })
        update(tab=100, args=ChangeInfo { change_type: Insert, rowid: Null, args: [Null, Integer(4), Integer(5), Integer(6)], conflict_mode: Abort })
        sync(tab=100, transaction=102)
        commit(tab=100, transaction=102)
        drop_transaction(tab=100, transaction=102)
        drop(tab=100)
    "#};
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn update() -> Result<()> {
    let (conn, out) = setup()?;
    conn.execute("UPDATE log SET a = b WHERE rowid = 1", ())?;
    drop(conn);
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = patch_output(indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        <M best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: -1, op: Eq, usable: true, argv_index: None, omit: false }], order_by: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98 })
        =M best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: -1, op: Eq, usable: true, rhs: Ok(Integer(1)), collation: Ok("BINARY"), argv_index: None, omit: false }], order_by: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98, estimated_rows: 25, scan_flags: 0, columns_used: 18446744073709551615 })
        begin(tab=100, transaction=102)
        open(tab=100, cursor=101)
        filter(tab=100, cursor=101, args=[Integer(1)])
        eof(tab=100, cursor=101) -> false
        rowid(tab=100, cursor=101) -> 0
        next(tab=100, cursor=101)
          rowid 0 -> 1
        eof(tab=100, cursor=101) -> false
        rowid(tab=100, cursor=101) -> 1
        column(tab=100, cursor=101, idx=1) -> Ok("b1")
        <M column(tab=100, cursor=101, idx=1) -> Ok("b1")
        <M column(tab=100, cursor=101, idx=2) -> Ok("c1")
        =M column(tab=100, cursor=101, idx=1) -> Err(NoChange)
        =M column(tab=100, cursor=101, idx=2) -> Err(NoChange)
        rowid(tab=100, cursor=101) -> 1
        rowid(tab=100, cursor=101) -> 1
        next(tab=100, cursor=101)
          rowid 1 -> 2
        eof(tab=100, cursor=101) -> false
        rowid(tab=100, cursor=101) -> 2
        next(tab=100, cursor=101)
          rowid 2 -> 3
        eof(tab=100, cursor=101) -> true
        <M update(tab=100, args=ChangeInfo { change_type: Update, rowid: Integer(1), args: [Integer(1), Text(Ok("b1")), Text(Ok("b1")), Text(Ok("c1"))], conflict_mode: Abort })
        =M update(tab=100, args=ChangeInfo { change_type: Update, rowid: Integer(1), args: [Integer(1), Text(Ok("b1")), Null, Null], conflict_mode: Abort })
        =M   unchanged: [2, 3]
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
fn delete() -> Result<()> {
    let (conn, out) = setup()?;
    conn.execute("DELETE FROM log WHERE a = 'a1'", ())?;
    drop(conn);
    let out = from_utf8(&out.borrow()).unwrap().to_owned();
    let expected = patch_output(indoc! {r#"
        create(tab=100, args=["vtablog", "temp", "log", "schema='CREATE TABLE x(a,b,c)'", "rows=3"])
        begin(tab=100, transaction=101)
        sync(tab=100, transaction=101)
        commit(tab=100, transaction=101)
        drop_transaction(tab=100, transaction=101)
        <M best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: 0, op: Eq, usable: true, argv_index: None, omit: false }], order_by: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98 })
        =M best_index(tab=100, index_info=IndexInfo { constraints: [IndexInfoConstraint { column: 0, op: Eq, usable: true, rhs: Ok(Text(Ok("a1"))), collation: Ok("BINARY"), argv_index: None, omit: false }], order_by: [], index_num: 0, index_str: None, order_by_consumed: false, estimated_cost: 5e98, estimated_rows: 25, scan_flags: 0, columns_used: 1 })
        begin(tab=100, transaction=102)
        open(tab=100, cursor=101)
        filter(tab=100, cursor=101, args=[Text(Ok("a1"))])
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a0")
        next(tab=100, cursor=101)
          rowid 0 -> 1
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a1")
        rowid(tab=100, cursor=101) -> 1
        next(tab=100, cursor=101)
          rowid 1 -> 2
        eof(tab=100, cursor=101) -> false
        column(tab=100, cursor=101, idx=0) -> Ok("a2")
        next(tab=100, cursor=101)
          rowid 2 -> 3
        eof(tab=100, cursor=101) -> true
        update(tab=100, args=ChangeInfo { change_type: Delete, rowid: Integer(1), args: [], conflict_mode: Abort })
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
fn rename() -> Result<()> {
    let (conn, out) = setup()?;
    conn.execute("ALTER TABLE log RENAME to newname", ())?;
    conn.execute("DROP TABLE newname", ())?;
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
#[cfg(modern_sqlite)]
fn shadow_name() -> Result<()> {
    let (conn, out) = setup()?;
    unsafe {
        rusqlite::Connection::from_handle(conn.as_mut_ptr())
            .unwrap()
            .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .unwrap();
    }
    match conn.execute("CREATE TABLE log_shadow (a, b, c)", ()) {
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
