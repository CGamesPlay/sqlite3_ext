use sqlite3_ext::*;

#[test]
fn main() -> Result<()> {
    let dylib_path = test_cdylib::build_example("generate_series");
    let conn = Database::open_in_memory()?;
    conn.load_extension(&dylib_path.to_string_lossy(), None)?;
    let results: Vec<i64> = conn
        .prepare("SELECT value FROM generate_series(5, 100, 5)")?
        .query(())?
        .map(|row| Ok(row[0].get_i64()))
        .collect()?;
    assert_eq!(
        results,
        vec![5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95, 100]
    );
    Ok(())
}
