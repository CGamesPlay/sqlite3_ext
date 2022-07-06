use sqlite3_ext::sqlite3_match_version;

fn test_no_fallback() {
    sqlite3_match_version! {
        3_008_006 => ()
    }
}

fn main() {}
