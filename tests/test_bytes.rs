mod common;

use audion::value::Value;

fn eval(src: &str) -> Value {
    common::eval(src)
}

// ---------------------------------------------------------------------------
// Value::Bytes basics
// ---------------------------------------------------------------------------

#[test]
fn test_bytes_type_name() {
    let val = eval(r#"
        let b = file_read_bytes("/dev/null");
        str(b);
    "#);
    assert_eq!(val, Value::String("<bytes: 0>".to_string()));
}

#[test]
fn test_bytes_is_truthy_empty() {
    let val = eval(r#"
        let b = file_read_bytes("/dev/null");
        bool(b);
    "#);
    assert_eq!(val, Value::Bool(false));
}

// ---------------------------------------------------------------------------
// file_read_bytes / file_write_bytes round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_file_read_write_bytes_roundtrip() {
    let val = eval(r#"
        let data = array_to_bytes([0, 127, 255, 72, 101]);
        let path = "/tmp/audion_test_bytes_roundtrip.bin";
        file_write_bytes(path, data);
        let read_back = file_read_bytes(path);
        file_delete(path);
        bytes_len(read_back);
    "#);
    assert_eq!(val, Value::Number(5.0));
}

#[test]
fn test_file_write_bytes_returns_length() {
    let val = eval(r#"
        let data = array_to_bytes([10, 20, 30]);
        let path = "/tmp/audion_test_bytes_write_len.bin";
        let n = file_write_bytes(path, data);
        file_delete(path);
        n;
    "#);
    assert_eq!(val, Value::Number(3.0));
}

#[test]
fn test_file_read_bytes_nonexistent() {
    let val = eval(r#"file_read_bytes("/tmp/audion_nonexistent_file_xyz.bin");"#);
    assert_eq!(val, Value::Bool(false));
}

// ---------------------------------------------------------------------------
// bytes_len
// ---------------------------------------------------------------------------

#[test]
fn test_bytes_len() {
    let val = eval(r#"
        let b = array_to_bytes([1, 2, 3, 4, 5]);
        bytes_len(b);
    "#);
    assert_eq!(val, Value::Number(5.0));
}

#[test]
fn test_bytes_len_empty() {
    let val = eval(r#"
        let b = array_to_bytes([]);
        bytes_len(b);
    "#);
    assert_eq!(val, Value::Number(0.0));
}

// ---------------------------------------------------------------------------
// bytes_get
// ---------------------------------------------------------------------------

#[test]
fn test_bytes_get() {
    let val = eval(r#"
        let b = array_to_bytes([10, 20, 30]);
        bytes_get(b, 1);
    "#);
    assert_eq!(val, Value::Number(20.0));
}

#[test]
fn test_bytes_get_negative_index() {
    let val = eval(r#"
        let b = array_to_bytes([10, 20, 30]);
        bytes_get(b, -1);
    "#);
    assert_eq!(val, Value::Number(30.0));
}

#[test]
fn test_bytes_get_out_of_bounds() {
    let val = eval(r#"
        let b = array_to_bytes([10, 20, 30]);
        bytes_get(b, 99);
    "#);
    assert_eq!(val, Value::Nil);
}

// ---------------------------------------------------------------------------
// bytes_slice
// ---------------------------------------------------------------------------

#[test]
fn test_bytes_slice_with_length() {
    let val = eval(r#"
        let b = array_to_bytes([10, 20, 30, 40, 50]);
        let s = bytes_slice(b, 1, 3);
        bytes_to_array(s);
    "#);
    // Should be [20, 30, 40]
    match val {
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            let vals: Vec<f64> = guard.entries().iter().map(|(_, v)| {
                if let Value::Number(n) = v { *n } else { panic!("expected number") }
            }).collect();
            assert_eq!(vals, vec![20.0, 30.0, 40.0]);
        }
        _ => panic!("expected array"),
    }
}

#[test]
fn test_bytes_slice_without_length() {
    let val = eval(r#"
        let b = array_to_bytes([10, 20, 30, 40, 50]);
        let s = bytes_slice(b, 2);
        bytes_len(s);
    "#);
    assert_eq!(val, Value::Number(3.0));
}

#[test]
fn test_bytes_slice_clamps_to_bounds() {
    let val = eval(r#"
        let b = array_to_bytes([10, 20, 30]);
        let s = bytes_slice(b, 1, 100);
        bytes_len(s);
    "#);
    assert_eq!(val, Value::Number(2.0));
}

// ---------------------------------------------------------------------------
// bytes_to_array / array_to_bytes round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_bytes_to_array() {
    let val = eval(r#"
        let b = array_to_bytes([0, 127, 255]);
        let a = bytes_to_array(b);
        count(a);
    "#);
    assert_eq!(val, Value::Number(3.0));
}

#[test]
fn test_bytes_to_array_values() {
    let val = eval(r#"
        let b = array_to_bytes([72, 101, 108]);
        let a = bytes_to_array(b);
        a[1];
    "#);
    assert_eq!(val, Value::Number(101.0));
}

#[test]
fn test_array_to_bytes_clamps() {
    // Values outside 0-255 should be clamped
    let val = eval(r#"
        let b = array_to_bytes([-10, 300, 128]);
        let a = bytes_to_array(b);
        a[0];
    "#);
    assert_eq!(val, Value::Number(0.0));
}

#[test]
fn test_array_to_bytes_clamps_high() {
    let val = eval(r#"
        let b = array_to_bytes([-10, 300, 128]);
        let a = bytes_to_array(b);
        a[1];
    "#);
    assert_eq!(val, Value::Number(255.0));
}

#[test]
fn test_roundtrip_array_bytes_array() {
    let val = eval(r#"
        let original = [0, 64, 128, 192, 255];
        let b = array_to_bytes(original);
        let result = bytes_to_array(b);
        result == original;
    "#);
    assert_eq!(val, Value::Bool(true));
}

// ---------------------------------------------------------------------------
// bytes_to_array with empty
// ---------------------------------------------------------------------------

#[test]
fn test_empty_roundtrip() {
    let val = eval(r#"
        let b = array_to_bytes([]);
        let a = bytes_to_array(b);
        count(a);
    "#);
    assert_eq!(val, Value::Number(0.0));
}

// ---------------------------------------------------------------------------
// file_size
// ---------------------------------------------------------------------------

#[test]
fn test_file_size() {
    let val = eval(r#"
        let path = "/tmp/audion_test_file_size.bin";
        file_write_bytes(path, array_to_bytes([10, 20, 30, 40, 50]));
        let sz = file_size(path);
        file_delete(path);
        sz;
    "#);
    assert_eq!(val, Value::Number(5.0));
}

#[test]
fn test_file_size_nonexistent() {
    let val = eval(r#"file_size("/tmp/audion_nope_nope_nope.bin");"#);
    assert_eq!(val, Value::Bool(false));
}

// ---------------------------------------------------------------------------
// file_read_bytes with offset and length (partial reads)
// ---------------------------------------------------------------------------

#[test]
fn test_file_read_bytes_with_offset() {
    let val = eval(r#"
        let path = "/tmp/audion_test_partial_read.bin";
        file_write_bytes(path, array_to_bytes([10, 20, 30, 40, 50]));
        let tail = file_read_bytes(path, 2);
        file_delete(path);
        bytes_to_array(tail);
    "#);
    match val {
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            let vals: Vec<f64> = guard.entries().iter().map(|(_, v)| {
                if let Value::Number(n) = v { *n } else { panic!("expected number") }
            }).collect();
            assert_eq!(vals, vec![30.0, 40.0, 50.0]);
        }
        _ => panic!("expected array"),
    }
}

#[test]
fn test_file_read_bytes_with_offset_and_length() {
    let val = eval(r#"
        let path = "/tmp/audion_test_partial_read2.bin";
        file_write_bytes(path, array_to_bytes([10, 20, 30, 40, 50]));
        let chunk = file_read_bytes(path, 1, 2);
        file_delete(path);
        bytes_to_array(chunk);
    "#);
    match val {
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            let vals: Vec<f64> = guard.entries().iter().map(|(_, v)| {
                if let Value::Number(n) = v { *n } else { panic!("expected number") }
            }).collect();
            assert_eq!(vals, vec![20.0, 30.0]);
        }
        _ => panic!("expected array"),
    }
}

#[test]
fn test_file_read_bytes_offset_beyond_eof() {
    let val = eval(r#"
        let path = "/tmp/audion_test_partial_eof.bin";
        file_write_bytes(path, array_to_bytes([10, 20, 30]));
        let chunk = file_read_bytes(path, 100, 5);
        file_delete(path);
        bytes_len(chunk);
    "#);
    assert_eq!(val, Value::Number(0.0));
}

// ---------------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------------

#[test]
fn test_bytes_equality() {
    let val = eval(r#"
        let a = array_to_bytes([1, 2, 3]);
        let b = array_to_bytes([1, 2, 3]);
        a == b;
    "#);
    assert_eq!(val, Value::Bool(true));
}

#[test]
fn test_bytes_inequality() {
    let val = eval(r#"
        let a = array_to_bytes([1, 2, 3]);
        let b = array_to_bytes([1, 2, 4]);
        a == b;
    "#);
    assert_eq!(val, Value::Bool(false));
}

// ---------------------------------------------------------------------------
// Real binary file: read actual bytes from disk
// ---------------------------------------------------------------------------

#[test]
fn test_file_read_bytes_real_file() {
    let val = eval(r#"
        let path = "/tmp/audion_test_binary_real.bin";
        // Write known bytes
        file_write_bytes(path, array_to_bytes([77, 84, 104, 100]));
        // Read them back and verify (MThd - MIDI header magic)
        let b = file_read_bytes(path);
        file_delete(path);
        let a = bytes_to_array(b);
        a[0] * 256 * 256 * 256 + a[1] * 256 * 256 + a[2] * 256 + a[3];
    "#);
    // 0x4D546864 = 1297377380
    assert_eq!(val, Value::Number(1297377380.0));
}
