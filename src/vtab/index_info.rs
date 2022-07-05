use crate::{ffi, sqlite3_require_version, stack_ref, types::*};
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
/// best query plan, and then set the results using
/// [constraint_usage_mut](Self::constraint_usage_mut),
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
    pub fn distinct_mode(&self) -> DistinctMode {
        let ret = unsafe { ffi::sqlite3_vtab_distinct(&self.base as *const _ as _) };
        DistinctMode::from_sqlite(ret)
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

    /// Requires SQLite 3.8.2.
    pub fn set_estimated_rows(&mut self, val: i64) -> Result<()> {
        let _ = val;
        sqlite3_require_version!(3_008_002, {
            self.base.estimatedRows = val;
            Ok(())
        })
    }

    /// Retrieve the value previously set by
    /// [set_scan_flags](Self::set_scan_flags).
    ///
    /// Requires SQLite 3.9.0.
    pub fn scan_flags(&self) -> Result<usize> {
        sqlite3_require_version!(3_009_000, Ok(self.base.idxFlags as _))
    }

    /// Requires SQLite 3.9.0.
    pub fn set_scan_flags(&mut self, val: usize) -> Result<()> {
        let _ = val;
        sqlite3_require_version!(3_009_000, {
            self.base.idxFlags = val as _;
            Ok(())
        })
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

    /// 0-based index of columns. The rowid column is index -1.
    pub fn column(&self) -> i32 {
        self.constraint().iColumn as _
    }

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

    /// Retrieve the value prevoiusly set using [set_argv_index](Self::set_argv_index).
    pub fn argv_index(&self) -> Option<u32> {
        match self.usage().argvIndex {
            0 => None,
            x => Some((x - 1) as _),
        }
    }

    /// Set the desired index for [filter](super::VTabCursor::filter)'s argv.
    ///
    /// Exactly one entry in the IndexInfo should be set to 1, another to 2, another to 3,
    /// and so forth up to as many or as few as the best_index method wants. The EXPR of
    /// the corresponding constraints will then be passed in as the argv[] parameters to
    /// filter.
    ///
    /// Notice that the idx values specified here are 1-based, but array passed to filter
    /// will use 0-based indexing. The value 0 is used to indicate that the constraint is
    /// not needed by the filter function.
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

#[derive(Debug, PartialEq, Clone, Copy)]
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
#[derive(Debug, PartialEq, Clone, Copy)]
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
        f.debug_struct("IndexInfo")
            .field(
                "constraints",
                &self.constraints().collect::<Vec<_>>().as_slice(),
            )
            .field("order_by", &self.order_by().collect::<Vec<_>>().as_slice())
            .field("index_num", &self.index_num())
            .field("index_str", &self.index_str())
            .field("order_by_consumed", &self.order_by_consumed())
            .field("estimated_cost", &self.estimated_cost())
            .field("estimated_rows", &self.estimated_rows())
            .field("scan_flags", &self.scan_flags())
            .field("columns_used", &self.columns_used())
            .finish()
    }
}

impl std::fmt::Debug for IndexInfoConstraint<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfoConstraint")
            .field("column", &self.column())
            .field("op", &self.op())
            .field("usable", &self.usable())
            .field("argv_index", &self.argv_index())
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
