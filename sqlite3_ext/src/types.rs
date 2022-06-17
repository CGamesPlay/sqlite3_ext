use super::ffi;

#[derive(Debug)]
pub enum Error {
    Sqlite(i32),
    Utf8Error(std::str::Utf8Error),
    OutOfMemory(usize),
    VersionNotSatisfied(std::os::raw::c_int),
    ConstraintViolation,
    Module(String),
}

impl Error {
    pub fn from_sqlite(rc: i32) -> Result<()> {
        match rc {
            ffi::SQLITE_OK | ffi::SQLITE_ROW | ffi::SQLITE_DONE => Ok(()),
            _ => Err(Error::Sqlite(rc)),
        }
    }
}

impl From<Error> for rusqlite::Error {
    fn from(e: Error) -> Self {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ffi::ErrorCode::Unknown,
                extended_code: ffi::SQLITE_ERROR,
            },
            Some(format!("{}", e)),
        )
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Sqlite(i) => write!(f, "SQLite error {}", i),
            Error::Utf8Error(e) => e.fmt(f),
            Error::OutOfMemory(l) => write!(f, "unable to allocate {} bytes", l),
            Error::VersionNotSatisfied(v) => write!(
                f,
                "requires SQLite version {}.{}.{} or above",
                v / 1_000_000,
                (v / 1000) % 1000,
                v % 1000
            ),
            Error::ConstraintViolation => write!(f, "constraint violation"),
            Error::Module(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
