use super::super::types::*;

type InitFn = fn(db: &super::Connection) -> Result<()>;
