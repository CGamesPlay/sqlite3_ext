use crate::test_vtab::*;
use sqlite3_ext::{vtab::*, *};

#[test]
fn best_index_rhs() -> Result<()> {
    #[derive(Default)]
    struct Hooks;

    impl TestHooks for Hooks {
        fn best_index<'a>(
            &'a self,
            _vtab: &TestVTab<'a, Self>,
            index_info: &mut IndexInfo,
        ) -> Result<()> {
            assert_eq!(index_info.distinct_mode(), DistinctMode::Ordered);
            let mut _c = index_info.constraints().next().expect("no constraint");
            #[cfg(modern_sqlite)]
            assert_eq!(_c.rhs()?.get_i64(), 20);
            Ok(())
        }
    }

    let hooks = Hooks::default();
    let conn = setup(&hooks)?;
    conn.query_row("SELECT COUNT(*) FROM tbl WHERE a = 20", (), |_| Ok(()))?;
    Ok(())
}

#[test]
#[cfg(modern_sqlite)]
fn best_index_in() -> Result<()> {
    #[derive(Default)]
    struct Hooks {
        num_filter: std::cell::Cell<u32>,
    }

    impl TestHooks for Hooks {
        fn best_index<'a>(
            &'a self,
            _vtab: &TestVTab<'a, Self>,
            index_info: &mut IndexInfo,
        ) -> Result<()> {
            let mut c = index_info.constraints().next().expect("no constraint");
            if c.usable() {
                c.set_argv_index(Some(0));
                assert!(c.value_list_available(), "value list unavailable");
                assert!(c.set_value_list_wanted(true), "failed to enable multiple");
                index_info.set_estimated_cost(1.0);
            }
            Ok(())
        }

        fn filter<'a>(
            &self,
            _cursor: &mut TestVTabCursor<'a, Self>,
            args: &mut [&mut ValueRef],
        ) -> Result<()> {
            self.num_filter.set(self.num_filter.get() + 1);
            let vals: Vec<String> = ValueList::from_value_ref(args[0])?
                .map(|x| Ok(x.get_str()?.to_owned()))
                .collect()?;
            assert_eq!(vals, vec!("a1", "b2"));
            Ok(())
        }
    }

    let hooks = Hooks::default();
    let conn = setup(&hooks)?;
    conn.query_row(
        "SELECT COUNT(*) FROM tbl WHERE a IN ('a1', 'b2')",
        (),
        |_| Ok(()),
    )?;
    assert_eq!(hooks.num_filter.get(), 1);
    Ok(())
}
