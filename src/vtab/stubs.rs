use super::super::{ffi, function::Context, types::*, value::*, vtab::*, Connection};
use std::{
    ffi::{CStr, CString},
    marker::PhantomData,
    os::raw::{c_int, c_void},
    ptr, slice,
};

#[repr(C)]
struct VTabHandle<'vtab, T: VTab<'vtab>> {
    base: ffi::sqlite3_vtab,
    // Declare txn before vtab to drop it first
    vtab: T,
    txn: Option<*mut c_void>,
    phantom: PhantomData<&'vtab T>,
}

#[repr(C)]
struct VTabCursorHandle<'vtab, T: VTab<'vtab>> {
    base: ffi::sqlite3_vtab_cursor,
    cursor: T::Cursor,
    phantom: PhantomData<&'vtab T>,
}

macro_rules! vtab_connect {
    ($name:ident, $trait:ident, $func:ident) => {
        pub unsafe extern "C" fn $name<'vtab, T: $trait<'vtab> + 'vtab>(
            db: *mut ffi::sqlite3,
            module: *mut c_void,
            argc: i32,
            argv: *const *const i8,
            p_vtab: *mut *mut ffi::sqlite3_vtab,
            err_msg: *mut *mut i8,
        ) -> c_int {
            let conn = &*(db as *mut Connection);
            let module = ModuleHandle::<'vtab, T>::from_ptr(module);
            let args: Result<Vec<&str>> = slice::from_raw_parts(argv, argc as _)
                .into_iter()
                .map(|arg| {
                    CStr::from_ptr(*arg)
                        .to_str()
                        .map_err(|e| Error::Utf8Error(e))
                })
                .collect();
            let args = match args {
                Ok(x) => x,
                Err(e) => return ffi::handle_error(e, err_msg),
            };
            let vtab_conn = &mut *(db as *mut VTabConnection);
            let ret = T::$func(vtab_conn, &module.aux, args.as_slice());
            let (sql, vtab) = match ret {
                Ok(x) => x,
                Err(e) => return ffi::handle_error(e, err_msg),
            };
            let rc = ffi::sqlite3_declare_vtab(
                conn.as_ptr(),
                CString::from_vec_unchecked(sql.into_bytes()).as_ptr() as _,
            );
            if rc != ffi::SQLITE_OK {
                return rc;
            }
            let vtab = Box::new(VTabHandle {
                base: ffi::sqlite3_vtab {
                    pModule: ptr::null_mut(),
                    nRef: 0,
                    zErrMsg: ptr::null_mut(),
                },
                vtab,
                txn: None,
                phantom: PhantomData,
            });
            *p_vtab = Box::into_raw(vtab) as _;
            ffi::SQLITE_OK
        }
    };
}

vtab_connect!(vtab_create, CreateVTab, create);
vtab_connect!(vtab_connect, VTab, connect);

pub unsafe extern "C" fn vtab_connect_transaction<'vtab, T: TransactionVTab<'vtab> + 'vtab>(
    db: *mut ffi::sqlite3,
    module: *mut c_void,
    argc: i32,
    argv: *const *const i8,
    p_vtab: *mut *mut ffi::sqlite3_vtab,
    err_msg: *mut *mut i8,
) -> c_int {
    match vtab_connect::<T>(db, module, argc, argv, p_vtab, err_msg) {
        ffi::SQLITE_OK => (),
        rc => return rc,
    }
    vtab_begin::<T>(*p_vtab)
}

pub unsafe extern "C" fn vtab_create_transaction<
    'vtab,
    T: TransactionVTab<'vtab> + CreateVTab<'vtab> + 'vtab,
>(
    db: *mut ffi::sqlite3,
    module: *mut c_void,
    argc: i32,
    argv: *const *const i8,
    p_vtab: *mut *mut ffi::sqlite3_vtab,
    err_msg: *mut *mut i8,
) -> c_int {
    match vtab_create::<T>(db, module, argc, argv, p_vtab, err_msg) {
        ffi::SQLITE_OK => (),
        rc => return rc,
    }
    vtab_begin::<T>(*p_vtab)
}

pub unsafe extern "C" fn vtab_best_index<'vtab, T: VTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    info: *mut ffi::sqlite3_index_info,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let info = &mut *(info as *mut IndexInfo);
    let ret = vtab.vtab.best_index(info);
    ffi::handle_result(ret, &mut vtab.base.zErrMsg)
}

pub unsafe extern "C" fn vtab_open<'vtab, T: VTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    p_cursor: *mut *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let cursor = match vtab.vtab.open() {
        Ok(x) => x,
        Err(e) => return ffi::handle_error(e, &mut vtab.base.zErrMsg),
    };
    let cursor = Box::new(VTabCursorHandle::<'vtab, T> {
        base: ffi::sqlite3_vtab_cursor {
            pVtab: ptr::null_mut(),
        },
        cursor,
        phantom: PhantomData,
    });
    *p_cursor = Box::into_raw(cursor) as _;
    ffi::SQLITE_OK
}

pub unsafe extern "C" fn vtab_close<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let cursor: Box<VTabCursorHandle<T>> = Box::from_raw(cursor as _);
    std::mem::drop(cursor);
    ffi::SQLITE_OK
}

pub unsafe extern "C" fn vtab_disconnect<'vtab, T: VTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
) -> c_int {
    let vtab: Box<VTabHandle<T>> = Box::from_raw(vtab as _);
    std::mem::drop(vtab);
    ffi::SQLITE_OK
}

pub unsafe extern "C" fn vtab_destroy<'vtab, T: CreateVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    match vtab.vtab.destroy() {
        Ok(_) => {
            let vtab: Box<VTabHandle<T>> = Box::from_raw(vtab as _);
            std::mem::drop(vtab);
            ffi::SQLITE_OK
        }
        Err(e) => ffi::handle_error(e, &mut vtab.base.zErrMsg),
    }
}

pub unsafe extern "C" fn vtab_filter<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
    index_num: i32,
    index_str: *const i8,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    let index_str = if index_str.is_null() {
        None
    } else {
        CStr::from_ptr(index_str).to_str().ok()
    };
    let args = slice::from_raw_parts(argv as *mut &Value, argc as _);
    ffi::handle_result(
        cursor.cursor.filter(index_num as _, index_str, args),
        &mut (*cursor.base.pVtab).zErrMsg,
    )
}

pub unsafe extern "C" fn vtab_next<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    ffi::handle_result(cursor.cursor.next(), &mut (*cursor.base.pVtab).zErrMsg)
}

pub unsafe extern "C" fn vtab_eof<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    cursor.cursor.eof() as _
}

pub unsafe extern "C" fn vtab_column<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
    context: *mut ffi::sqlite3_context,
    i: i32,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    let context = &mut *(context as *mut Context);
    ffi::handle_result(
        cursor.cursor.column(context, i as _),
        &mut (*cursor.base.pVtab).zErrMsg,
    )
}

pub unsafe extern "C" fn vtab_rowid<'vtab, T: VTab<'vtab> + 'vtab>(
    cursor: *mut ffi::sqlite3_vtab_cursor,
    ptr: *mut i64,
) -> c_int {
    let cursor = &mut *(cursor as *mut VTabCursorHandle<T>);
    match cursor.cursor.rowid() {
        Ok(x) => {
            *ptr = x;
            ffi::SQLITE_OK
        }
        Err(e) => ffi::handle_error(e, &mut (*cursor.base.pVtab).zErrMsg),
    }
}

pub unsafe extern "C" fn vtab_update<'vtab, T: UpdateVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    argc: i32,
    argv: *mut *mut ffi::sqlite3_value,
    p_rowid: *mut i64,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let args = slice::from_raw_parts(argv as *mut &Value, argc as _);
    if args.len() == 1 {
        ffi::handle_result(vtab.vtab.delete(args[0]), &mut vtab.base.zErrMsg)
    } else {
        if args[0].value_type() == ValueType::Null {
            match vtab.vtab.insert(&args[1..]) {
                Ok(rowid) => {
                    *p_rowid = rowid;
                    ffi::SQLITE_OK
                }
                Err(e) => ffi::handle_error(e, &mut vtab.base.zErrMsg),
            }
        } else {
            ffi::handle_result(
                vtab.vtab.update(args[0], &args[1..]),
                &mut vtab.base.zErrMsg,
            )
        }
    }
}

pub unsafe extern "C" fn vtab_begin<'vtab, T: TransactionVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    if let Some(x) = vtab.txn.take() {
        Box::from_raw(x as *mut T::Transaction);
    }
    match vtab.vtab.begin() {
        Ok(txn) => {
            vtab.txn.replace(Box::into_raw(Box::new(txn)) as _);
            ffi::SQLITE_OK
        }
        Err(e) => ffi::handle_error(e, &mut vtab.base.zErrMsg),
    }
}

pub unsafe extern "C" fn vtab_sync<'vtab, T: TransactionVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let txn = &mut *(vtab.txn.unwrap() as *mut T::Transaction);
    ffi::handle_result(txn.sync(), &mut vtab.base.zErrMsg)
}

pub unsafe extern "C" fn vtab_commit<'vtab, T: TransactionVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let txn = Box::from_raw(vtab.txn.take().unwrap() as *mut T::Transaction);
    ffi::handle_result(txn.commit(), &mut vtab.base.zErrMsg)
}

pub unsafe extern "C" fn vtab_rollback<'vtab, T: TransactionVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let txn = Box::from_raw(vtab.txn.take().unwrap() as *mut T::Transaction);
    ffi::handle_result(txn.rollback(), &mut vtab.base.zErrMsg)
}

pub unsafe extern "C" fn vtab_rename<'vtab, T: RenameVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    name: *const i8,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let name = CStr::from_ptr(name)
        .to_str()
        .map_err(|e| Error::Utf8Error(e));
    let name = match name {
        Ok(name) => name,
        Err(e) => return ffi::handle_error(e, &mut vtab.base.zErrMsg),
    };
    ffi::handle_result(vtab.vtab.rename(name), &mut vtab.base.zErrMsg)
}

pub unsafe extern "C" fn vtab_savepoint<'vtab, T: TransactionVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    n: c_int,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let txn = &mut *(vtab.txn.unwrap() as *mut T::Transaction);
    ffi::handle_result(txn.savepoint(n), &mut vtab.base.zErrMsg)
}

pub unsafe extern "C" fn vtab_release<'vtab, T: TransactionVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    n: c_int,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let txn = &mut *(vtab.txn.unwrap() as *mut T::Transaction);
    ffi::handle_result(txn.release(n), &mut vtab.base.zErrMsg)
}

pub unsafe extern "C" fn vtab_rollback_to<'vtab, T: TransactionVTab<'vtab> + 'vtab>(
    vtab: *mut ffi::sqlite3_vtab,
    n: c_int,
) -> c_int {
    let vtab = &mut *(vtab as *mut VTabHandle<T>);
    let txn = &mut *(vtab.txn.unwrap() as *mut T::Transaction);
    ffi::handle_result(txn.rollback_to(n), &mut vtab.base.zErrMsg)
}

pub unsafe extern "C" fn vtab_shadow_name<'vtab, T: CreateVTab<'vtab> + 'vtab>(
    name: *const i8,
) -> c_int {
    let name = CStr::from_ptr(name).to_bytes();
    for candidate in T::SHADOW_NAMES {
        if candidate.as_bytes() == name {
            return 1;
        }
    }
    0
}
