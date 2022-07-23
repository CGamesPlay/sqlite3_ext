use sqlite3_ext::{function::*, *};
use std::cell::Cell;

fn add_funcs(db: &Connection) -> Result<()> {
    let opts = FunctionOptions::default()
        .set_risk_level(RiskLevel::Innocuous)
        .set_n_args(0);
    let cell = Cell::new(42);
    db.create_scalar_function("drop_check", &opts, |c, _| c.set_result(cell.get()))?;
    Ok(())
}

fn main() {}
