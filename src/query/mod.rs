//! Facilities for running SQL queries.
use super::{ffi, iterator::*, sqlite3_match_version, types::*, value::*, Connection};
use std::{ffi::CStr, mem::MaybeUninit, ptr, slice, str};

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
}

impl Statement {
    /// Return the underlying sqlite3_stmt pointer.
    pub fn as_ptr(&self) -> *mut ffi::sqlite3_stmt {
        self.base
    }

    /// Return an iterator over the result of the query.
    pub fn query<'a>(&'a mut self) -> Result<ResultSet<'a>> {
        unsafe { ffi::sqlite3_reset(self.base) };
        Ok(ResultSet::new(self))
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
