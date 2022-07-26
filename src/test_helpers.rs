#![cfg(all(test, feature = "static"))]

use prelude::*;
use std::{cell::Cell, mem::transmute};

pub mod prelude {
    pub use super::*;
    pub use crate::{function::*, iterator::*, types::*, value::*, *};
}

pub struct TestHelpers {
    pub db: Database,
}

impl TestHelpers {
    pub fn new() -> TestHelpers {
        let db = Database::open_in_memory().expect("failed to open database");
        TestHelpers { db }
    }

    pub fn with_value<T: ToContextResult + 'static, F: Fn(&mut ValueRef) -> Result<()>>(
        &self,
        input: T,
        func: F,
    ) {
        let opts = FunctionOptions::default().set_n_args(-1);
        let input = Cell::new(Some(input));
        let func: Box<dyn Fn(&mut ValueRef) -> Result<()>> = Box::new(func);
        // Safe because we remove the function inside this function.
        let func: Box<dyn 'static + Fn(&mut ValueRef) -> Result<()>> = unsafe { transmute(func) };
        self.db
            .create_scalar_function("produce", &opts, move |c, _| {
                c.set_result(input.replace(None).unwrap())
            })
            .unwrap();
        self.db
            .create_scalar_function("with_value", &opts, move |c, args| {
                c.set_result(func(args[0]))
            })
            .unwrap();
        self.db
            .query_row("SELECT with_value(produce())", (), |_| Ok(()))
            .unwrap();
        self.db.remove_function("with_value", -1).unwrap();
        self.db.remove_function("produce", -1).unwrap();
    }

    pub fn with_value_from_sql<F: Fn(&mut ValueRef) -> Result<()>>(&self, sql: &str, func: F) {
        let opts = FunctionOptions::default().set_n_args(1);
        let func: Box<dyn Fn(&mut ValueRef) -> Result<()>> = Box::new(func);
        // Safe because we remove the function inside this function.
        let func: Box<dyn 'static + Fn(&mut ValueRef) -> Result<()>> = unsafe { transmute(func) };
        self.db
            .create_scalar_function("with_value", &opts, move |c, args| {
                c.set_result(func(args[0]))
            })
            .unwrap();
        self.db
            .query_row(&format!("SELECT with_value({})", sql), (), |_| Ok(()))
            .unwrap();
        self.db.remove_function("with_value", 1).unwrap();
    }
}

#[test]
fn with_value() {
    let h = TestHelpers::new();
    let did_run = Cell::new(false);
    h.with_value("input string", |val| {
        assert_eq!(val.get_str()?, "input string");
        did_run.set(true);
        Ok(())
    });
    assert!(did_run.get());
}

#[test]
fn with_value_from_sql() {
    let h = TestHelpers::new();
    let did_run = Cell::new(false);
    h.with_value_from_sql("NULL", |val| {
        assert!(val.is_null());
        did_run.set(true);
        Ok(())
    });
    assert!(did_run.get());
}
