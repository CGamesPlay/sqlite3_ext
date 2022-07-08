use crate::test_vtab::*;
use sqlite3_ext::{vtab::*, *};

#[test]
fn best_index() -> rusqlite::Result<()> {
    #[derive(Default)]
    struct Hooks;

    impl TestHooks for Hooks {
        fn best_index<'a>(
            &'a self,
            _vtab: &TestVTab<'a, Self>,
            index_info: &mut IndexInfo,
        ) -> Result<()> {
            assert_eq!(index_info.distinct_mode(), DistinctMode::Ordered);
            match index_info.constraints().next() {
                Some(_c) => {
                    #[cfg(modern_sqlite)]
                    assert_eq!(_c.rhs()?.get_i64(), 20)
                }
                None => panic!("no constraint"),
            }
            Ok(())
        }
    }

    let hooks = Hooks::default();
    let conn = setup(&hooks)?;
    conn.query_row("SELECT COUNT(*) FROM tbl WHERE a = 20", [], |_| Ok(()))?;
    Ok(())
}
