use rusqlite::*;

#[test]
fn example() -> Result<()> {
    crdb::auto_register();
    let conn = Connection::open_in_memory()?;
    Ok(())
}
