use sqlite3_ext::*;
use subprocess::{Popen, PopenConfig, Redirection};

/// Invoke a subprocess to build one of the examples as a loadable module.
fn build_extension() -> String {
    use serde_json::Value;
    let mut p = Popen::create(
        &[
            "cargo",
            "build",
            "--message-format=json",
            "--example",
            "generate_series",
        ],
        PopenConfig {
            stdout: Redirection::Pipe,
            ..Default::default()
        },
    )
    .expect("compilation failure");
    let (json, _) = p.communicate(None).expect("command failed");
    for line in json.unwrap().lines() {
        if let Value::Object(v) = serde_json::from_str(line).expect("invalid JSON") {
            if !matches!(&v["reason"], Value::String(s) if s == "compiler-artifact") {
                continue;
            }
            let target = match &v["target"] {
                Value::Object(t) => t,
                _ => continue,
            };
            if !target.contains_key("name")
                || !matches!(&target["name"], Value::String(s) if s == "generate_series")
            {
                continue;
            }
            let filenames = match &v["filenames"] {
                Value::Array(a) => a,
                _ => continue,
            };
            match &filenames[0] {
                Value::String(s) => return s.to_owned(),
                _ => continue,
            }
        }
    }
    todo!();
}

#[test]
fn main() -> Result<()> {
    let dylib_path = build_extension();
    let conn = Database::open(":memory:")?;
    conn.load_extension(&dylib_path, None)?;
    let results: Vec<i64> = conn
        .prepare("SELECT value FROM generate_series(5, 100, 5)")?
        .query(())?
        .map(|row| Ok(row[0].get_i64()))
        .collect()?;
    assert_eq!(
        results,
        vec![5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95, 100]
    );
    Ok(())
}
