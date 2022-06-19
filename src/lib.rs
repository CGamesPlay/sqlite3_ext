use sqlite3_ext::{vtab::*, *};

mod vtab;

#[sqlite3_ext_main]
fn crdb_init(db: &Connection) -> Result<bool> {
    db.create_module("crdb", Module::<vtab::CrdbVTab>::eponymous(), None)?;
    Ok(false)
}
