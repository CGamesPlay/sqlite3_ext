//! Rust implementation of the generate_series table-valued function built into SQLite.

use sqlite3_ext::{function::*, vtab::*, *};

struct GenerateSeries {}

impl<'vtab> VTab<'vtab> for GenerateSeries {
    type Aux = ();
    type Cursor = Cursor;

    fn connect(
        _db: &mut Connection,
        _aux: Option<&'vtab Self::Aux>,
        _args: &[&str],
    ) -> Result<(String, Self)> {
        Ok((
            "CREATE TABLE x ( value INTEGER )".to_owned(),
            GenerateSeries {},
        ))
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        todo!()
    }

    fn open(&'vtab mut self) -> Result<Self::Cursor> {
        todo!()
    }
}

struct Cursor {}

impl VTabCursor for Cursor {
    fn filter(&mut self, index_num: usize, index_str: Option<&str>, args: &[&Value]) -> Result<()> {
        todo!()
    }

    fn next(&mut self) -> Result<()> {
        todo!()
    }

    fn eof(&self) -> bool {
        todo!()
    }

    fn column(&self, context: &mut Context, idx: usize) -> Result<()> {
        todo!()
    }

    fn rowid(&self) -> Result<i64> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rusqlite;

    #[test]
    fn usage() {
        println!("generate_srries");
    }
}
