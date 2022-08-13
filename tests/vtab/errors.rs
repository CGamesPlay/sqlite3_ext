use crate::test_vtab::*;
use sqlite3_ext::{vtab::*, *};

#[test]
fn errors() -> Result<()> {
    struct Hooks;

    impl TestHooks for Hooks {
        fn best_index<'a>(&'a self, _: &TestVTab<'a, Self>, _: &mut IndexInfo) -> Result<()> {
            Err(Error::Sqlite(ffi::SQLITE_ERROR, Some("".to_string())))
        }
    }

    let hooks = Hooks;
    let conn = setup(&hooks)?;
    let err = conn
        .query_row("SELECT a FROM tbl", (), |_| Ok(()))
        .unwrap_err();
    assert_eq!(err.to_string(), "SQL logic error".to_string());
    Ok(())
}
