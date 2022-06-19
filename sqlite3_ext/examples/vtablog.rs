//! Rust implementation of the vtablog virtual table.
//!
//! For more information, consult [the original implementation](https://sqlite.org/src/file/ext/misc/vtablog.c).

use sqlite3_ext::{function::*, vtab::*, *};
use std::sync::atomic::{AtomicUsize, Ordering};

enum VTabArg {
    Schema(String),
    NumRows(i64),
}

mod parsing {
    use super::VTabArg;
    use nom::{
        branch::alt,
        bytes::complete::{is_not, tag},
        character::complete::i64,
        combinator::{eof, map},
        multi::many0,
        sequence::{terminated, tuple},
        IResult,
    };
    use sqlite3_ext::*;

    pub(super) fn parse_arg(input: &str) -> Result<VTabArg> {
        let ret: IResult<&str, VTabArg> = terminated(
            alt((
                map(tuple((tag("rows="), i64)), |(_, s): (&str, i64)| {
                    VTabArg::NumRows(s)
                }),
                map(
                    tuple((
                        tag("schema='"),
                        many0(alt((is_not("'"), tag("''")))),
                        tag("'"),
                    )),
                    |(_, s, _): (&str, Vec<&str>, &str)| VTabArg::Schema(s.join("")),
                ),
            )),
            eof,
        )(input);
        match ret {
            Ok((_, arg)) => Ok(arg),
            Err(e) => Err(Error::Module(format!("{}", e))),
        }
    }
}

struct VTabLog {
    id: usize,
    num_rows: i64,
    num_cursors: usize,
}

struct VTabLogCursor<'vtab> {
    vtab: &'vtab VTabLog,
    id: usize,
    rowid: i64,
}

impl VTabLog {
    fn connect_create(args: &[&str], method: &str) -> Result<(String, Self)> {
        static N_INST: AtomicUsize = AtomicUsize::new(100);
        let id = N_INST.fetch_add(100, Ordering::SeqCst);

        println!("{}(tab={}, args={:?})", method, id, args);

        let mut num_rows = 0;
        let mut schema = None;

        let opts: Vec<VTabArg> = args[3..]
            .iter()
            .map(|a| Ok(parsing::parse_arg(a)?))
            .collect::<Result<_>>()?;
        for o in opts {
            match o {
                VTabArg::Schema(s) => schema = Some(s),
                VTabArg::NumRows(r) => num_rows = r,
            }
        }

        let schema = schema.ok_or_else(|| Error::Module("schema not provided".to_owned()))?;

        Ok((
            schema,
            VTabLog {
                id,
                num_rows,
                num_cursors: 0,
            },
        ))
    }
}

impl<'vtab> VTab<'vtab> for VTabLog {
    type Aux = ();
    type Cursor = VTabLogCursor<'vtab>;

    fn connect(
        _: &'vtab mut VTabConnection,
        _: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create(args, "connect")
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        println!("best_index(tab={}, index_info={:?})", self.id, index_info);
        Ok(())
    }

    fn open(&'vtab mut self) -> Result<Self::Cursor> {
        self.num_cursors += 1;
        let ret = VTabLogCursor {
            vtab: self,
            id: self.id + self.num_cursors,
            rowid: 0,
        };
        println!("open(tab={}, cursor={})", self.id, ret.id);
        Ok(ret)
    }
}

impl<'vtab> CreateVTab<'vtab> for VTabLog {
    fn create(
        _: &'vtab mut VTabConnection,
        _: Option<&'vtab Self::Aux>,
        args: &[&str],
    ) -> Result<(String, Self)> {
        Self::connect_create(args, "create")
    }

    fn destroy(&mut self) -> Result<()> {
        println!("destroy(tab={})", self.id);
        Ok(())
    }
}

impl Drop for VTabLog {
    fn drop(&mut self) {
        println!("drop(tab={})", self.id);
    }
}

impl VTabCursor for VTabLogCursor<'_> {
    fn filter(&mut self, _: usize, _: Option<&str>, args: &[&Value]) -> Result<()> {
        println!(
            "filter(tab={}, cursor={}, args={:?})",
            self.vtab.id, self.id, args
        );
        self.rowid = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        println!(
            "next(tab={}, cursor={})\n  rowid {} -> {}",
            self.vtab.id,
            self.id,
            self.rowid,
            self.rowid + 1
        );
        self.rowid += 1;
        Ok(())
    }

    fn eof(&self) -> bool {
        let ret = self.rowid >= self.vtab.num_rows;
        println!("eof(tab={}, cursor={}) -> {}", self.vtab.id, self.id, ret);
        ret
    }

    fn column(&self, context: &mut Context, idx: usize) -> Result<()> {
        const ALPHABET: &[u8] = "abcdefghijklmnopqrstuvwxyz".as_bytes();
        let ret = ALPHABET
            .get(idx)
            .map(|l| format!("{}{}", *l as char, self.rowid))
            .unwrap_or_else(|| format!("{{{}}}{}", idx, self.rowid));
        println!(
            "column(tab={}, cursor={}, idx={}) -> {:?}",
            self.vtab.id, self.id, idx, ret
        );
        context.set_result(ret);
        Ok(())
    }

    fn rowid(&self) -> Result<i64> {
        println!(
            "rowid(tab={}, cursor={}) -> {}",
            self.vtab.id, self.id, self.rowid
        );
        Ok(self.rowid)
    }
}

impl Drop for VTabLogCursor<'_> {
    fn drop(&mut self) {
        println!("drop(tab={}, cursor={})", self.vtab.id, self.id);
    }
}

#[sqlite3_ext_main]
fn init(db: &Connection) -> Result<()> {
    db.create_module("vtablog", Module::<VTabLog>::standard(), None)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use rusqlite;

    fn setup() -> rusqlite::Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open_in_memory()?;
        init(Connection::from_rusqlite(&conn))?;
        Ok(conn)
    }

    #[test]
    fn example() -> rusqlite::Result<()> {
        let conn = setup()?;
        conn.execute(
            "CREATE VIRTUAL TABLE temp.log USING vtablog(schema='CREATE TABLE x(a,b,c)', rows=25)",
            [],
        )?;
        let ret = conn
            .prepare("SELECT * FROM log")?
            .query_map([], |row| {
                Ok(vec![
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ])
            })?
            .into_iter()
            .collect::<rusqlite::Result<Vec<Vec<String>>>>()?;
        assert_eq!(
            ret,
            (0..25)
                .map(|i| vec![format!("a{}", i), format!("b{}", i), format!("c{}", i)])
                .collect::<Vec<Vec<String>>>()
        );
        Ok(())
    }
}
