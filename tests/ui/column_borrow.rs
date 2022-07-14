use sqlite3_ext::*;

fn column_borrow(conn: &Connection) -> Result<()> {
    let results: Vec<bool> = conn
        .prepare("SELECT 1, 2, 3")?
        .query(())?
        .map(|r| {
            let mut col1 = r.col(0);
            let mut col2 = r.col(1);
            assert_ne!(col1.get_str()?, col2.get_str()?);
            Ok(true)
        })
        .collect()?;
    assert_eq!(results, vec![true]);
    Ok(())
}

fn main() {}
