#![cfg(all(test, feature = "static"))]
use crate::test_helpers::prelude::*;

#[test]
fn get_blob() {
    let h = TestHelpers::new();
    let bytes = "my string".as_bytes();
    h.with_value(bytes, |val| {
        assert_eq!(
            val.get_blob()?.unwrap(),
            vec![109, 121, 32, 115, 116, 114, 105, 110, 103]
        );
        Ok(())
    });
}
