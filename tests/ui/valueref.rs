use sqlite3_ext::*;

fn verify_held_conversions(val: &mut ValueRef) -> Result<()> {
    let ref_str = val.get_str()?;
    let ref_str_2 = val.get_str()?;
    println!("{}, {}", ref_str, ref_str_2);
    Ok(())
}

fn main() {}
