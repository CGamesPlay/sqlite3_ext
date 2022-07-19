use super::Statement;
use crate::{ffi, sqlite3_match_version, sqlite3_require_version, types::*, value::*};
use sealed::sealed;
use std::ffi::CString;

/// Create a [Params] with values of mixed types.
///
/// If all of the parameters to a query are of the same type, a simple array can be used to
/// bind them. If you want to pass multiple values, you need to use this macro.
///
/// # Syntax
///
/// This macro works like an array expression, except the values do not have to be the same
/// type (and is more performant than an array of `dyn ToParam`).
///
/// ```no_run
/// use sqlite3_ext::{Connection, Result, params};
///
/// fn do_thing(conn: &Connection) -> Result<i64> {
///     conn.execute(
///         "INSERT INTO tbl VALUES (?, ?)",
///         params![1024, "one thousand twenty four"],
///     )
/// }
/// ```
///
/// Named parameters are always provided as a tuple, and work with this macro or the normal
/// array syntax.
///
/// ```no_run
/// use sqlite3_ext::{Connection, Result, params};
///
/// fn do_thing(conn: &Connection) -> Result<i64> {
///     conn.execute(
///         "INSERT INTO tbl VALUES (:number, :name)",
///         params![(":name", "one thousand twenty four"), (":number", 1024)],
///     )
/// }
/// ```
#[macro_export]
macro_rules! params {
    ($($val:expr),* $(,)?) => {
        |stmt: &mut $crate::query::Statement| {{
            #![allow(unused_assignments)]
            use $crate::query::ToParam;
            let mut i = 1i32;
            $(
            $val.bind_param(stmt, i)?;
            i += 1;
            )*
            Ok(())
        }}
    }
}

/// Trait for collections of parameters to a query.
///
/// This is a private trait with no public API. There are existing implementations which should
/// cover most use cases:
///
/// - An empty tuple (`()`) binds no parameters to the query.
/// - An array binds parameters that are all the same type.
/// - The [params!] macro binds parameters of arbitrary types.
/// - A closure can arbitrarily bind parameters.
///
/// Named parameters are implemented by using a tuple of `("name", value)`, and can be in any
/// order. See [params!] for an example.
///
/// # Using a closure
///
/// If you are dynamically creating SQL queries and need to dynamically bind parameters to
/// them, you can use a closure to accomplish this.
///
/// ```no_run
/// use sqlite3_ext::{Connection, Result, query::{ Statement, ToParam }};
///
/// fn do_thing(conn: &Connection) -> Result<i64> {
///     conn.prepare("INSERT INTO tbl VALUES (?, ?)")?
///         .execute(|stmt: &mut Statement| {
///             "foo".bind_param(stmt, 1)?;
///             "bar".bind_param(stmt, 2)?;
///             Ok(())
///         })
/// }
/// ```
pub trait Params {
    fn bind_params(self, stmt: &mut Statement) -> Result<()>;
}

impl Params for () {
    fn bind_params(self, _: &mut Statement) -> Result<()> {
        Ok(())
    }
}

impl<T> Params for T
where
    T: FnOnce(&mut Statement) -> Result<()>,
{
    fn bind_params(self, stmt: &mut Statement) -> Result<()> {
        self(stmt)
    }
}

impl<T: ToParam> Params for Vec<T> {
    fn bind_params(self, stmt: &mut Statement) -> Result<()> {
        for (pos, val) in self.into_iter().enumerate() {
            val.bind_param(stmt, pos as i32 + 1)?;
        }
        Ok(())
    }
}

impl<T: ToParam, const N: usize> Params for [T; N] {
    fn bind_params(self, stmt: &mut Statement) -> Result<()> {
        for (pos, val) in self.into_iter().enumerate() {
            val.bind_param(stmt, pos as i32 + 1)?;
        }
        Ok(())
    }
}

impl Params for &mut [&mut ValueRef] {
    fn bind_params(self, stmt: &mut Statement) -> Result<()> {
        for (pos, val) in self.into_iter().enumerate() {
            val.bind_param(stmt, pos as i32 + 1)?;
        }
        Ok(())
    }
}

/// Trait for types which can be passed into SQLite queries as parameters.
#[sealed]
pub trait ToParam {
    /// Bind this value to the prepared Statement at the provided position.
    ///
    /// Note: the position of a named parameter can be obtained using
    /// [Statement::parameter_position].
    fn bind_param(self, stmt: &mut Statement, position: i32) -> Result<()>;
}

macro_rules! to_param {
    ($(#[$attr:meta])* $ty:ty as ($stmt:ident, $pos:ident, $val:ident) => $impl:expr) => {
        $(#[$attr])*
        #[sealed]
        impl ToParam for $ty {
            fn bind_param(self, stmt: &mut Statement, $pos: i32) -> Result<()> {
                let $val = self;
                let $stmt = stmt.base;
                Error::from_sqlite(unsafe { $impl })
            }
        }
    };
}

to_param!(() as (stmt, pos, _val) => ffi::sqlite3_bind_null(stmt, pos));
to_param!(bool as (stmt, pos, val) => ffi::sqlite3_bind_int(stmt, pos, val as i32));
to_param!(i64 as (stmt, pos, val) => ffi::sqlite3_bind_int64(stmt, pos, val));
to_param!(f64 as (stmt, pos, val) => ffi::sqlite3_bind_double(stmt, pos, val));
to_param!(&mut ValueRef as (stmt, pos, val) => ffi::sqlite3_bind_value(stmt, pos, val.as_ptr()));
to_param!(&'static str as (stmt, pos, val) => {
    let val = val.as_bytes();
    let len = val.len();
    sqlite3_match_version! {
        3_008_007 => ffi::sqlite3_bind_text64(stmt, pos, val.as_ptr() as _, len as _, None, ffi::SQLITE_UTF8 as _),
        _ => ffi::sqlite3_bind_text(stmt, pos, val.as_ptr() as _, len as _, None),
    }
});
to_param!(String as (stmt, pos, val) => {
    let val = val.as_bytes();
    let len = val.len();
    let cstring = CString::new(val).unwrap().into_raw();
    sqlite3_match_version! {
        3_008_007 => ffi::sqlite3_bind_text64(stmt, pos, cstring, len as _, Some(ffi::drop_cstring), ffi::SQLITE_UTF8 as _),
        _ => ffi::sqlite3_bind_text(stmt, pos, cstring, len as _, Some(ffi::drop_cstring)),
    }
});

#[sealed]
impl<'a> ToParam for &'a ValueRef {
    fn bind_param(self, stmt: &mut Statement, pos: i32) -> Result<()> {
        unsafe { Error::from_sqlite(ffi::sqlite3_bind_value(stmt.base, pos, self.as_ptr())) }
    }
}

/// Sets the parameter to a dynamically typed [Value].
#[sealed]
impl ToParam for Value {
    fn bind_param(self, stmt: &mut Statement, pos: i32) -> Result<()> {
        match self {
            Value::Integer(x) => x.bind_param(stmt, pos),
            Value::Float(x) => x.bind_param(stmt, pos),
            Value::Text(x) => x.bind_param(stmt, pos),
            Value::Blob(x) => x.bind_param(stmt, pos),
            Value::Null => ().bind_param(stmt, pos),
        }
    }
}

#[sealed]
impl<T: Into<Blob> + 'static> ToParam for T {
    fn bind_param(self, stmt: &mut Statement, pos: i32) -> Result<()> {
        let blob = self.into();
        let len = blob.len();
        let rc = unsafe {
            sqlite3_match_version! {
                3_008_007 => ffi::sqlite3_bind_blob64(stmt.base, pos, blob.into_raw(), len as _, Some(ffi::drop_blob),),
                _ => ffi::sqlite3_bind_blob(stmt.base, pos, blob.into_raw(), len as _, Some(ffi::drop_blob)),
            }
        };
        Error::from_sqlite(rc)
    }
}

/// Sets the parameter to the contained value or NULL.
#[sealed]
impl<T> ToParam for Option<T>
where
    T: ToParam,
{
    fn bind_param(self, stmt: &mut Statement, pos: i32) -> Result<()> {
        match self {
            Some(x) => x.bind_param(stmt, pos),
            None => ().bind_param(stmt, pos),
        }
    }
}

/// Sets the parameter to NULL with this value as an associated pointer.
#[sealed]
impl<T: 'static> ToParam for PassedRef<T> {
    fn bind_param(self, stmt: &mut Statement, pos: i32) -> Result<()> {
        let _ = (POINTER_TAG, &stmt, pos);
        sqlite3_require_version!(3_020_000, unsafe {
            Error::from_sqlite(ffi::sqlite3_bind_pointer(
                stmt.base,
                pos,
                Box::into_raw(Box::new(self)) as _,
                POINTER_TAG,
                Some(ffi::drop_boxed::<PassedRef<T>>),
            ))
        })
    }
}

/// Used to bind named parameters. Sets the parameter with the name at `self.0` to the value at
/// `self.1`.
#[sealed]
impl<K, V> ToParam for (K, V)
where
    K: Into<Vec<u8>>,
    V: ToParam,
{
    fn bind_param(self, stmt: &mut Statement, _: i32) -> Result<()> {
        let pos = stmt.parameter_position(self.0);
        match pos {
            Some(pos) => self.1.bind_param(stmt, pos.get()),
            None => Err(SQLITE_RANGE),
        }
    }
}
