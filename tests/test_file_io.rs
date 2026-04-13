mod common;

use audion::value::Value;

fn eval(src: &str) -> Value {
    common::eval(src)
}

// ---------------------------------------------------------------------------
// Handle-based streaming file I/O
// ---------------------------------------------------------------------------

#[test]
fn test_file_open_invalid_returns_false() {
    let val = eval(r#"file_open("/no/such/path/x.txt", "r");"#);
    assert_eq!(val, Value::Bool(false));
}

#[test]
fn test_file_open_bad_mode_errors() {
    let result = std::panic::catch_unwind(|| eval(r#"file_open("/tmp/x.txt", "z");"#));
    assert!(result.is_err());
}

#[test]
fn test_write_and_read_lines() {
    let val = eval(r#"
        let wh = file_open("/tmp/audion_test_stream.txt", "w");
        file_write_handle(wh, "hello\nworld\n");
        file_close(wh);

        let rh = file_open("/tmp/audion_test_stream.txt", "r");
        let l1 = file_line(rh);
        let l2 = file_line(rh);
        file_close(rh);

        bytes_len(l1) + bytes_len(l2);
    "#);
    assert_eq!(val, Value::Number(12.0)); // "hello\n"=6, "world\n"=6
}

#[test]
fn test_read_chunk() {
    let val = eval(r#"
        let wh = file_open("/tmp/audion_test_chunk.txt", "w");
        file_write_handle(wh, "ABCDEFGH");
        file_close(wh);

        let rh = file_open("/tmp/audion_test_chunk.txt", "r");
        let chunk = file_read_chunk(rh, 4);
        file_close(rh);
        bytes_len(chunk);
    "#);
    assert_eq!(val, Value::Number(4.0));
}

#[test]
fn test_eof_returns_false() {
    let val = eval(r#"
        let wh = file_open("/tmp/audion_test_eof.txt", "w");
        file_write_handle(wh, "x");
        file_close(wh);

        let rh = file_open("/tmp/audion_test_eof.txt", "r");
        file_line(rh);
        let at_eof = file_line(rh);
        file_close(rh);
        at_eof;
    "#);
    assert_eq!(val, Value::Bool(false));
}

#[test]
fn test_seek_and_tell() {
    let val = eval(r#"
        let wh = file_open("/tmp/audion_test_seek.txt", "w");
        file_write_handle(wh, "0123456789");
        file_close(wh);

        let rh = file_open("/tmp/audion_test_seek.txt", "r");
        file_seek(rh, 5);
        let pos = file_tell(rh);
        file_close(rh);
        pos;
    "#);
    assert_eq!(val, Value::Number(5.0));
}

#[test]
fn test_seek_then_read() {
    let val = eval(r#"
        let wh = file_open("/tmp/audion_test_seekread.txt", "w");
        file_write_handle(wh, "ABCDE");
        file_close(wh);

        let rh = file_open("/tmp/audion_test_seekread.txt", "r");
        file_seek(rh, 3);
        let chunk = file_read_chunk(rh, 2);
        file_close(rh);
        bytes_len(chunk);
    "#);
    assert_eq!(val, Value::Number(2.0));
}

#[test]
fn test_append_mode() {
    let val = eval(r#"
        let wh = file_open("/tmp/audion_test_append.txt", "w");
        file_write_handle(wh, "line1\n");
        file_close(wh);

        let ah = file_open("/tmp/audion_test_append.txt", "a");
        file_write_handle(ah, "line2\n");
        file_close(ah);

        let rh = file_open("/tmp/audion_test_append.txt", "r");
        let l1 = file_line(rh);
        let l2 = file_line(rh);
        file_close(rh);
        bytes_len(l1) + bytes_len(l2);
    "#);
    assert_eq!(val, Value::Number(12.0)); // "line1\n"=6, "line2\n"=6
}

#[test]
fn test_write_bytes_binary_safe() {
    // Write raw bytes including a null byte, read them back.
    let val = eval(r#"
        let data = array_to_bytes([0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF]);
        let wh = file_open("/tmp/audion_test_binary.bin", "w");
        file_write_handle(wh, data);
        file_close(wh);

        let rh = file_open("/tmp/audion_test_binary.bin", "r");
        let back = file_read_chunk(rh, 6);
        file_close(rh);
        bytes_len(back);
    "#);
    assert_eq!(val, Value::Number(6.0));
}

#[test]
fn test_close_invalid_handle_is_noop() {
    let val = eval("file_close(9999999);");
    assert_eq!(val, Value::Nil);
}
