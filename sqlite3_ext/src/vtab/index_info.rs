use super::super::{ffi, types::*};
use std::{ffi::CStr, ptr, slice};

#[repr(transparent)]
pub struct IndexInfo {
    base: ffi::sqlite3_index_info,
}

#[repr(transparent)]
pub struct IndexInfoConstraint {
    base: ffi::sqlite3_index_info_sqlite3_index_constraint,
}

#[repr(transparent)]
pub struct IndexInfoOrderBy {
    base: ffi::sqlite3_index_info_sqlite3_index_orderby,
}

#[repr(transparent)]
pub struct IndexInfoConstraintUsage {
    base: ffi::sqlite3_index_info_sqlite3_index_constraint_usage,
}

impl IndexInfo {
    pub fn constraints(&self) -> &[IndexInfoConstraint] {
        unsafe {
            slice::from_raw_parts(
                self.base.aConstraint as *const IndexInfoConstraint,
                self.base.nConstraint as _,
            )
        }
    }

    pub fn order_by(&self) -> &[IndexInfoOrderBy] {
        unsafe {
            slice::from_raw_parts(
                self.base.aOrderBy as *const IndexInfoOrderBy,
                self.base.nOrderBy as _,
            )
        }
    }

    pub fn constraint_usage(&self) -> &[IndexInfoConstraintUsage] {
        unsafe {
            slice::from_raw_parts(
                self.base.aConstraintUsage as *const IndexInfoConstraintUsage,
                self.base.nConstraint as _,
            )
        }
    }

    pub fn constraint_usage_mut(&mut self) -> &mut [IndexInfoConstraintUsage] {
        unsafe {
            slice::from_raw_parts_mut(
                self.base.aConstraintUsage as *mut IndexInfoConstraintUsage,
                self.base.nConstraint as _,
            )
        }
    }

    pub fn index_num(&self) -> usize {
        self.base.idxNum as _
    }

    pub fn set_index_num(&mut self, val: usize) {
        self.base.idxNum = val as _;
    }

    pub fn index_str(&self) -> Option<&str> {
        if self.base.idxStr.is_null() {
            None
        } else {
            let cstr = unsafe { CStr::from_ptr(self.base.idxStr) };
            cstr.to_str().ok()
        }
    }

    /// Set the index string to the provided value. This function can fail if SQLite is not
    /// able to allocate memory for the string.
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

    pub fn order_by_consumed(&self) -> bool {
        self.base.orderByConsumed != 0
    }

    pub fn set_order_by_consumed(&mut self, val: bool) {
        self.base.orderByConsumed = val as _;
    }

    pub fn estimated_cost(&self) -> f64 {
        self.base.estimatedCost
    }

    pub fn set_estimated_cost(&mut self, val: f64) {
        self.base.estimatedCost = val;
    }

    /// Requires SQLite 3.8.2.
    pub fn estimated_rows(&self) -> Result<i64> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_008_002)?;
                Ok(self.base.estimatedRows)
            },
            _ => Err(Error::VersionNotSatisfied(3_008_002))
        }
    }

    /// Requires SQLite 3.8.2.
    pub fn set_estimated_rows(&mut self, val: i64) -> Result<()> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_008_002)?;
                self.base.estimatedRows = val;
                Ok(())
            },
            _ => {
                let _ = val;
                Err(Error::VersionNotSatisfied(3_008_002))
            }
        }
    }

    /// Requires SQLite 3.9.0.
    pub fn scan_flags(&self) -> Result<usize> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_009_000)?;
                Ok(self.base.idxFlags as _)
            },
            _ => Err(Error::VersionNotSatisfied(3_009_000))
        }
    }

    /// Requires SQLite 3.9.0.
    pub fn set_scan_flags(&mut self, val: usize) -> Result<()> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_009_000)?;
                self.base.idxFlags = val as _;
                Ok(())
            },
            _ => {
                let _ = val;
                Err(Error::VersionNotSatisfied(3_009_000))
            }
        }
    }

    /// Requires SQLite 3.10.0.
    pub fn columns_used(&self) -> Result<u64> {
        ffi::match_sqlite! {
            modern => {
                ffi::require_version(3_010_000)?;
                Ok(self.base.colUsed)
            },
            _ => Err(Error::VersionNotSatisfied(3_010_000))
        }
    }
}

impl IndexInfoConstraint {
    pub fn column(&self) -> isize {
        self.base.iColumn as _
    }

    pub fn op(&self) -> u8 {
        self.base.op
    }

    pub fn usable(&self) -> bool {
        self.base.usable != 0
    }
}

impl IndexInfoOrderBy {
    pub fn column(&self) -> isize {
        self.base.iColumn as _
    }

    pub fn desc(&self) -> bool {
        self.base.desc != 0
    }
}

impl IndexInfoConstraintUsage {
    pub fn argv_index(&self) -> usize {
        self.base.argvIndex as _
    }

    pub fn omit(&self) -> bool {
        self.base.omit != 0
    }
}

impl std::fmt::Debug for IndexInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfo")
            .field("constraints", &self.constraints())
            .field("order_by", &self.order_by())
            .field("constraint_usage", &self.constraint_usage())
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

impl std::fmt::Debug for IndexInfoConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfoConstraint")
            .field("column", &self.column())
            .field("op", &self.op())
            .field("usable", &self.usable())
            .finish()
    }
}

impl std::fmt::Debug for IndexInfoOrderBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfoOrderBy")
            .field("column", &self.column())
            .field("desc", &self.desc())
            .finish()
    }
}

impl std::fmt::Debug for IndexInfoConstraintUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("IndexInfoConstraintUsage")
            .field("argv_index", &self.argv_index())
            .field("omit", &self.omit())
            .finish()
    }
}
