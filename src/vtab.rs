use sqlite3_ext::{function::Context, vtab::*, *};

pub struct CrdbVTab {
    data: Vec<i32>,
}

impl<'vtab> CrdbVTab {
    fn connect_create(
        _db: &mut VTabConnection,
        _aux: Option<&'vtab Vec<i32>>,
        _args: &[&str],
    ) -> Result<(String, Self)> {
        Ok((
            "CREATE TABLE x ( value INTEGER NOT NULL )".to_owned(),
            CrdbVTab {
                data: vec![1, 2, 3, 4, 5],
            },
        ))
    }
}

impl<'vtab> VTab<'vtab> for CrdbVTab {
    type Aux = Vec<i32>;
    type Cursor = StandardCursor<'vtab>;

    fn connect(
        db: &mut VTabConnection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create(db, aux, args)
    }

    fn best_index(&self, _index_info: &mut IndexInfo) -> Result<()> {
        Ok(())
    }

    fn open(&'vtab mut self) -> Result<Self::Cursor> {
        Ok(StandardCursor {
            iter: self.data.iter(),
            current: None,
        })
    }
}

impl<'vtab> CreateVTab<'vtab> for CrdbVTab {
    fn create(
        db: &mut VTabConnection,
        aux: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create(db, aux, args)
    }

    fn destroy(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct StandardCursor<'vtab> {
    iter: std::slice::Iter<'vtab, i32>,
    current: Option<i32>,
}

impl VTabCursor for StandardCursor<'_> {
    fn filter(
        &mut self,
        _index_num: usize,
        _index_str: Option<&str>,
        _args: &[&Value],
    ) -> Result<()> {
        self.current = self.iter.next().copied();
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        self.current = self.iter.next().copied();
        Ok(())
    }

    fn eof(&self) -> bool {
        match self.current {
            Some(_) => false,
            None => true,
        }
    }

    fn column(&self, context: &mut Context, _i: usize) -> Result<()> {
        if let Some(i) = self.current {
            context.set_result(i);
        }
        Ok(())
    }

    fn rowid(&self) -> Result<i64> {
        self.current
            .map(|i| i as _)
            .ok_or(Error::Sqlite(ffi::SQLITE_MISUSE))
    }
}
