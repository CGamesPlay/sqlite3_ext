use sqlite3_ext::{iterator::*, query::*, *};

fn column_borrow(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("SELECT 1, 2, 3")?;
    let results: Vec<Column<'_>> = stmt.query(())?.map(|r| Ok(r.col(0))).collect()?;
    assert_eq!(results.len(), 1);
    Ok(())
}

fn main() {}
