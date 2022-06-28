use super::ffi;

#[derive(Debug, Clone)]
pub enum Error {
    Sqlite(i32),
    Utf8Error(std::str::Utf8Error),
    VersionNotSatisfied(std::os::raw::c_int),
    Module(String),
    InvalidConversion,
}

impl Error {
    pub fn from_sqlite(rc: i32) -> Result<()> {
        match rc {
            ffi::SQLITE_OK | ffi::SQLITE_ROW | ffi::SQLITE_DONE => Ok(()),
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
            Error::InvalidConversion => write!(f, "invalid conversion"),
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
