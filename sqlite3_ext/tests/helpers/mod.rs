use sqlite3_ext::{function::*, vtab::*, *};
use std::sync::Once;

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

    pub fn xCreate<T: std::fmt::Debug>(&mut self, aux: Option<T>, args: &[&str]) {
        println!("=== xCreate with {:?}, {:?}", aux, args);
        self.assert_state(&[VTabLifecycleState::Default]);
        self.state = VTabLifecycleState::Connected;
    }

    pub fn xConnect<T: std::fmt::Debug>(&mut self, aux: Option<T>, args: &[&str]) {
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

    pub fn xFilter(&mut self, index_num: usize, index_str: Option<&str>, args: &[Value]) {
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

    pub fn xColumn(&self, context: &Context, i: usize) {
        println!("=== xColumn with {:?}, {}", context, i);
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

static START: Once = Once::new();

pub fn setup() -> rusqlite::Result<rusqlite::Connection> {
    START.call_once(|| {
        sqlite3_auto_extension(init_test).unwrap();
    });
    rusqlite::Connection::open_in_memory()
}

pub unsafe extern "C" fn init_test(
    _db: *mut ffi::sqlite3,
    err_msg: *mut *mut std::os::raw::c_char,
    api: *mut ffi::sqlite3_api_routines,
) -> std::os::raw::c_int {
    ffi::init_api_routines(api);
    ffi::handle_result(Ok(()), err_msg)
}
