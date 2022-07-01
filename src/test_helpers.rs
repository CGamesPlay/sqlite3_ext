#![cfg(all(test, feature = "static"))]

use prelude::*;
use std::{cell::Cell, mem::transmute};

pub mod prelude {
    pub use super::*;
    pub use crate::{function::*, types::*, value::*, *};
}

pub struct TestHelpers {
    db: rusqlite::Connection,
}

impl TestHelpers {
    pub fn new() -> TestHelpers {
        let db = rusqlite::Connection::open_in_memory().expect("failed to open database");
        TestHelpers { db }
    }

    pub fn sqlite3_ext<'a>(&self) -> &'a mut crate::Connection {
        unsafe { crate::Connection::from_ptr(self.db.handle()) }
    }

    pub fn with_value<Input: rusqlite::ToSql, F: Fn(&mut ValueRef) -> Result<()>>(
        &self,
        val: Input,
        func: F,
    ) {
        let opts = FunctionOptions::default().set_n_args(1);
        let func: Box<dyn Fn(&mut ValueRef) -> Result<()>> = Box::new(func);
        // Safe because we remove the function inside this function.
        let func: Box<dyn 'static + Fn(&mut ValueRef) -> Result<()>> = unsafe { transmute(func) };
        self.sqlite3_ext()
            .create_scalar_function("with_value", &opts, move |_, args| func(args[0]))
            .unwrap();
        self.db
            .query_row("SELECT with_value(?)", [val], |_| Ok(()))
            .unwrap();
        self.sqlite3_ext().remove_function("with_value", 1).unwrap();
    }

    pub fn with_value_from_sql<F: Fn(&mut ValueRef) -> Result<()>>(&self, sql: &str, func: F) {
        let opts = FunctionOptions::default().set_n_args(1);
        let func: Box<dyn Fn(&mut ValueRef) -> Result<()>> = Box::new(func);
        // Safe because we remove the function inside this function.
        let func: Box<dyn 'static + Fn(&mut ValueRef) -> Result<()>> = unsafe { transmute(func) };
        self.sqlite3_ext()
            .create_scalar_function("with_value", &opts, move |_, args| func(args[0]))
            .unwrap();
        self.db
            .query_row(&format!("SELECT with_value({})", sql), [], |_| Ok(()))
            .unwrap();
        self.sqlite3_ext().remove_function("with_value", 1).unwrap();
    }
}

#[test]
fn with_value() {
    let h = TestHelpers::new();
    let did_run = Cell::new(false);
    h.with_value("input string", |val| {
        assert_eq!(val.get_str()?.unwrap(), "input string");
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
        assert_eq!(val.value_type(), ValueType::Null);
        did_run.set(true);
        Ok(())
    });
    assert!(did_run.get());
}
