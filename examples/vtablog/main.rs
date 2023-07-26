//! Rust implementation of the vtablog virtual table.
//!
//! For more information, consult [the original implementation](https://sqlite.org/src/file/ext/misc/vtablog.c).

use sqlite3_ext::{vtab::*, *};
use std::{
    cell::{Cell, RefCell},
    fmt::Arguments,
    io::{stderr, Write},
    rc::Rc,
};

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
            Err(e) => Err(Error::Module(format!("{e}"))),
        }
    }
}

struct DB<O: Write> {
    out: Rc<RefCell<O>>,
    n_inst: RefCell<usize>,
}

#[sqlite3_ext_vtab(StandardModule, UpdateVTab, TransactionVTab, RenameVTab)]
struct VTabLog<O: Write + 'static> {
    db: Rc<DB<O>>,
    id: usize,
    num_rows: i64,
    num_cursors: Cell<usize>,
    num_transactions: Cell<usize>,
}

struct VTabLogCursor<'vtab, O: Write + 'static> {
    vtab: &'vtab VTabLog<O>,
    id: usize,
    rowid: i64,
}

struct VTabLogTransaction<'vtab, O: Write + 'static> {
    vtab: &'vtab VTabLog<O>,
    id: usize,
}

impl<O: Write> VTabLog<O> {
    fn write_fmt(&self, args: Arguments<'_>) -> Result<()> {
        self.db
            .out
            .borrow_mut()
            .write_fmt(args)
            .map_err(|e| Error::Module(e.to_string()))
    }

    fn connect_create(aux: &Rc<DB<O>>, args: &[&str], method: &str) -> Result<(String, Self)> {
        let id = {
            let mut n_inst = aux.n_inst.borrow_mut();
            *n_inst += 100;
            *n_inst
        };

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
        let vtab = VTabLog {
            db: aux.clone(),
            id,
            num_rows,
            num_cursors: Cell::new(0),
            num_transactions: Cell::new(0),
        };

        writeln!(vtab, "{method}(tab={id}, args={args:?})")?;

        Ok((schema, vtab))
    }
}

impl<'vtab, O: Write + 'static> VTab<'vtab> for VTabLog<O> {
    type Aux = Rc<DB<O>>;
    type Cursor = VTabLogCursor<'vtab, O>;

    fn connect(_: &VTabConnection, db: &Self::Aux, args: &[&str]) -> Result<(String, Self)> {
        Self::connect_create(db, args, "connect")
    }

    fn best_index(&self, index_info: &mut IndexInfo) -> Result<()> {
        writeln!(
            self,
            "best_index(tab={}, index_info={index_info:?})",
            self.id
        )?;
        if let Some(mut c) = index_info.constraints().next() {
            if c.usable() {
                c.set_argv_index(Some(0));
                index_info.set_estimated_cost(1.0);
            }
        }
        Ok(())
    }

    fn open(&'vtab self) -> Result<Self::Cursor> {
        self.num_cursors.set(self.num_cursors.get() + 1);
        let ret = VTabLogCursor {
            vtab: self,
            id: self.id + self.num_cursors.get(),
            rowid: 0,
        };
        writeln!(self, "open(tab={}, cursor={})", self.id, ret.id)?;
        Ok(ret)
    }

    fn disconnect(self) -> DisconnectResult<Self> {
        writeln!(self, "disconnect(tab={})", self.id).map_err(|e| (self, e))?;
        Ok(())
    }
}

impl<'vtab, O: Write + 'static> CreateVTab<'vtab> for VTabLog<O> {
    const SHADOW_NAMES: &'static [&'static str] = &["shadow"];

    fn create(_: &VTabConnection, db: &Self::Aux, args: &[&str]) -> Result<(String, Self)> {
        Self::connect_create(db, args, "create")
    }

    fn destroy(self) -> DisconnectResult<Self> {
        writeln!(self, "destroy(tab={})", self.id).map_err(|e| (self, e))?;
        Ok(())
    }
}

impl<'vtab, O: Write + 'static> UpdateVTab<'vtab> for VTabLog<O> {
    fn update(&self, info: &mut ChangeInfo) -> Result<i64> {
        writeln!(self, "update(tab={}, args={info:?})", self.id)?;
        sqlite3_match_version! {
            3_022_000 => {
                let unchanged: Vec<_> = info.args()
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.nochange())
                    .map(|(i, _)| i)
                    .collect();
                if unchanged.len() > 0 {
                    writeln!(self, "  unchanged: {unchanged:?}")?;
                }
            }
            _ => (),
        }
        Ok(0)
    }
}

impl<'vtab, O: Write + 'static> TransactionVTab<'vtab> for VTabLog<O> {
    type Transaction = VTabLogTransaction<'vtab, O>;

    fn begin(&'vtab self) -> Result<Self::Transaction> {
        self.num_transactions.set(self.num_transactions.get() + 1);
        let ret = VTabLogTransaction {
            vtab: self,
            id: self.id + self.num_transactions.get(),
        };
        writeln!(self, "begin(tab={}, transaction={})", self.id, ret.id)?;
        Ok(ret)
    }
}

impl<'vtab, O: Write + 'static> RenameVTab<'vtab> for VTabLog<O> {
    fn rename(&self, name: &str) -> Result<()> {
        writeln!(self, "rename(tab={}, name={name:?})", self.id)?;
        Ok(())
    }
}

impl<O: Write> Drop for VTabLog<O> {
    fn drop(&mut self) {
        writeln!(self, "drop(tab={})", self.id).unwrap();
    }
}

impl<'vtab, O: Write> VTabCursor for VTabLogCursor<'vtab, O> {
    fn filter(&mut self, _: i32, _: Option<&str>, args: &mut [&mut ValueRef]) -> Result<()> {
        writeln!(
            self.vtab,
            "filter(tab={}, cursor={}, args={args:?})",
            self.vtab.id, self.id
        )?;
        self.rowid = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        writeln!(
            self.vtab,
            "next(tab={}, cursor={})\n  rowid {} -> {}",
            self.vtab.id,
            self.id,
            self.rowid,
            self.rowid + 1
        )?;
        self.rowid += 1;
        Ok(())
    }

    fn eof(&mut self) -> bool {
        let ret = self.rowid >= self.vtab.num_rows;
        writeln!(
            self.vtab,
            "eof(tab={}, cursor={}) -> {ret}",
            self.vtab.id, self.id
        )
        .unwrap();
        ret
    }

    fn column(&mut self, idx: usize, context: &ColumnContext) -> Result<()> {
        const ALPHABET: &[u8] = "abcdefghijklmnopqrstuvwxyz".as_bytes();
        let mut ret = match () {
            _ if context.nochange() => Err(Error::NoChange),
            _ => Ok(ALPHABET
                .get(idx)
                .map(|l| format!("{}{}", *l as char, self.rowid))
                .unwrap_or_else(|| format!("{{{idx}}}{}", self.rowid))),
        };
        if let Err(e) = writeln!(
            self.vtab,
            "column(tab={}, cursor={}, idx={idx}) -> {ret:?}",
            self.vtab.id, self.id
        ) {
            ret = Err(e.into())
        }
        context.set_result(ret)
    }

    fn rowid(&mut self) -> Result<i64> {
        writeln!(
            self.vtab,
            "rowid(tab={}, cursor={}) -> {}",
            self.vtab.id, self.id, self.rowid
        )?;
        Ok(self.rowid)
    }
}

impl<O: Write> Drop for VTabLogCursor<'_, O> {
    fn drop(&mut self) {
        writeln!(self.vtab, "drop(tab={}, cursor={})", self.vtab.id, self.id).unwrap();
    }
}

impl<'vtab, O: Write> VTabTransaction for VTabLogTransaction<'vtab, O> {
    fn sync(&mut self) -> Result<()> {
        writeln!(
            self.vtab,
            "sync(tab={}, transaction={})",
            self.vtab.id, self.id
        )?;
        Ok(())
    }

    fn commit(self) -> Result<()> {
        writeln!(
            self.vtab,
            "commit(tab={}, transaction={})",
            self.vtab.id, self.id
        )?;
        Ok(())
    }

    fn rollback(self) -> Result<()> {
        writeln!(
            self.vtab,
            "rollback(tab={}, transaction={})",
            self.vtab.id, self.id
        )?;
        Ok(())
    }

    fn savepoint(&mut self, n: i32) -> Result<()> {
        writeln!(
            self.vtab,
            "savepoint(tab={}, transaction={}, n={n})",
            self.vtab.id, self.id
        )?;
        Ok(())
    }

    fn release(&mut self, n: i32) -> Result<()> {
        writeln!(
            self.vtab,
            "release(tab={}, transaction={}, n={n})",
            self.vtab.id, self.id
        )?;
        Ok(())
    }

    fn rollback_to(&mut self, n: i32) -> Result<()> {
        writeln!(
            self.vtab,
            "rollback_to(tab={}, transaction={}, n={n})",
            self.vtab.id, self.id
        )?;
        Ok(())
    }
}

impl<O: Write> Drop for VTabLogTransaction<'_, O> {
    fn drop(&mut self) {
        writeln!(
            self.vtab,
            "drop_transaction(tab={}, transaction={})",
            self.vtab.id, self.id
        )
        .unwrap();
    }
}

#[sqlite3_ext_main]
fn init_stderr(db: &Connection) -> Result<()> {
    init(db, Rc::new(RefCell::new(stderr())))
}

fn init<O: Write + 'static>(db: &Connection, out: Rc<RefCell<O>>) -> Result<()> {
    let aux = Rc::new(DB {
        out,
        n_inst: RefCell::new(0),
    });
    db.create_module("vtablog", VTabLog::module(), aux)?;
    Ok(())
}

#[cfg(all(test, feature = "static"))]
mod test;
