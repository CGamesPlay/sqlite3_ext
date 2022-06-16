use super::ffi;

#[derive(Debug)]
pub enum Error {
    Sqlite(i32),
    OutOfMemory(usize),
    VersionNotSatisfied(std::os::raw::c_int),
    ConstraintViolation,
}

impl Error {
    pub fn from_sqlite(rc: i32) -> Result<()> {
        match rc {
            ffi::SQLITE_OK => Ok(()),
            _ => Err(Error::Sqlite(rc)),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Sqlite(_) => todo!(),
            Error::OutOfMemory(l) => write!(f, "unable to allocate {} bytes", l),
            Error::VersionNotSatisfied(v) => write!(
                f,
                "requires SQLite version {}.{}.{} or above",
                v / 1_000_000,
                (v / 1000) % 1000,
                v % 1000
            ),
            Error::ConstraintViolation => write!(f, "constraint violation"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
