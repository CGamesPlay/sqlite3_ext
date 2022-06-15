use super::bindings::*;
use super::types::*;

pub fn create_module(db: &Connection) -> Result<()> {
    VTabModule::new("crdb", CrdbVTab {}).register(db)?;
    Ok(())
}

struct CrdbVTab {}

impl VTab for CrdbVTab {
    fn connect() {
        todo!()
    }
    fn best_index() {
        todo!()
    }
    fn open() {
        todo!()
    }
}
