use super::ffi;

#[derive(Debug)]
pub enum Error {
    Sqlite(i32),
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
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
