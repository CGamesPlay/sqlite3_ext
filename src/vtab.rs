use sqlite3_ext::vtab::*;

pub struct CrdbVTab {}

impl VTab for CrdbVTab {
    type Aux = ();

    fn connect(&self) {
        todo!()
    }
    fn best_index(&self) {
        todo!()
    }
    fn open(&self) {
        todo!()
    }
}
