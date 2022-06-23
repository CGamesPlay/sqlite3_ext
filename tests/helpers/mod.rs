#![allow(dead_code)]

use sqlite3_ext::{vtab::*, *};

#[derive(Debug, PartialEq)]
pub enum VTabLifecycleState {
    Default,
    Connected,
    Destroyed,
    Disconnected,
}

pub struct VTabLifecycle {
    state: VTabLifecycleState,
}

#[derive(Debug, PartialEq)]
pub enum CursorLifecycleState {
    Default,
    Filtered,
}

pub struct CursorLifecycle<'vtab> {
    vtab: &'vtab VTabLifecycle,
    state: CursorLifecycleState,
}

impl Default for VTabLifecycle {
    fn default() -> Self {
        VTabLifecycle {
            state: VTabLifecycleState::Default,
        }
    }
}

#[allow(non_snake_case)]
impl<'vtab> VTabLifecycle {
    fn assert_state(&self, options: &[VTabLifecycleState]) {
        if let None = options.iter().position(|x| *x == self.state) {
            panic!("VTab: expected {:?}, found {:?}", options, self.state);
        }
    }

    pub fn xCreate<T: std::fmt::Debug>(&mut self, aux: &T, args: &[&str]) {
        println!("=== xCreate with {:?}, {:?}", aux, args);
        self.assert_state(&[VTabLifecycleState::Default]);
        self.state = VTabLifecycleState::Connected;
    }

    pub fn xConnect<T: std::fmt::Debug>(&mut self, aux: &T, args: &[&str]) {
        println!("=== xConnect with {:?}, {:?}", aux, args);
        self.assert_state(&[VTabLifecycleState::Default]);
        self.state = VTabLifecycleState::Connected;
    }

    pub fn xBestIndex(&self, index_info: &IndexInfo) {
        println!("=== xBestIndex with {:?}", index_info);
        self.assert_state(&[VTabLifecycleState::Connected]);
    }

    pub fn xOpen(&'vtab self) -> CursorLifecycle<'vtab> {
        println!("=== xOpen");
        self.assert_state(&[VTabLifecycleState::Connected]);
        CursorLifecycle {
            vtab: self,
            state: CursorLifecycleState::Default,
        }
    }

    pub fn xDestroy(&mut self) {
        println!("=== xDestroy");
        self.assert_state(&[VTabLifecycleState::Connected]);
        self.state = VTabLifecycleState::Destroyed;
    }

    pub fn xUpdateInsert(&self, args: &[&ValueRef]) {
        println!("=== xUpdate INSERT {:?}", args);
        self.assert_state(&[VTabLifecycleState::Connected]);
    }

    pub fn xUpdateUpdate(&self, rowid: &ValueRef, args: &[&ValueRef]) {
        println!("=== xUpdate UPDATE {:?} {:?}", rowid, args);
        self.assert_state(&[VTabLifecycleState::Connected]);
    }

    pub fn xUpdateDelete(&self, rowid: &ValueRef) {
        println!("=== xUpdate DELETE {:?}", rowid);
        self.assert_state(&[VTabLifecycleState::Connected]);
    }

    pub fn xRename(&self, name: &str) {
        println!("=== xRename to {}", name);
        self.assert_state(&[VTabLifecycleState::Connected]);
    }
}

impl Drop for VTabLifecycle {
    fn drop(&mut self) {
        println!("=== xDisconnect");
        self.assert_state(&[VTabLifecycleState::Connected, VTabLifecycleState::Destroyed]);
        self.state = VTabLifecycleState::Disconnected;
    }
}

#[allow(non_snake_case)]
impl<'vtab> CursorLifecycle<'vtab> {
    fn assert_state(&self, vtab: VTabLifecycleState, options: &[CursorLifecycleState]) {
        self.vtab.assert_state(&[vtab]);
        if let None = options.iter().position(|x| *x == self.state) {
            panic!("Cursor: expected {:?}, found {:?}", options, self.state);
        }
    }

    pub fn xFilter(&mut self, index_num: usize, index_str: Option<&str>, args: &[&ValueRef]) {
        println!(
            "=== xFilter with {}, {:?}, {:?}",
            index_num, index_str, args
        );
        self.assert_state(
            VTabLifecycleState::Connected,
            &[CursorLifecycleState::Default],
        );
        self.state = CursorLifecycleState::Filtered;
    }

    pub fn xNext(&self) {
        println!("=== xNext");
        self.assert_state(
            VTabLifecycleState::Connected,
            &[CursorLifecycleState::Filtered],
        );
    }

    pub fn xEof(&self) {
        println!("=== xEof");
        self.assert_state(
            VTabLifecycleState::Connected,
            &[CursorLifecycleState::Filtered],
        );
    }

    pub fn xColumn(&self, i: usize) {
        println!("=== xColumn with {}", i);
        self.assert_state(
            VTabLifecycleState::Connected,
            &[CursorLifecycleState::Filtered],
        );
    }

    pub fn xRowid(&self) {
        println!("=== xRowid");
        self.assert_state(
            VTabLifecycleState::Connected,
            &[CursorLifecycleState::Filtered],
        );
    }
}

impl Drop for CursorLifecycle<'_> {
    fn drop(&mut self) {
        println!("=== xClose");
        self.assert_state(
            VTabLifecycleState::Connected,
            &[
                CursorLifecycleState::Default,
                CursorLifecycleState::Filtered,
            ],
        );
    }
}

pub fn setup() -> rusqlite::Result<rusqlite::Connection> {
    rusqlite::Connection::open_in_memory()
}
