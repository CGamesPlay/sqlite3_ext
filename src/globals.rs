use super::{ffi, sqlite3_match_version, sqlite3_require_version, types::*};
use std::{cmp::Ordering, ffi::CStr, str};

/// The version of SQLite.
pub struct SqliteVersion;

/// The version of SQLite. See [SqliteVersion] for details.
pub static SQLITE_VERSION: SqliteVersion = SqliteVersion;

impl SqliteVersion {
    /// Returns the numeric version of SQLite.
    ///
    /// The format of this value is the semantic version with a simple encoding: `major *
    /// 1000000 + minor * 1000 + patch`. For example, SQLite version 3.8.2 is encoded as
    /// `3_008_002`.
    pub fn as_i32(&self) -> i32 {
        unsafe { ffi::sqlite3_libversion_number() }
    }

    /// Returns the human-readable version of SQLite. Example: `"3.8.2"`.
    pub fn as_str(&self) -> &'static str {
        let ret = unsafe { CStr::from_ptr(ffi::sqlite3_libversion()) };
        ret.to_str().expect("sqlite3_libversion")
    }

    /// Returns a hash of the SQLite source code. The objective is to detect accidental and/or
    /// careless edits. A forger can subvert this feature.
    ///
    /// Requires SQLite 3.21.0.
    pub fn sourceid(&self) -> Result<&'static str> {
        sqlite3_require_version!(3_021_000, {
            let ret = unsafe { CStr::from_ptr(ffi::sqlite3_sourceid()) };
            Ok(ret.to_str().expect("sqlite3_sourceid"))
        })
    }
}

impl std::fmt::Display for SqliteVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

/// Perform a case-insensitive comparison using the same collation that SQLite uses.
///
/// This interface was published in SQLite 3.6.17. On earlier versions of SQLite, this method
/// emulates the SQLite behavior.
pub fn sqlite3_stricmp(a: &str, b: &str) -> Ordering {
    sqlite3_match_version! {
        3_006_017 => {
            let rc = unsafe {
                ffi::sqlite3_strnicmp(a.as_ptr() as _, b.as_ptr() as _, std::cmp::min(a.len(), b.len()) as _)
            };
            if rc < 0 {
                Ordering::Less
            } else if rc > 0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        _ => a
            .bytes()
            .zip(b.bytes())
            .find_map(|(a, b)| match a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()) {
                Ordering::Equal => None,
                x => Some(x),
            })
            .unwrap_or(a.len().cmp(&b.len())),
    }
}

/// Perform an SQL [GLOB](https://www.sqlite.org/lang_expr.html#like) operation.
///
/// Requires SQLite 3.7.17.
pub fn sqlite3_strglob(pattern: impl Into<Vec<u8>>, input: impl Into<Vec<u8>>) -> Result<bool> {
    let _ = (&pattern, &input);
    sqlite3_require_version!(3_007_017, {
        let pattern = std::ffi::CString::new(pattern)?;
        let input = std::ffi::CString::new(input)?;
        Ok(unsafe { ffi::sqlite3_strglob(pattern.as_ptr(), input.as_ptr()) == 0 })
    })
}

/// Perform an SQL [LIKE](https://www.sqlite.org/lang_expr.html#like) operation. The escape
/// parameter can be 0 or any ASCII character. If the escape parameter is not ASCII, it is
/// treated as though 0 were specified (no escape).
///
/// Requires SQLite 3.10.0.
pub fn sqlite3_strlike(
    pattern: impl Into<Vec<u8>>,
    input: impl Into<Vec<u8>>,
    escape: impl Into<char>,
) -> Result<bool> {
    let _ = (&pattern, &input, &escape);
    sqlite3_require_version!(3_010_000, {
        let pattern = std::ffi::CString::new(pattern)?;
        let input = std::ffi::CString::new(input)?;
        let escape = escape.into();
        let escape: u32 = if escape.is_ascii() { escape as _ } else { 0 };
        Ok(unsafe { ffi::sqlite3_strlike(pattern.as_ptr(), input.as_ptr(), escape) == 0 })
    })
}

#[cfg(all(test, feature = "static"))]
mod test {
    use super::*;

    #[test]
    fn version() -> Result<()> {
        assert!(SqliteVersion.as_i32() > 3_000_000);
        // "3.0.0" is the shortest posible version string
        assert!(format!("{}", SqliteVersion).len() >= 5);
        sqlite3_match_version! {
            3_021_000 => assert!(SqliteVersion.sourceid()?.len() > 0),
            _ => (),
        }
        Ok(())
    }

    #[test]
    fn strings() -> Result<()> {
        assert_eq!(sqlite3_stricmp("FOO", "bar"), Ordering::Greater);
        assert_eq!(sqlite3_stricmp("bar", "FOO"), Ordering::Less);
        assert_eq!(sqlite3_stricmp("bar", "BAR"), Ordering::Equal);
        sqlite3_match_version! {
            3_007_017 => assert_eq!(sqlite3_strglob("a/**/b", "a/c/d/e/f/b"), Ok(true)),
            _ => (),
        }
        sqlite3_match_version! {
            3_010_000 => {
                assert_eq!(sqlite3_strlike("FOO\\_BAR", "FOO_BAR", '\\'), Ok(true));
                assert_eq!(sqlite3_strlike("FOO_BAR", "FOOXBAR", 0), Ok(true));
            }
            _ => (),
        }
        Ok(())
    }
}
