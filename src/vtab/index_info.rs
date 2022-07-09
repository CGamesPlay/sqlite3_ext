use crate::{ffi, sqlite3_match_version, sqlite3_require_version, stack_ref, types::*, value::*};
use std::{ffi::CStr, ptr};

stack_ref!(static CURRENT_INDEX_INFO: &*const ffi::sqlite3_index_info);

/// Information about a query plan.
///
/// This struct contains all of the information about a query plan that SQLite is attempting on
/// the virtual table. The struct will be passed to
/// [VTab::best_index](super::VTab::best_index).
///
/// This struct is both an input and an output. The virtual table implementation should examine
/// the [constraints](Self::constraints) and [order_by](Self::order_by) fields, decide on the
/// best query plan, and then set the results using [IndexInfoConstraint::set_argv_index],
/// [set_estimated_cost](Self::set_estimated_cost), and the other methods.
#[repr(transparent)]
pub struct IndexInfo {
    base: ffi::sqlite3_index_info,
}

impl IndexInfo {
    pub(crate) fn with_current<F: FnOnce(&mut Self) -> R, R>(&mut self, f: F) -> R {
        CURRENT_INDEX_INFO.with_value(&(&self.base as *const _), || f(self))
    }

    pub fn constraints(&self) -> IndexInfoConstraintIterator {
        IndexInfoConstraintIterator::new(self)
    }

    pub fn order_by(&self) -> IndexInfoOrderByIterator {
        IndexInfoOrderByIterator::new(self)
    }

    /// Determine if a query is DISTINCT.
    ///
    /// Requires SQLite 3.38.0. On earlier versions, this function will always return
    /// [DistinctMode::Ordered].
    pub fn distinct_mode(&self) -> DistinctMode {
        sqlite3_match_version! {
            3_038_000 => {
                let ret = unsafe { ffi::sqlite3_vtab_distinct(&self.base as *const _ as _) };
                DistinctMode::from_sqlite(ret)
            },
            _ => DistinctMode::Ordered,
        }
    }

    /// Retrieve the value previously set by
    /// [set_index_num](Self::set_index_num).
    pub fn index_num(&self) -> i32 {
        self.base.idxNum
    }

    /// Set the index number of this query plan. This is an arbitrary value which will be
    /// passed to [VTabCursor::filter](super::VTabCursor::filter).
    pub fn set_index_num(&mut self, val: i32) {
        self.base.idxNum = val;
    }

    /// Retrieve the value previously set by
    /// [set_index_str](Self::set_index_str).
    pub fn index_str(&self) -> Option<&str> {
        if self.base.idxStr.is_null() {
            None
        } else {
            let cstr = unsafe { CStr::from_ptr(self.base.idxStr) };
            cstr.to_str().ok()
        }
    }

    /// Set the index string of this query plan. This is an arbitrary value which will be
    /// passed to [VTabCursor::filter](super::VTabCursor::filter).
    ///
    /// This function can fail if SQLite is not able to allocate memory for the string.
    pub fn set_index_str(&mut self, val: Option<&str>) -> Result<()> {
        if self.base.needToFreeIdxStr != 0 {
            unsafe { ffi::sqlite3_free(self.base.idxStr as _) };
        }
        match val {
            None => {
                self.base.idxStr = ptr::null_mut();
                self.base.needToFreeIdxStr = 0;
            }
            Some(x) => {
                self.base.idxStr = ffi::str_to_sqlite3(x)?;
                self.base.needToFreeIdxStr = 1;
            }
        }
        Ok(())
    }

    /// Set the index string without copying.
    pub fn set_index_str_static(&mut self, val: &'static CStr) {
        if self.base.needToFreeIdxStr != 0 {
            unsafe { ffi::sqlite3_free(self.base.idxStr as _) };
        }
        self.base.idxStr = val.as_ptr() as _;
        self.base.needToFreeIdxStr = 0;
    }

    /// Retrieve the value previously set by
    /// [set_order_by_consumed](Self::set_order_by_consumed).
    pub fn order_by_consumed(&self) -> bool {
        self.base.orderByConsumed != 0
    }

    /// Indicate that the virtual table fully understands the requirements of the
    /// [order_by](Self::order_by) and [distinct_mode](Self::distinct_mode) fields. If this
    /// is the case, then SQLite can omit reordering the results of the query, which may
    /// improve performance. It is never necessary to use the order_by information, but
    /// virtual tables may opt to use it as a performance optimization.
    pub fn set_order_by_consumed(&mut self, val: bool) {
        self.base.orderByConsumed = val as _;
    }

    /// Retrieve the value previously set by
    /// [set_estimated_cost](Self::set_estimated_cost).
    pub fn estimated_cost(&self) -> f64 {
        self.base.estimatedCost
    }

    pub fn set_estimated_cost(&mut self, val: f64) {
        self.base.estimatedCost = val;
    }

    /// Retrieve the value previously set by
    /// [set_estimated_rows](Self::set_estimated_rows).
    ///
    /// Requires SQLite 3.8.2.
    pub fn estimated_rows(&self) -> Result<i64> {
        sqlite3_require_version!(3_008_002, Ok(self.base.estimatedRows))
    }

    /// Requires SQLite 3.8.2. On earlier versions of SQLite, this function is a harmless
    /// no-op.
    pub fn set_estimated_rows(&mut self, val: i64) {
        let _ = val;
        sqlite3_match_version! {
            3_008_220 => self.base.estimatedRows = val,
            _ => (),
        }
    }

    /// Retrieve the value previously set by
    /// [set_scan_flags](Self::set_scan_flags).
    ///
    /// Requires SQLite 3.9.0.
    pub fn scan_flags(&self) -> Result<usize> {
        sqlite3_require_version!(3_009_000, Ok(self.base.idxFlags as _))
    }

    /// Requires SQLite 3.9.0. On earlier versions of SQLite, this function is a harmless
    /// no-op.
    pub fn set_scan_flags(&mut self, val: usize) -> () {
        let _ = val;
        sqlite3_match_version! {
            3_009_000 => self.base.idxFlags = val as _,
            _ => (),
        }
    }

    /// Requires SQLite 3.10.0.
    pub fn columns_used(&self) -> Result<u64> {
        sqlite3_require_version!(3_010_000, Ok(self.base.colUsed))
    }
}

#[derive(Copy, Clone)]
pub struct IndexInfoConstraint<'a> {
    index_info: &'a IndexInfo,
    position: usize,
}

impl IndexInfoConstraint<'_> {
    fn constraint(&self) -> &ffi::sqlite3_index_info_sqlite3_index_constraint {
        unsafe { &*self.index_info.base.aConstraint.offset(self.position as _) }
    }

    fn usage(&self) -> &mut ffi::sqlite3_index_info_sqlite3_index_constraint_usage {
        unsafe {
            &mut *self
                .index_info
                .base
                .aConstraintUsage
                .offset(self.position as _)
        }
    }

    /// Return the column being constrained. The value is a 0-based index of columns as declared by
    /// [connect](super::VTab::connect) / [create](super::CreateVTab::create). The rowid column is
    /// index -1.
    pub fn column(&self) -> i32 {
        self.constraint().iColumn as _
    }

    /// Return the type of constraint.
    pub fn op(&self) -> ConstraintOp {
        ConstraintOp::from_sqlite(self.constraint().op)
    }

    /// [IndexInfo::constraints] contains information about all constraints that apply to
    /// the virtual table, but some of the constraints might not be usable because of the
    /// way tables are ordered in a join. The best_index method must therefore only
    /// consider constraints that for which this method returns true.
    pub fn usable(&self) -> bool {
        self.constraint().usable != 0
    }

    /// Returns the right-hand side of the constraint.
    ///
    /// This routine attempts to retrieve the value of the right-hand operand of the constraint if
    /// that operand is known. If the operand is not known, then Err([SQLITE_NOTFOUND]) is returned.
    /// This method can return another error type if something goes wrong.
    ///
    /// This method is usually only successful if the right-hand operand of a constraint is a
    /// literal value in the original SQL statement. If the right-hand operand is an expression or
    /// a reference to some other column or a host parameter, then this method will probably return
    /// Err(SQLITE_NOTFOUND).
    ///
    /// Some constraints, such as [ConstraintOp::IsNull], have no right-hand operand. For such
    /// constraints, this method always returns Err(SQLITE_NOTFOUND).
    ///
    /// Requires SQLite 3.38.0. On earlier versions of SQLite, Err(SQLITE_NOTFOUND) is always
    /// returned.
    pub fn rhs(&self) -> Result<&ValueRef> {
        sqlite3_match_version! {
            3_038_000 => unsafe {
                let mut ret: *mut ffi::sqlite3_value = ptr::null_mut();
                Error::from_sqlite(ffi::sqlite3_vtab_rhs_value(
                    &self.index_info.base as *const _ as _,
                    self.position as _,
                    &mut ret,
                ))?;
                Ok(&*(ret as *const crate::value::ValueRef))
            },
            _ => Err(SQLITE_NOTFOUND),
        }
    }

    /// Return the collation to use for text comparisons on this column.
    ///
    /// See [the SQLite documentation](https://www.sqlite.org/c3ref/vtab_collation.html)
    /// for more details.
    pub fn collation(&self) -> Result<&str> {
        sqlite3_require_version!(3_022_000, {
            let ret = unsafe {
                CStr::from_ptr(ffi::sqlite3_vtab_collation(
                    &self.index_info.base as *const _ as _,
                    self.position as _,
                ))
            };
            Ok(ret.to_str()?)
        })
    }

    /// Retrieve the value previously set using [set_argv_index](Self::set_argv_index).
    pub fn argv_index(&self) -> Option<u32> {
        match self.usage().argvIndex {
            0 => None,
            x => Some((x - 1) as _),
        }
    }

    /// Set the desired index for [filter](super::VTabCursor::filter)'s argv.
    ///
    /// Exactly one entry in the IndexInfo should be set to 0, another to 1, another to 2,
    /// and so forth up to as many or as few as the best_index method wants. The EXPR of
    /// the corresponding constraints will then be passed in as the argv[] parameters to
    /// filter.
    pub fn set_argv_index(&mut self, idx: Option<u32>) {
        self.usage().argvIndex = match idx {
            None => 0,
            Some(i) => (i + 1) as _,
        };
    }

    /// Retrieve the value previously set by [set_omit](Self::set_omit).
    pub fn omit(&self) -> bool {
        self.usage().omit != 0
    }

    /// Advise SQLite that this constraint is validated by the virtual table
    /// implementation. SQLite may skip performing its own check in some cases. It is
    /// generally a hint and not a requirement, but a notable exception is for
    /// [ConstraintOp::Offset], which is always honored. See [the SQLite
    /// documentation](https://www.sqlite.org/vtab.html#omit_constraint_checking_in_bytecode)
    /// for more details.
    pub fn set_omit(&mut self, val: bool) {
        self.usage().omit = val as _;
    }

    /// Check if all values in this IN constraint are able to be processed simultaneously.
    /// If this method returns true, then a call to
    /// [set_value_list_wanted](Self::set_value_list_wanted) would also return true.
    ///
    /// Requires SQLite 3.38.0. On earlier versions, this function will always return
    /// false.
    pub fn value_list_available(&self) -> bool {
        sqlite3_match_version! {
            3_038_000 => unsafe {
                ffi::sqlite3_vtab_in(
                    &self.index_info.base as *const _ as _,
                    self.position as _,
                    -1,
                ) != 0
            },
            _ => false,
        }
    }

    /// Instruct SQLite to return all values in an IN constraint simultaneously.
    ///
    /// A constraint on a virtual table in the form of "column IN (...)" is communicated to
    /// [VTab::best_index](super::VTab::best_index) as a [ConstraintOp::Eq] constraint. If
    /// the virtual table wants to use this constraint, it must use
    /// [set_argv_index](Self::set_argv_index) to assign the constraint to an argument.
    /// Then, SQLite will invoke [VTabCursor::filter](super::VTabCursor::filter) once for
    /// each value on the right-hand side of the IN operator. Thus, the virtual table only
    /// sees a single value from the right-hand side of the IN operator at a time.
    ///
    /// In some cases, however, it would be advantageous for the virtual table to see all values on the right-hand of the IN operator all at once. This method enables this feature.
    ///
    /// Calling this method with true will request [ValueList](crate::ValueList)
    /// processing. In order for ValueList processing to work:
    ///
    /// 1. this constraint must be assigned an argv index using
    /// [set_argv_index](Self::set_argv_index);
    /// 2. this method is called with true; and
    /// 3. SQLite is able to provide all values simultaneously.
    ///
    /// If all of these criteria are met, then the corresponding argument passed to
    /// [VTabCursor::filter](super::VTabCursor::filter) will appear to be SQL NULL, but
    /// accessible using [ValueList](crate::ValueList). If this facility is requested but
    /// this method returns false, then VTabCursor::filter will be invoked multiple times
    /// with each different value of the constraint, as normal.
    ///
    /// This method always returns the same value that
    /// [value_list_available](Self::value_list_available) would. Calling this method with
    /// false cancels a previous request for a ValueList.
    ///
    /// See [the SQLite documentation](https://www.sqlite.org/c3ref/vtab_in.html) for more
    /// details.
    ///
    /// Requires SQLite 3.38.0. On earlier versions, this function will always return
    /// false.
    pub fn set_value_list_wanted(&mut self, val: bool) -> bool {
        let _ = val;
        sqlite3_match_version! {
            3_038_000 => unsafe {
                ffi::sqlite3_vtab_in(
                    &self.index_info.base as *const _ as _,
                    self.position as _,
                    if val { 1 } else { 0 },
                ) != 0
            },
            _ => false,
        }
    }
}

#[derive(Copy, Clone)]
pub struct IndexInfoOrderBy<'a> {
    index_info: &'a IndexInfo,
    position: usize,
}

impl IndexInfoOrderBy<'_> {
    fn base(&self) -> &ffi::sqlite3_index_info_sqlite3_index_orderby {
        unsafe { &*self.index_info.base.aOrderBy.offset(self.position as _) }
    }

    pub fn column(&self) -> i32 {
        self.base().iColumn as _
    }

    pub fn desc(&self) -> bool {
        self.base().desc != 0
    }
}

macro_rules! make_iterator(
    ($t:ident, $n:ident) => {
        paste::paste! {
            pub struct [<$t Iterator>]<'a> {
                current: $t<'a>,
            }

            impl<'a> [<$t Iterator>]<'a> {
                fn new(index_info: &'a IndexInfo) -> Self {
                    Self {
                        current: $t {
                            index_info,
                            position: usize::MAX,
                        },
                    }
                }
            }

            impl<'a> Iterator for [<$t Iterator>]<'a> {
                type Item = $t<'a>;

                fn next(&mut self) -> Option<Self::Item> {
                    let pos = self.current.position.wrapping_add(1);
                    if pos < self.current.index_info.base.$n as usize {
                        self.current.position = pos;
                        Some(self.current)
                    } else {
                        None
                    }
                }

                fn size_hint(&self) -> (usize, Option<usize>) {
                    let remaining = self.current.index_info.base.$n as usize
                        - self.current.position.wrapping_add(1);
                    (remaining, Some(remaining))
                }
            }
        }
    }
);

make_iterator!(IndexInfoConstraint, nConstraint);
make_iterator!(IndexInfoOrderBy, nOrderBy);

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum ConstraintOp {
    Eq,
    GT,
    LE,
    LT,
    GE,
    Match,
    Like,         /* 3.10.0 and later */
    Glob,         /* 3.10.0 and later */
    Regexp,       /* 3.10.0 and later */
    NE,           /* 3.21.0 and later */
    IsNot,        /* 3.21.0 and later */
    IsNotNull,    /* 3.21.0 and later */
    IsNull,       /* 3.21.0 and later */
    Is,           /* 3.21.0 and later */
    Limit,        /* 3.38.0 and later */
    Offset,       /* 3.38.0 and later */
    Function(u8), /* 3.25.0 and later */
}

impl ConstraintOp {
    pub(crate) fn assert_valid_function_constraint(&self) {
        if let ConstraintOp::Function(val) = *self {
            if val >= 150 {
                return;
            }
        }
        panic!("invalid function constraint")
    }

    fn from_sqlite(val: u8) -> ConstraintOp {
        match val as _ {
            2 => ConstraintOp::Eq,
            4 => ConstraintOp::GT,
            8 => ConstraintOp::LE,
            16 => ConstraintOp::LT,
            32 => ConstraintOp::GE,
            64 => ConstraintOp::Match,
            65 => ConstraintOp::Like,
            66 => ConstraintOp::Glob,
            67 => ConstraintOp::Regexp,
            68 => ConstraintOp::NE,
            69 => ConstraintOp::IsNot,
            70 => ConstraintOp::IsNotNull,
            71 => ConstraintOp::IsNull,
            72 => ConstraintOp::Is,
            73 => ConstraintOp::Limit,
            74 => ConstraintOp::Offset,
            150..=255 => ConstraintOp::Function(val),
            _ => panic!("invalid constraint op"),
        }
    }
}

/// Describes the requirements of the virtual table query.
///
/// This value is retured by [IndexInfo::distinct_mode]. It allows the virtual table
/// implementation to decide if it is safe to consume the [order_by](IndexInfo::order_by)
/// fields using [IndexInfo::set_order_by_consumed].
///
/// The levels described here are progressively less demanding. If the virtual table
/// implementation meets the requirements of [DistinctMode::Ordered], then it is always safe to
/// consume the order_by fields.
///
/// For the purposes of comparing virtual table output values to see if the values are same
/// value for sorting purposes, two NULL values are considered to be the same. In other words,
/// the comparison operator is "IS" (or "IS NOT DISTINCT FROM") and not "==".
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum DistinctMode {
    /// The virtual table must return all rows in the correct order according to the
    /// [order_by](IndexInfo::order_by) fields.
    Ordered,
    /// The virtual table may return rows in any order, but all rows that are in the same
    /// group must be adjacent to one another. A group is defined as all rows for which all
    /// of the columns in [order_by](IndexInfo::order_by) are equal. This is the mode used
    /// when planning a GROUP BY query.
    Grouped,
    /// The same as [DistinctMode::Grouped], however the virtual table is allowed (but not
    /// required) to skip all but a single row within each group. This is the mode used
    /// when planning a DISTINCT query.
    Distinct,
}

impl DistinctMode {
    #[cfg(modern_sqlite)]
    fn from_sqlite(val: i32) -> Self {
        match val {
            0 => Self::Ordered,
            1 => Self::Grouped,
            2 => Self::Distinct,
            _ => panic!("invalid distinct mode"),
        }
    }
}

impl std::fmt::Debug for IndexInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let mut ds = f.debug_struct("IndexInfo");
        ds.field(
            "constraints",
            &self.constraints().collect::<Vec<_>>().as_slice(),
        )
        .field("order_by", &self.order_by().collect::<Vec<_>>().as_slice())
        .field("index_num", &self.index_num())
        .field("index_str", &self.index_str())
        .field("order_by_consumed", &self.order_by_consumed())
        .field("estimated_cost", &self.estimated_cost());
        self.estimated_rows()
            .map(|v| ds.field("estimated_rows", &v))
            .ok();
        self.scan_flags().map(|v| ds.field("scan_flags", &v)).ok();
        self.columns_used()
            .map(|v| ds.field("columns_used", &v))
            .ok();
        ds.finish()
    }
}

impl std::fmt::Debug for IndexInfoConstraint<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let mut ds = f.debug_struct("IndexInfoConstraint");
        ds.field("column", &self.column())
            .field("op", &self.op())
            .field("usable", &self.usable());
        sqlite3_match_version! {
            3_038_000 => {
                ds.field("rhs", &self.rhs());
            }
            _ => (),
        }
        sqlite3_match_version! {
            3_022_000 => {
                ds.field("collation", &self.collation());
            }
            _ => (),
        }
        ds.field("argv_index", &self.argv_index())
            .field("omit", &self.omit())
            .finish()
    }
}

impl std::fmt::Debug for IndexInfoOrderBy<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfoOrderBy")
            .field("column", &self.column())
            .field("desc", &self.desc())
            .finish()
    }
}
