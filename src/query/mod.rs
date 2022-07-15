//! Facilities for running SQL queries.
//!
//! The main entry points into this module are [Connection::prepare], [Connection::execute],
//! and [Connection::query_row].
use super::{ffi, iterator::*, sqlite3_match_version, types::*, value::*, Connection};
pub use params::*;
use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
    num::NonZeroI32,
    ops::{Index, IndexMut},
    ptr, slice, str,
};

mod params;
mod test;

#[derive(Copy, Clone, Eq, PartialEq)]
enum QueryState {
    Ready,
    Active,
    Finished,
}

/// A prepared statement.
///
/// The basic method for accessing data using sqlite3_ext is:
///
/// 1. Create a Statement using [Connection::prepare].
/// 2. Bind parameters (if necessary) using [Statement::query].
/// 3. Retrieve results using [Statement::map] or [Statement::next].
///
/// Statement objects can be reused for multiple executions. A call to [query](Self::query) resets
/// the bound parameters and restarts the query. This also applies to methods that call query
/// internally, like [execute](Self::execute) and [query_row](Self::query_row).
///
/// Results can be accessed in an imperative or functional style. The imperative style looks like
/// this:
///
/// ```no_run
/// use sqlite3_ext::*;
///
/// fn pages_imperative(conn: &Connection, user_id: i64) -> Result<Vec<(i64, Option<String>)>> {
///     let mut stmt = conn.prepare("SELECT id, name FROM pages WHERE owner_id = ?")?;
///     stmt.query([user_id])?;
///     let mut results = Vec::new();
///     while let Some(row) = stmt.next()? {
///         results.push((
///             row[0].get_i64(),
///             row[1].get_str()?.map(String::from),
///         ));
///     }
///     Ok(results)
/// }
/// ```
///
/// The functional style makes use of [FallibleIterator] methods.
///
/// ```no_run
/// use sqlite3_ext::*;
///
/// fn pages_functional(conn: &Connection, user_id: i64) -> Result<Vec<(i64, Option<String>)>> {
///     let results: Vec<(i64, Option<String>)> = conn
///         .prepare("SELECT id, name FROM pages WHERE owner_id = ?")?
///         .query([user_id])?
///         .map(|row| {
///             Ok((
///                 row[0].get_i64(),
///                 row[1].get_str()?.map(String::from),
///             ))
///         })
///         .collect()?;
///     Ok(results)
/// }
/// ```
pub struct Statement {
    base: *mut ffi::sqlite3_stmt,
    state: QueryState,
    // We allocate column objects for all columns so that they can be returned by our Index
    // implementation. It's possible to skip this if we add a lifetime parameter to Column to
    // prevent pointer aliasing, but then we can't use Index and IndexMut.
    columns: Box<[Column]>,
}

impl Connection {
    /// Prepare some SQL for execution.
    pub fn prepare(&self, sql: &str) -> Result<Statement> {
        const FLAGS: u32 = 0;
        let guard = self.lock();
        let mut ret = MaybeUninit::uninit();
        Error::from_sqlite_desc(
            unsafe {
                sqlite3_match_version! {
                    3_020_000 => ffi::sqlite3_prepare_v3(
                        self.as_mut_ptr(),
                        sql.as_ptr() as _,
                        sql.len() as _,
                        FLAGS,
                        ret.as_mut_ptr(),
                        ptr::null_mut(),
                    ),
                    _ => ffi::sqlite3_prepare_v2(
                        self.as_mut_ptr(),
                        sql.as_ptr() as _,
                        sql.len() as _,
                        ret.as_mut_ptr(),
                        ptr::null_mut(),
                    ),
                }
            },
            guard,
        )?;
        let stmt = unsafe { ret.assume_init() };
        let len = unsafe { ffi::sqlite3_column_count(stmt) as usize };
        let columns = (0..len).map(|i| Column::new(stmt, i)).collect();
        Ok(Statement {
            base: stmt,
            state: QueryState::Ready,
            columns,
        })
    }

    /// Convenience method for `self.prepare(sql)?.query_row(params, f)`. See
    /// [Statement::query_row].
    pub fn query_row<P: Params, R, F: FnOnce(&mut QueryResult) -> Result<R>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> Result<R> {
        self.prepare(sql)?.query_row(params, f)
    }

    /// Convenience method for `self.prepare(sql)?.execute(params)`. See [Statement::execute].
    pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<i64> {
        self.prepare(sql)?.execute(params)
    }

    /// Convenience method for `self.prepare(sql)?.insert(params)`. See [Statement::insert].
    pub fn insert<P: Params>(&self, sql: &str, params: P) -> Result<i64> {
        self.prepare(sql)?.insert(params)
    }
}

impl Statement {
    /// Return the underlying sqlite3_stmt pointer.
    ///
    /// # Safety
    ///
    /// This method is unsafe because applying SQLite methods to the sqlite3_stmt pointer returned
    /// by this method may violate invariants of other methods on this statement.
    pub unsafe fn as_ptr(&self) -> *mut ffi::sqlite3_stmt {
        self.base
    }

    /// Bind the provided parameters to the query. If the query was previously used, it is reset
    /// and existing paramters are cleared.
    ///
    /// This method is not necessary to call on the first execution of a query where there are no
    /// parameters to bind (e.g. on a single-use hard-coded query).
    pub fn query<P: Params>(&mut self, params: P) -> Result<&mut Self> {
        if self.state != QueryState::Ready {
            unsafe {
                ffi::sqlite3_reset(self.base);
                Error::from_sqlite(ffi::sqlite3_clear_bindings(self.base))?;
            }
            self.state = QueryState::Ready;
        }
        params.bind_params(self)?;
        Ok(self)
    }

    /// Execute a query which is expected to return only a single row.
    ///
    /// This method will fail with [SQLITE_MISUSE] if the query returns more than a single
    /// row. It will fail with [SQLITE_EMPTY] if the query does not return any rows.
    ///
    /// If you are not storing this Statement for later reuse, [Connection::query_row] is a
    /// shortcut for this method.
    pub fn query_row<P: Params, R, F: FnOnce(&mut QueryResult) -> Result<R>>(
        &mut self,
        params: P,
        f: F,
    ) -> Result<R> {
        let rs = self.query(params)?;
        let ret = match rs.next()? {
            None => return Err(SQLITE_EMPTY),
            Some(r) => f(r)?,
        };
        if let Some(_) = rs.next()? {
            return Err(SQLITE_MISUSE);
        }
        Ok(ret)
    }

    /// Execute a query that is expected to return no results (such as an INSERT, UPDATE, or
    /// DELETE).
    ///
    /// If this query returns rows, this method will fail with [SQLITE_MISUSE] (use
    /// [query](Self::query) for a query which returns rows).
    ///
    /// If you are not storing this Statement for later reuse, [Connection::execute] is a shortcut
    /// for this method.
    pub fn execute<P: Params>(&mut self, params: P) -> Result<i64> {
        let db = unsafe { self.db() }.lock();
        if let Some(_) = self.query(params)?.next()? {
            // Query returned rows!
            Err(SQLITE_MISUSE)
        } else {
            Ok(unsafe {
                sqlite3_match_version! {
                    3_037_000 => ffi::sqlite3_changes64(db.as_mut_ptr()),
                    _ => ffi::sqlite3_changes(db.as_mut_ptr()) as _,
                }
            })
        }
    }

    /// Execute a query that is expected to be an INSERT, then return the inserted rowid.
    ///
    /// This method will fail with [SQLITE_MISUSE] if this method returns rows, but there are no
    /// other verifications that the executed statement is actually an INSERT. If this Statement is
    /// not an INSERT, the return value of this function is meaningless.
    pub fn insert<P: Params>(&mut self, params: P) -> Result<i64> {
        let db = unsafe { self.db() }.lock();
        if let Some(_) = self.query(params)?.next()? {
            // Query returned rows!
            Err(SQLITE_MISUSE)
        } else {
            Ok(unsafe { ffi::sqlite3_last_insert_rowid(db.as_mut_ptr()) })
        }
    }

    /// Returns the original text of the prepared statement.
    pub fn sql(&self) -> Result<&str> {
        unsafe {
            let ret = ffi::sqlite3_sql(self.base);
            Ok(CStr::from_ptr(ret).to_str()?)
        }
    }

    /// Returns the number of parameters which should be bound to the query. Valid
    /// parameter positions are `1..=self.parameter_count()`.
    pub fn parameter_count(&self) -> i32 {
        unsafe { ffi::sqlite3_bind_parameter_count(self.base) }
    }

    /// Returns the name of the parameter at the given position. Note that the first
    /// parameter has a position of 1, not 0.
    pub fn parameter_name(&self, position: i32) -> Option<&str> {
        unsafe {
            let ptr = ffi::sqlite3_bind_parameter_name(self.base, position);
            match ptr.is_null() {
                true => None,
                // Safety - in safe code this value must have originally come
                // from a &str, so it's valid UTF-8.
                false => Some(str::from_utf8_unchecked(CStr::from_ptr(ptr).to_bytes())),
            }
        }
    }

    /// Return the position of the parameter with the provided name.
    pub fn parameter_position(&self, name: impl Into<Vec<u8>>) -> Option<NonZeroI32> {
        CString::new(name).ok().and_then(|name| {
            NonZeroI32::new(unsafe { ffi::sqlite3_bind_parameter_index(self.base, name.as_ptr()) })
        })
    }

    /// Returns the number of columns in the result set returned by this query.
    pub fn column_count(&self) -> usize {
        unsafe { ffi::sqlite3_column_count(self.base) as _ }
    }

    /// Returns the current result, without advancing the cursor. This method returns `None` if the
    /// query has already run to completion, or if the query has not been started using
    /// [query](Self::query).
    pub fn current_result(&self) -> Option<&QueryResult> {
        match self.state {
            QueryState::Active => Some(QueryResult::from_statement(self)),
            _ => None,
        }
    }

    /// Mutable version of [current_result](Self::current_result).
    pub fn current_result_mut(&mut self) -> Option<&mut QueryResult> {
        match self.state {
            QueryState::Active => Some(QueryResult::from_statement_mut(self)),
            _ => None,
        }
    }

    /// Returns a handle to the Connection associated with this statement.
    ///
    /// # Safety
    ///
    /// The returned reference's lifetime is not tied to the lifetime of this Statement. It
    /// is the responsibility of the caller to ensure that the Connection reference is not
    /// improperly used.
    pub unsafe fn db<'a>(&self) -> &'a Connection {
        Connection::from_ptr(ffi::sqlite3_db_handle(self.base))
    }
}

impl FallibleIteratorMut for Statement {
    type Item = QueryResult;
    type Error = Error;

    fn next(&mut self) -> Result<Option<&mut Self::Item>> {
        match self.state {
            QueryState::Ready | QueryState::Active => unsafe {
                let guard = self.db().lock();
                let rc = ffi::sqlite3_step(self.base);
                Error::from_sqlite_desc(rc, guard)?;
                match rc {
                    ffi::SQLITE_DONE => {
                        self.state = QueryState::Finished;
                        Ok(None)
                    }
                    ffi::SQLITE_ROW => {
                        self.state = QueryState::Active;
                        Ok(Some(QueryResult::from_statement_mut(self)))
                    }
                    _ => unreachable!(),
                }
            },
            QueryState::Finished => Ok(None),
        }
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_finalize(self.base) };
    }
}

/// A row returned from a query.
#[repr(transparent)]
pub struct QueryResult {
    stmt: Statement,
}

impl QueryResult {
    fn from_statement(stmt: &Statement) -> &Self {
        unsafe { &*(stmt as *const Statement as *const Self) }
    }

    fn from_statement_mut(stmt: &mut Statement) -> &mut Self {
        unsafe { &mut *(stmt as *mut Statement as *mut Self) }
    }

    /// Returns the number of columns in the result.
    pub fn len(&self) -> usize {
        self.stmt.column_count()
    }
}

impl Index<usize> for QueryResult {
    type Output = Column;

    fn index(&self, index: usize) -> &Self::Output {
        &self.stmt.columns[index]
    }
}

impl IndexMut<usize> for QueryResult {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.stmt.columns[index]
    }
}

impl std::fmt::Debug for QueryResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dt = f.debug_tuple("QueryResult");
        for i in 0..self.len() {
            dt.field(&self[i]);
        }
        dt.finish()
    }
}

/// A single value returned from a query.
///
/// SQLite automatically converts between data types on request, which is why many of the
/// methods require `&mut`.
pub struct Column {
    stmt: *mut ffi::sqlite3_stmt,
    position: usize,
}

impl Column {
    fn new(stmt: *mut ffi::sqlite3_stmt, position: usize) -> Self {
        Self { stmt, position }
    }

    pub fn get_unprotected_value(&self) -> UnprotectedValue {
        UnprotectedValue::from_ptr(unsafe {
            ffi::sqlite3_column_value(self.stmt, self.position as _)
        })
    }

    /// Get the bytes of this BLOB value.
    ///
    /// # Safety
    ///
    /// If the type of this value is not BLOB, the behavior of this function is undefined.
    pub unsafe fn get_blob_unchecked(&self) -> &[u8] {
        let len = ffi::sqlite3_column_bytes(self.stmt, self.position as _);
        let data = ffi::sqlite3_column_blob(self.stmt, self.position as _);
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
            let ret = ffi::sqlite3_column_name(self.stmt, self.position as _);
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
            let ret = ffi::sqlite3_column_database_name(self.stmt, self.position as _);
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
            let ret = ffi::sqlite3_column_table_name(self.stmt, self.position as _);
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
            let ret = ffi::sqlite3_column_origin_name(self.stmt, self.position as _);
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
            let ret = ffi::sqlite3_column_decltype(self.stmt, self.position as _);
            if ret.is_null() {
                Ok(None)
            } else {
                Ok(Some(CStr::from_ptr(ret).to_str()?))
            }
        }
    }
}

impl FromValue for Column {
    fn value_type(&self) -> ValueType {
        unsafe { ValueType::from_sqlite(ffi::sqlite3_column_type(self.stmt, self.position as _)) }
    }

    fn get_i32(&self) -> i32 {
        unsafe { ffi::sqlite3_column_int(self.stmt, self.position as _) }
    }

    fn get_i64(&self) -> i64 {
        unsafe { ffi::sqlite3_column_int64(self.stmt, self.position as _) }
    }

    fn get_f64(&self) -> f64 {
        unsafe { ffi::sqlite3_column_double(self.stmt, self.position as _) }
    }

    fn get_blob(&mut self) -> Result<Option<&[u8]>> {
        unsafe {
            let data = ffi::sqlite3_column_blob(self.stmt, self.position as _);
            let len = ffi::sqlite3_column_bytes(self.stmt, self.position as _);
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

impl std::fmt::Debug for Column {
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
