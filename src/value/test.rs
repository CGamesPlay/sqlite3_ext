#![cfg(all(test, feature = "static"))]
use crate::test_helpers::prelude::*;
use std::{f64::consts::PI, mem::transmute};

#[test]
fn get_i64() {
    let h = TestHelpers::new();
    let val = 69420i64;
    h.with_value(val, |val| {
        assert_eq!(val.value_type(), ValueType::Integer);
        assert_eq!(val.get_i64(), 69420);
        assert_eq!(format!("{:?}", val), "ValueRef::Integer(69420)");
        Ok(())
    });
}

#[test]
fn get_f64() {
    let h = TestHelpers::new();
    let val = PI;
    h.with_value(val, |val| {
        assert_eq!(val.value_type(), ValueType::Float);
        assert_eq!(val.get_f64(), PI);
        assert_eq!(format!("{:?}", val), "ValueRef::Float(3.141592653589793)");
        Ok(())
    });
}

#[test]
fn get_blob() {
    let h = TestHelpers::new();
    let bytes = b"my string";
    h.with_value(bytes, |val| {
        assert_eq!(val.value_type(), ValueType::Blob);
        assert_eq!(val.get_blob()?, Some(b"my string".as_slice()));
        assert_eq!(
            format!("{:?}", val),
            "ValueRef::Blob([109, 121, 32, 115, 116, 114, 105, 110, 103])"
        );
        Ok(())
    });
}

#[test]
fn get_blob_null() {
    let h = TestHelpers::new();
    let null: Option<i64> = None;
    h.with_value(null, |val| {
        assert_eq!(val.value_type(), ValueType::Null);
        assert_eq!(val.get_blob()?, None);
        assert_eq!(format!("{:?}", val), "ValueRef::Null");
        Ok(())
    });
}

#[test]
fn get_ptr() {
    let h = TestHelpers::new();
    type T<'a> = &'a String;
    let owned_string = "input string".to_owned();
    let bits: [u8; 8] = unsafe { transmute::<T, _>(&owned_string) };
    h.with_value(bits, |val| {
        assert_eq!(val.value_type(), ValueType::Blob);
        let borrowed_string = unsafe { *(val.get_ptr::<T>()?) };
        assert_eq!(borrowed_string, "input string");
        Ok(())
    });
}

#[test]
fn get_ptr_wide() {
    let h = TestHelpers::new();
    type T<'a> = &'a [u32];
    let owned_slice: &[u32] = &[1, 2, 3, 4];
    let bits: [u8; 16] = unsafe { transmute::<T, _>(owned_slice) };
    h.with_value(bits, |val| {
        assert_eq!(val.value_type(), ValueType::Blob);
        let borrowed_slice = unsafe { *val.get_ptr::<T>()? };
        assert_eq!(borrowed_slice, [1, 2, 3, 4]);
        Ok(())
    });
}

#[test]
fn get_ptr_null() {
    let h = TestHelpers::new();
    let null: Option<i64> = None;
    h.with_value(null, |val| {
        assert_eq!(val.value_type(), ValueType::Null);
        let ptr: *const () = val.get_ptr()?;
        assert!(ptr.is_null(), "ptr should be null");
        Ok(())
    });
}

#[test]
fn get_ptr_invalid() {
    let h = TestHelpers::new();
    h.with_value([1, 2, 3], |val| {
        assert_eq!(val.value_type(), ValueType::Blob);
        val.get_ptr::<()>().expect_err("incorrect length");
        Ok(())
    });
}

#[test]
fn get_str() {
    let h = TestHelpers::new();
    let string = "my string";
    h.with_value(string, |val| {
        assert_eq!(val.value_type(), ValueType::Text);
        assert_eq!(val.get_str()?, Some("my string"));
        assert_eq!(format!("{:?}", val), "ValueRef::Text(Ok(\"my string\"))");
        Ok(())
    });
}

#[test]
fn get_str_null() {
    let h = TestHelpers::new();
    let null: Option<i64> = None;
    h.with_value(null, |val| {
        assert_eq!(val.value_type(), ValueType::Null);
        assert_eq!(val.get_str()?, None);
        assert_eq!(format!("{:?}", val), "ValueRef::Null");
        Ok(())
    });
}

#[test]
fn get_str_invalid() {
    let h = TestHelpers::new();
    h.with_value_from_sql("CAST(x'009f9296' AS TEXT)", |val| {
        assert_eq!(val.value_type(), ValueType::Text);
        val.get_str().expect_err("invalid utf8");
        assert_eq!(
            format!("{:?}", val),
            "ValueRef::Text(Err(Utf8Error(Utf8Error { valid_up_to: 1, error_len: Some(1) })))"
        );
        Ok(())
    });
}
