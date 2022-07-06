use sqlite3_ext::sqlite3_match_version;

fn test_invalid_version() {
    sqlite3_match_version! {
        308 => {
            println!("feature supported");
        }
        _ => (),
    }
}

fn main() {}
