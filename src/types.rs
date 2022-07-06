use super::ffi;

#[derive(Debug, Clone)]
pub enum Error {
    Sqlite(i32),
    Utf8Error(std::str::Utf8Error),
    VersionNotSatisfied(std::os::raw::c_int),
    Module(String),
    NotFound,
    /// The result was not necessary to produce because it is an unchanged column in an
    /// UPDATE operation. See [ValueRef::nochange](crate::ValueRef::nochange) for details.
    #[cfg(modern_sqlite)]
    NoChange,
}

impl Error {
    pub fn from_sqlite(rc: i32) -> Result<()> {
        match rc {
            ffi::SQLITE_OK | ffi::SQLITE_ROW | ffi::SQLITE_DONE => Ok(()),
            ffi::SQLITE_NOTFOUND => Err(Error::NotFound),
            _ => Err(Error::Sqlite(rc)),
        }
    }

    pub fn constraint_violation() -> Error {
        Error::Sqlite(ffi::SQLITE_CONSTRAINT)
    }

    pub fn no_memory() -> Error {
        Error::Sqlite(ffi::SQLITE_NOMEM)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Sqlite(i) => write!(f, "SQLite error {}", i),
            Error::Utf8Error(e) => e.fmt(f),
            Error::Module(s) => write!(f, "{}", s),
            Error::VersionNotSatisfied(v) => write!(
                f,
                "requires SQLite version {}.{}.{} or above",
                v / 1_000_000,
                (v / 1000) % 1000,
                v % 1000
            ),
            Error::NotFound => write!(f, "not found"),
            #[cfg(modern_sqlite)]
            Error::NoChange => write!(f, "invalid Error::NoChange"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Self {
        Self::Utf8Error(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
