use crate::test_vtab::*;
use sqlite3_ext::{function::*, *};
use std::cell::Cell;

#[test]
fn find_function() -> Result<()> {
    #[derive(Default)]
    struct Hooks {
        pub was_called: Cell<bool>,
    }

    impl TestHooks for Hooks {
        fn connect_create<'a>(&'a self, vtab: &mut TestVTab<'a, Self>) {
            vtab.functions.add(1, "overloaded_func", None, |c, _| {
                self.was_called.set(true);
                c.set_result(true)
            });

            vtab.functions
                .add(1, "passthrough", None, |c, a| c.set_result(&*a[0]));

            vtab.functions
                .add_method(1, "passthrough_method", None, |_, c, a| {
                    c.set_result(&*a[0])
                });
        }
    }

    let hooks = Hooks::default();
    let conn = setup(&hooks)?;
    conn.create_overloaded_function("overloaded_func", &FunctionOptions::default().set_n_args(1))?;
    conn.query_row(
        "SELECT a FROM tbl WHERE overloaded_func(a) LIMIT 1",
        (),
        |_| Ok(()),
    )?;

    assert!(hooks.was_called.get(), "overloaded_func was not called");
    Ok(())
}
