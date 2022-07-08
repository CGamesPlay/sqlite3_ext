use crate::test_vtab::*;
use sqlite3_ext::{function::*, *};
use std::cell::Cell;

#[test]
fn find_function() -> rusqlite::Result<()> {
    #[derive(Default)]
    struct Hooks {
        pub was_called: Cell<bool>,
    }

    impl TestHooks for Hooks {
        fn connect_create<'a>(&'a self, vtab: &mut TestVTab<'a, Self>) {
            vtab.functions.add(1, "overloaded_func", None, |_, _| {
                self.was_called.set(true);
                true
            });
        }
    }

    let hooks = Hooks::default();
    let conn = setup(&hooks)?;
    Connection::from_rusqlite(&conn)
        .create_overloaded_function("overloaded_func", &FunctionOptions::default().set_n_args(1))?;
    conn.query_row("SELECT a FROM tbl WHERE overloaded_func(a)", [], |_| Ok(()))?;

    assert!(hooks.was_called.get(), "overloaded_func was not called");
    Ok(())
}
