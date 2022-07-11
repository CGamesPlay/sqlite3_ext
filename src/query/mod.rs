//! Facilities for running SQL queries.
use super::{ffi, iterator::*, sqlite3_match_version, types::*, value::*, Connection};
use sealed::sealed;
use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
    ptr, slice, str,
};

mod test;

/// A prepared statement.
///
/// These can be created using methods such as [Connection::prepare].
pub struct Statement {
    base: *mut ffi::sqlite3_stmt,
}

impl Connection {
    /// Prepare some SQL for execution.
    pub fn prepare(&self, sql: &str) -> Result<Statement> {
        let mut ret = MaybeUninit::uninit();
        unsafe {
            sqlite3_match_version! {
                3_020_000 => Error::from_sqlite(ffi::sqlite3_prepare_v3(
                    self.as_ptr() as _,
                    sql.as_ptr() as _,
                    sql.len() as _,
                    0,
                    ret.as_mut_ptr(),
                    ptr::null_mut(),
                ))?,
                _ => Error::from_sqlite(ffi::sqlite3_prepare_v2(
                    self.as_ptr() as _,
                    sql.as_ptr() as _,
                    sql.len() as _,
                    ret.as_mut_ptr(),
                    ptr::null_mut(),
                ))?,
            }
            Ok(Statement {
                base: ret.assume_init(),
            })
        }
    }

    /// Convenience method for `self.prepare(sql)?.execute()`.
    pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<i64> {
        self.prepare(sql)?.execute(params)
    }
}

impl Statement {
    /// Return the underlying sqlite3_stmt pointer.
    pub fn as_ptr(&self) -> *mut ffi::sqlite3_stmt {
        self.base
    }

    /// Return an iterator over the result of the query.
    pub fn query<'a, P: Params>(&'a mut self, params: P) -> Result<ResultSet<'a>> {
        params.bind_params(self.as_ptr())?;
        Ok(ResultSet::new(self))
    }

    /// Execute a query that is expected to return no results (such as an INSERT, UPDATE,
    /// or DELETE).
    ///
    /// If this query returns rows, this method will fail (use [query](Self::query) for
    /// such a query).
    pub fn execute<P: Params>(&mut self, params: P) -> Result<i64> {
        params.bind_params(self.as_ptr())?;
        let db = self.db().lock();
        if self.step()? != false {
            // Query returned rows!
            Err(SQLITE_MISUSE)
        } else {
            Ok(unsafe {
                sqlite3_match_version! {
                    3_037_000 => ffi::sqlite3_changes64(db.as_ptr() as _),
                    _ => ffi::sqlite3_changes(db.as_ptr() as _) as _,
                }
            })
        }
    }

    /// Returns the original text of the prepared statement.
    pub fn sql(&self) -> Result<&str> {
        unsafe {
            let ret = ffi::sqlite3_sql(self.base);
            Ok(CStr::from_ptr(ret).to_str()?)
        }
    }

    /// Returns the number of columns in the result set returned by this query.
    pub fn column_count(&self) -> usize {
        unsafe { ffi::sqlite3_column_count(self.base) as _ }
    }

    pub fn db<'a>(&self) -> &'a Connection {
        unsafe { Connection::from_ptr(ffi::sqlite3_db_handle(self.base)) }
    }

    fn step(&mut self) -> Result<bool> {
        match unsafe { ffi::sqlite3_step(self.base) } {
            ffi::SQLITE_DONE => Ok(false),
            ffi::SQLITE_ROW => Ok(true),
            e => Err(Error::Sqlite(e)),
        }
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_finalize(self.base) };
    }
}

/// An iterator of results for a [Statement].
pub struct ResultSet<'stmt> {
    finished: bool,
    result: QueryResult<'stmt>,
}

impl<'stmt> ResultSet<'stmt> {
    fn new(stmt: &'stmt mut Statement) -> Self {
        Self {
            finished: false,
            result: QueryResult::new(stmt),
        }
    }
}

impl<'stmt> FallibleIteratorMut for ResultSet<'stmt> {
    type Item = QueryResult<'stmt>;
    type Error = Error;

    fn next(&mut self) -> Result<Option<&mut Self::Item>> {
        if self.finished {
            // This is to avoid a case where continuing to use the iterator after
            // it ends would automatically reset the statement, so it would return
            // its results again.
            return Err(SQLITE_MISUSE);
        }
        match self.result.stmt.step() {
            Ok(true) => Ok(Some(&mut self.result)),
            Ok(false) => {
                self.finished = true;
                Ok(None)
            }
            Err(x) => {
                self.finished = true;
                Err(x)
            }
        }
    }
}

impl Drop for ResultSet<'_> {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_reset(self.result.stmt.base) };
    }
}

/// A row returned from a query.
pub struct QueryResult<'stmt> {
    stmt: &'stmt mut Statement,
}

impl<'stmt> QueryResult<'stmt> {
    fn new(stmt: &'stmt mut Statement) -> Self {
        Self { stmt }
    }

    /// Returns the number of columns in the result.
    pub fn len(&self) -> usize {
        self.stmt.column_count()
    }

    /// # Safety
    ///
    /// This method does not verify that only one Column exists for a particular
    /// (statement, position) pair.
    unsafe fn col_unchecked(&self, index: usize) -> Column<'_> {
        debug_assert!(index < self.len(), "index out of bounds");
        Column::new(self.stmt, index)
    }

    /// Get the value in the requested column.
    pub fn col<'a>(&'a mut self, index: usize) -> Column<'a> {
        unsafe { self.col_unchecked(index) }
    }
}

impl std::fmt::Debug for QueryResult<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dt = f.debug_tuple("QueryResult");
        for i in 0..self.len() {
            unsafe { dt.field(&self.col_unchecked(i)) };
        }
        dt.finish()
    }
}

/// A single value returned from a query.
///
/// SQLite automatically converts between data types on request, which is why many of the
/// methods require `&mut`.
pub struct Column<'stmt> {
    stmt: &'stmt Statement,
    position: usize,
}

impl<'stmt> Column<'stmt> {
    fn new(stmt: &'stmt Statement, position: usize) -> Self {
        Self { stmt, position }
    }

    /// Get the bytes of this BLOB value.
    ///
    /// # Safety
    ///
    /// If the type of this value is not BLOB, the behavior of this function is undefined.
    pub unsafe fn get_blob_unchecked(&self) -> &[u8] {
        let len = ffi::sqlite3_column_bytes(self.stmt.base, self.position as _);
        let data = ffi::sqlite3_column_blob(self.stmt.base, self.position as _);
        slice::from_raw_parts(data as _, len as _)
    }

    /// Get the underlying TEXT value.
    ///
    /// This method will fail if the value has invalid UTF-8.
    ///
    /// # Safety
    ///
    /// If the type of this value is not TEXT, the behavior of this function is undefined.
    pub unsafe fn get_str_unchecked(&self) -> Result<&str> {
        Ok(str::from_utf8(self.get_blob_unchecked())?)
    }

    /// Returns the value of the AS clause for this column, if one was specified. If no AS
    /// clause was specified, the name of the column is unspecified and may change from one
    /// release of SQLite to the next.
    pub fn name(&self) -> Result<&str> {
        unsafe {
            let ret = ffi::sqlite3_column_name(self.stmt.base, self.position as _);
            if ret.is_null() {
                Err(SQLITE_NOMEM)
            } else {
                Ok(CStr::from_ptr(ret).to_str()?)
            }
        }
    }

    /// Returns the original, unaliased name of the database that is the origin of this
    /// column.
    pub fn database_name(&self) -> Result<Option<&str>> {
        unsafe {
            let ret = ffi::sqlite3_column_database_name(self.stmt.base, self.position as _);
            if ret.is_null() {
                Ok(None)
            } else {
                Ok(Some(CStr::from_ptr(ret).to_str()?))
            }
        }
    }

    /// Returns the original, unaliased name of the table that is the origin of this
    /// column.
    pub fn table_name(&self) -> Result<Option<&str>> {
        unsafe {
            let ret = ffi::sqlite3_column_table_name(self.stmt.base, self.position as _);
            if ret.is_null() {
                Ok(None)
            } else {
                Ok(Some(CStr::from_ptr(ret).to_str()?))
            }
        }
    }

    /// Returns the original, unaliased name of the column that is the origin of this
    /// column.
    pub fn origin_name(&self) -> Result<Option<&str>> {
        unsafe {
            let ret = ffi::sqlite3_column_origin_name(self.stmt.base, self.position as _);
            if ret.is_null() {
                Ok(None)
            } else {
                Ok(Some(CStr::from_ptr(ret).to_str()?))
            }
        }
    }

    /// Returns the declared type of the column that is the origin of this column. Note
    /// that this does not mean that values contained in this column comply with the
    /// declared type.
    pub fn decltype(&self) -> Result<Option<&str>> {
        unsafe {
            let ret = ffi::sqlite3_column_decltype(self.stmt.base, self.position as _);
            if ret.is_null() {
                Ok(None)
            } else {
                Ok(Some(CStr::from_ptr(ret).to_str()?))
            }
        }
    }
}

impl FromValue for Column<'_> {
    fn value_type(&self) -> ValueType {
        unsafe {
            ValueType::from_sqlite(ffi::sqlite3_column_type(self.stmt.base, self.position as _))
        }
    }

    fn get_i32(&self) -> i32 {
        unsafe { ffi::sqlite3_column_int(self.stmt.base, self.position as _) }
    }

    fn get_i64(&self) -> i64 {
        unsafe { ffi::sqlite3_column_int64(self.stmt.base, self.position as _) }
    }

    fn get_f64(&self) -> f64 {
        unsafe { ffi::sqlite3_column_double(self.stmt.base, self.position as _) }
    }

    fn get_blob(&mut self) -> Result<Option<&[u8]>> {
        unsafe {
            let data = ffi::sqlite3_column_blob(self.stmt.base, self.position as _);
            let len = ffi::sqlite3_column_bytes(self.stmt.base, self.position as _);
            if data.is_null() {
                if self.value_type() == ValueType::Null {
                    return Ok(None);
                } else {
                    return Err(SQLITE_NOMEM);
                }
            } else {
                Ok(Some(slice::from_raw_parts(data as _, len as _)))
            }
        }
    }

    fn get_str(&mut self) -> Result<Option<&str>> {
        Ok(self.get_blob()?.map(|b| str::from_utf8(b)).transpose()?)
    }

    fn to_owned(&self) -> Result<Value> {
        match self.value_type() {
            ValueType::Integer => Ok(Value::from(self.get_i64())),
            ValueType::Float => Ok(Value::from(self.get_f64())),
            ValueType::Text => unsafe { Ok(Value::from(self.get_str_unchecked()?.to_owned())) },
            ValueType::Blob => unsafe { Ok(Value::from(Blob::from(self.get_blob_unchecked()))) },
            ValueType::Null => Ok(Value::Null),
        }
    }
}

impl std::fmt::Debug for Column<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self.value_type() {
            ValueType::Integer => f.debug_tuple("Integer").field(&self.get_i64()).finish(),
            ValueType::Float => f.debug_tuple("Float").field(&self.get_f64()).finish(),
            ValueType::Text => f
                .debug_tuple("Text")
                .field(unsafe { &self.get_str_unchecked() })
                .finish(),
            ValueType::Blob => f
                .debug_tuple("Blob")
                .field(unsafe { &self.get_blob_unchecked() })
                .finish(),
            ValueType::Null => f.debug_tuple("Null").finish(),
        }
    }
}

#[macro_export]
macro_rules! params {
    ($($val:expr),* $(,)?) => {
        |stmt| unsafe {
            use $crate::query::ToParam;
            let mut i = 1i32;
            $(
            $val.bind_param(stmt, i)?;
            #[allow(unused_assignments)]
            { i += 1; }
            )*
            Ok(())
        }
    }
}

#[sealed]
pub trait Params {
    #[doc(hidden)]
    fn bind_params(self, stmt: *mut ffi::sqlite3_stmt) -> Result<()>;
}

#[sealed]
impl Params for () {
    fn bind_params(self, _: *mut ffi::sqlite3_stmt) -> Result<()> {
        Ok(())
    }
}

#[sealed]
impl<T> Params for T
where
    T: FnOnce(*mut ffi::sqlite3_stmt) -> Result<()>,
{
    fn bind_params(self, stmt: *mut ffi::sqlite3_stmt) -> Result<()> {
        self(stmt)
    }
}

#[sealed]
impl<T: ToParam + std::fmt::Debug, const N: usize> Params for [T; N] {
    fn bind_params(self, stmt: *mut ffi::sqlite3_stmt) -> Result<()> {
        for (pos, val) in self.into_iter().enumerate() {
            unsafe { val.bind_param(stmt, pos as i32 + 1)? };
        }
        Ok(())
    }
}

#[sealed]
pub trait ToParam {
    #[doc(hidden)]
    unsafe fn bind_param(self, stmt: *mut ffi::sqlite3_stmt, position: i32) -> Result<()>;
}

macro_rules! to_param {
    ($(#[$attr:meta])* $ty:ty as ($stmt:ident, $pos:ident, $val:ident) => $impl:expr) => {
        $(#[$attr])*
        #[sealed]
        impl ToParam for $ty {
            unsafe fn bind_param(self, $stmt: *mut ffi::sqlite3_stmt, $pos: i32) -> Result<()> {
                let $val = self;
                Error::from_sqlite($impl)
            }
        }
    };
}

to_param!(() as (stmt, pos, _val) => ffi::sqlite3_bind_null(stmt, pos));
to_param!(bool as (stmt, pos, val) => ffi::sqlite3_bind_int(stmt, pos, val as i32));
to_param!(i32 as (stmt, pos, val) => ffi::sqlite3_bind_int(stmt, pos, val));
to_param!(i64 as (stmt, pos, val) => ffi::sqlite3_bind_int64(stmt, pos, val));
to_param!(f64 as (stmt, pos, val) => ffi::sqlite3_bind_double(stmt, pos, val));
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
impl<T: 'static> ToParam for T
where
    Blob: From<T>,
{
    unsafe fn bind_param(self, stmt: *mut ffi::sqlite3_stmt, pos: i32) -> Result<()> {
        let blob = Blob::from(self);
        let len = blob.len();
        let rc = sqlite3_match_version! {
            3_008_007 => ffi::sqlite3_bind_blob64(stmt, pos, blob.into_raw(), len as _, Some(ffi::drop_blob),),
            _ => ffi::sqlite3_bind_blob(stmt, pos, blob.into_raw(), len as _, Some(ffi::drop_blob)),
        };
        Error::from_sqlite(rc)
    }
}

#[sealed]
impl<T> ToParam for Option<T>
where
    T: ToParam,
{
    unsafe fn bind_param(self, stmt: *mut ffi::sqlite3_stmt, pos: i32) -> Result<()> {
        match self {
            Some(x) => x.bind_param(stmt, pos),
            None => Error::from_sqlite(ffi::sqlite3_bind_null(stmt, pos)),
        }
    }
}
