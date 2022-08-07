#[sqlite3_ext::sqlite3_ext_init]
fn init(conn: &sqlite3_ext::Connection) -> sqlite3_ext::Result<()> {
    let opts = sqlite3_ext::function::FunctionOptions::default()
        .set_deterministic(true)
        .set_risk_level(sqlite3_ext::RiskLevel::Innocuous)
        .set_n_args(0);
    conn.create_scalar_function("user_function", &opts, |c, _| {
        c.set_result("user defined function")
    })?;
    Ok(())
}

#[test]
fn main() -> rusqlite::Result<()> {
    let conn = rusqlite::Connection::open(":memory:")?;
    init(sqlite3_ext::Connection::from_rusqlite(&conn))?;
    let ret = conn.query_row("SELECT user_function()", [], |r| r.get::<_, String>(0))?;
    assert_eq!(ret, "user defined function".to_owned());
    Ok(())
}
