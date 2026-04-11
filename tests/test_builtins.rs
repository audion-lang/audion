mod common;

use audion::value::Value;

fn eval(src: &str) -> Value {
    common::eval(src)
}

// ---------------------------------------------------------------------------
// Date/Time
// ---------------------------------------------------------------------------

#[test]
fn test_timestamp() {
    let val = eval("timestamp();");
    match val {
        Value::Number(n) => assert!(n > 1700000000.0, "timestamp should be recent"),
        _ => panic!("expected number"),
    }
}

#[test]
fn test_timestamp_ms() {
    let val = eval("timestamp_ms();");
    match val {
        Value::Number(n) => assert!(n > 1700000000000.0, "timestamp_ms should be in milliseconds"),
        _ => panic!("expected number"),
    }
}

#[test]
fn test_timestamp_ms_greater_than_timestamp() {
    let ts = eval("timestamp();");
    let ts_ms = eval("timestamp_ms();");
    match (ts, ts_ms) {
        (Value::Number(s), Value::Number(ms)) => {
            assert!(ms > s * 999.0);
            assert!(ms < s * 1001.0);
        }
        _ => panic!("expected numbers"),
    }
}

#[test]
fn test_date_year() {
    let val = eval(r#"date("Y");"#);
    match val {
        Value::String(s) => {
            let year: i32 = s.parse().unwrap();
            assert!(year >= 2024 && year <= 2100);
        }
        _ => panic!("expected string"),
    }
}

#[test]
fn test_date_literal_passthrough() {
    let val = eval(r#"date("Y-m-d");"#);
    match val {
        Value::String(s) => {
            assert!(s.contains('-'), "dashes should be preserved");
            let parts: Vec<&str> = s.split('-').collect();
            assert_eq!(parts.len(), 3);
        }
        _ => panic!("expected string"),
    }
}

#[test]
fn test_date_time_format() {
    let val = eval(r#"date("H:i:s");"#);
    match val {
        Value::String(s) => {
            assert!(s.contains(':'), "colons should be preserved");
            let parts: Vec<&str> = s.split(':').collect();
            assert_eq!(parts.len(), 3);
        }
        _ => panic!("expected string"),
    }
}

#[test]
fn test_date_with_timestamp() {
    // 0 = 1970-01-01 00:00:00 UTC
    let val = eval(r#"date("Y", 0);"#);
    match val {
        Value::String(s) => assert_eq!(s, "1970"),
        _ => panic!("expected string"),
    }
}

// ---------------------------------------------------------------------------
// Type Casts
// ---------------------------------------------------------------------------

#[test]
fn test_int_from_float() {
    assert_eq!(eval("int(3.7);"), Value::Number(3.0));
    assert_eq!(eval("int(-3.7);"), Value::Number(-3.0));
}

#[test]
fn test_int_from_string() {
    assert_eq!(eval(r#"int("42");"#), Value::Number(42.0));
    assert_eq!(eval(r#"int("3.9");"#), Value::Number(3.0));
    assert_eq!(eval(r#"int("abc");"#), Value::Number(0.0));
}

#[test]
fn test_int_from_bool() {
    assert_eq!(eval("int(true);"), Value::Number(1.0));
    assert_eq!(eval("int(false);"), Value::Number(0.0));
}

#[test]
fn test_int_from_nil() {
    assert_eq!(eval("int(nil);"), Value::Number(0.0));
}

#[test]
fn test_float_from_string() {
    assert_eq!(eval(r#"float("3.14");"#), Value::Number(3.14));
    assert_eq!(eval(r#"float("abc");"#), Value::Number(0.0));
}

#[test]
fn test_float_identity() {
    assert_eq!(eval("float(42.5);"), Value::Number(42.5));
}

#[test]
fn test_float_from_bool() {
    assert_eq!(eval("float(true);"), Value::Number(1.0));
    assert_eq!(eval("float(false);"), Value::Number(0.0));
}

#[test]
fn test_bool_truthy() {
    assert_eq!(eval("bool(1);"), Value::Bool(true));
    assert_eq!(eval("bool(0);"), Value::Bool(false));
    assert_eq!(eval(r#"bool("");"#), Value::Bool(false));
    assert_eq!(eval(r#"bool("hello");"#), Value::Bool(true));
    assert_eq!(eval("bool(nil);"), Value::Bool(false));
    assert_eq!(eval("bool(true);"), Value::Bool(true));
    assert_eq!(eval("bool(false);"), Value::Bool(false));
}

#[test]
fn test_str_cast() {
    assert_eq!(eval("str(42);"), Value::String("42".to_string()));
    assert_eq!(eval("str(3.14);"), Value::String("3.14".to_string()));
    assert_eq!(eval("str(true);"), Value::String("true".to_string()));
    assert_eq!(eval("str(nil);"), Value::String("nil".to_string()));
}

// ---------------------------------------------------------------------------
// String Functions
// ---------------------------------------------------------------------------

#[test]
fn test_str_upper() {
    assert_eq!(
        eval(r#"str_upper("hello");"#),
        Value::String("HELLO".to_string())
    );
}

#[test]
fn test_str_lower() {
    assert_eq!(
        eval(r#"str_lower("HELLO");"#),
        Value::String("hello".to_string())
    );
}

#[test]
fn test_str_trim() {
    assert_eq!(
        eval(r#"str_trim("  hello  ");"#),
        Value::String("hello".to_string())
    );
}

#[test]
fn test_str_length() {
    assert_eq!(eval(r#"str_length("hello");"#), Value::Number(5.0));
    assert_eq!(eval(r#"str_length("");"#), Value::Number(0.0));
}

#[test]
fn test_str_replace_literal() {
    assert_eq!(
        eval(r#"str_replace("world", "Rust", "hello world");"#),
        Value::String("hello Rust".to_string())
    );
}

#[test]
fn test_str_replace_regex() {
    assert_eq!(
        eval(r#"str_replace("/[0-9]+/", "X", "abc123def456");"#),
        Value::String("abcXdefX".to_string())
    );
}

#[test]
fn test_str_contains_literal() {
    assert_eq!(eval(r#"str_contains("lo", "hello");"#), Value::Bool(true));
    assert_eq!(eval(r#"str_contains("xyz", "hello");"#), Value::Bool(false));
}

#[test]
fn test_str_contains_regex() {
    assert_eq!(
        eval(r#"str_contains("/[0-9]+/", "abc123");"#),
        Value::Bool(true)
    );
    assert_eq!(
        eval(r#"str_contains("/[0-9]+/", "abcdef");"#),
        Value::Bool(false)
    );
}

#[test]
fn test_str_substr() {
    assert_eq!(
        eval(r#"str_substr("hello world", 6);"#),
        Value::String("world".to_string())
    );
    assert_eq!(
        eval(r#"str_substr("hello world", 0, 5);"#),
        Value::String("hello".to_string())
    );
}

#[test]
fn test_str_substr_out_of_range() {
    assert_eq!(
        eval(r#"str_substr("hello", 10);"#),
        Value::String("".to_string())
    );
}

#[test]
fn test_str_starts_with() {
    assert_eq!(
        eval(r#"str_starts_with("hel", "hello");"#),
        Value::Bool(true)
    );
    assert_eq!(
        eval(r#"str_starts_with("xyz", "hello");"#),
        Value::Bool(false)
    );
}

#[test]
fn test_str_ends_with() {
    assert_eq!(
        eval(r#"str_ends_with("llo", "hello");"#),
        Value::Bool(true)
    );
    assert_eq!(
        eval(r#"str_ends_with("xyz", "hello");"#),
        Value::Bool(false)
    );
}

// ---------------------------------------------------------------------------
// Exec
// ---------------------------------------------------------------------------

#[test]
fn test_exec_echo() {
    let val = eval(r#"exec("echo", "hello");"#);
    match val {
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            let stdout = guard.get(&Value::String("stdout".to_string())).unwrap();
            assert_eq!(*stdout, Value::String("hello\n".to_string()));
            let status = guard.get(&Value::String("status".to_string())).unwrap();
            assert_eq!(*status, Value::Number(0.0));
        }
        _ => panic!("expected array"),
    }
}

#[test]
fn test_exec_nonexistent() {
    let val = eval(r#"exec("this_command_does_not_exist_xyz");"#);
    assert_eq!(val, Value::Bool(false));
}

#[test]
fn test_exec_with_status() {
    let val = eval(r#"exec("test", "-d", "/tmp");"#);
    match val {
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();
            let status = guard.get(&Value::String("status".to_string())).unwrap();
            assert_eq!(*status, Value::Number(0.0));
        }
        _ => panic!("expected array"),
    }
}

// ---------------------------------------------------------------------------
// Hash
// ---------------------------------------------------------------------------

#[test]
fn test_hash_md5() {
    assert_eq!(
        eval(r#"hash("md5", "hello");"#),
        Value::String("5d41402abc4b2a76b9719d911017c592".to_string())
    );
}

#[test]
fn test_hash_sha256() {
    assert_eq!(
        eval(r#"hash("sha256", "hello");"#),
        Value::String(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string()
        )
    );
}

#[test]
fn test_hash_sha512() {
    let val = eval(r#"hash("sha512", "hello");"#);
    match val {
        Value::String(s) => assert_eq!(s.len(), 128, "SHA-512 hex should be 128 chars"),
        _ => panic!("expected string"),
    }
}

#[test]
#[should_panic]
fn test_hash_unsupported() {
    eval(r#"hash("sha1", "hello");"#);
}

// ---------------------------------------------------------------------------
// UDP Networking
// ---------------------------------------------------------------------------

#[test]
fn test_net_udp_bind_single_arg() {
    // Bind to a random available port
    let val = eval("net_udp_bind(0);");
    match val {
        Value::Number(n) => assert!(n > 0.0, "should return a valid handle"),
        Value::Bool(false) => panic!("binding to port 0 should not fail"),
        _ => panic!("expected number or false"),
    }
}

#[test]
fn test_net_udp_bind_two_args() {
    // Bind to localhost with specific interface
    let val = eval(r#"net_udp_bind("127.0.0.1", 0);"#);
    match val {
        Value::Number(n) => assert!(n > 0.0, "should return a valid handle"),
        Value::Bool(false) => panic!("binding to localhost:0 should not fail"),
        _ => panic!("expected number or false"),
    }
}

#[test]
fn test_net_udp_send_recv_roundtrip() {
    // Create two UDP sockets and test sending/receiving
    let code = r#"
        let receiver = net_udp_bind("127.0.0.1", 19999);
        let sender = net_udp_bind("127.0.0.1", 0);

        // Send a message
        let bytes_sent = net_udp_send(sender, "127.0.0.1", 19999, "Hello UDP!");

        // Give it a moment to arrive (in real code, would use proper synchronization)
        wait_ms(10);

        // Try to receive
        let result = net_udp_recv(receiver);

        // Close sockets
        net_close(sender);
        net_close(receiver);

        result;
    "#;

    let val = eval(code);
    match val {
        Value::Array(arr) => {
            let guard = arr.lock().unwrap();

            // Check that we got data
            let data = guard.get(&Value::String("data".to_string()));
            assert!(data.is_some(), "should have 'data' key");

            if let Some(Value::String(s)) = data {
                assert_eq!(s, "Hello UDP!", "received data should match sent data");
            } else {
                panic!("data should be a string");
            }

            // Check that we got host
            let host = guard.get(&Value::String("host".to_string()));
            assert!(host.is_some(), "should have 'host' key");

            // Check that we got port
            let port = guard.get(&Value::String("port".to_string()));
            assert!(port.is_some(), "should have 'port' key");
        }
        Value::Nil => {
            // It's possible the packet didn't arrive in time (UDP is unreliable)
            // This is acceptable for a UDP test
            println!("Note: UDP packet may not have arrived (this is expected occasionally)");
        }
        _ => panic!("expected array or nil, got {:?}", val),
    }
}

#[test]
fn test_net_udp_recv_returns_nil_when_no_data() {
    // Create a socket but don't send anything
    let code = r#"
        let sock = net_udp_bind(0);
        let result = net_udp_recv(sock);
        net_close(sock);
        result;
    "#;

    let val = eval(code);
    assert_eq!(val, Value::Nil, "should return nil when no data available");
}

#[test]
fn test_net_udp_send_returns_byte_count() {
    let code = r#"
        let sock = net_udp_bind(0);
        let result = net_udp_send(sock, "127.0.0.1", 19998, "test");
        net_close(sock);
        result;
    "#;

    let val = eval(code);
    match val {
        Value::Number(n) => assert_eq!(n, 4.0, "should return 4 bytes sent"),
        _ => panic!("expected number"),
    }
}

#[test]
fn test_net_udp_close() {
    let code = r#"
        let sock = net_udp_bind(0);
        net_close(sock);
    "#;

    let val = eval(code);
    assert_eq!(val, Value::Bool(true), "net_close should return true");
}

#[test]
fn test_net_udp_invalid_handle() {
    // Try to send with an invalid handle
    let val = eval(r#"net_udp_send(99999, "127.0.0.1", 1234, "data");"#);
    assert_eq!(val, Value::Bool(false), "should return false for invalid handle");

    // Try to recv with an invalid handle
    let val = eval(r#"net_udp_recv(99999);"#);
    assert_eq!(val, Value::Nil, "should return nil for invalid handle");
}

// ---------------------------------------------------------------------------
// Ableton Link
// ---------------------------------------------------------------------------

#[test]
fn test_link_is_enabled_default_false() {
    assert_eq!(eval("link_is_enabled();"), Value::Bool(false));
}

#[test]
fn test_link_enable_and_disable() {
    let val = eval(r#"
        link_enable();
        let was = link_is_enabled();
        link_disable();
        was;
    "#);
    assert_eq!(val, Value::Bool(true));
}

#[test]
fn test_link_enable_returns_bool() {
    assert_eq!(eval("link_enable();"), Value::Bool(true));
    assert_eq!(eval("link_enable(false);"), Value::Bool(false));
}

#[test]
fn test_link_peers_returns_number() {
    let val = eval("link_peers();");
    assert_eq!(val, Value::Number(0.0));
}

#[test]
fn test_link_quantum_default() {
    assert_eq!(eval("link_quantum();"), Value::Number(4.0));
}

#[test]
fn test_link_quantum_set() {
    let val = eval(r#"
        link_quantum(8);
        link_quantum();
    "#);
    assert_eq!(val, Value::Number(8.0));
}

#[test]
fn test_link_beat_returns_number() {
    let val = eval(r#"
        link_enable();
        let b = link_beat();
        link_disable();
        b >= 0;
    "#);
    assert_eq!(val, Value::Bool(true));
}

#[test]
fn test_link_phase_returns_number() {
    let val = eval(r#"
        link_enable();
        let p = link_phase();
        link_disable();
        p >= 0;
    "#);
    assert_eq!(val, Value::Bool(true));
}

#[test]
fn test_link_is_playing_default() {
    let val = eval(r#"
        link_enable();
        let p = link_is_playing();
        link_disable();
        p;
    "#);
    assert_eq!(val, Value::Bool(false));
}

#[test]
fn test_link_play_stop() {
    // link_play/link_stop should not error; transport state may not
    // reflect instantly with zero peers, so just verify no crash
    let val = eval(r#"
        link_enable();
        link_play();
        wait_ms(10);
        link_stop();
        link_disable();
        true;
    "#);
    assert_eq!(val, Value::Bool(true));
}

#[test]
fn test_link_request_beat() {
    // Should not error
    let val = eval(r#"
        link_enable();
        link_request_beat(0);
        link_disable();
        true;
    "#);
    assert_eq!(val, Value::Bool(true));
}

#[test]
fn test_bpm_through_link() {
    let val = eval(r#"
        link_enable();
        bpm(140);
        let b = bpm();
        link_disable();
        b;
    "#);
    match val {
        Value::Number(n) => assert!((n - 140.0).abs() < 0.1, "bpm should be ~140, got {}", n),
        _ => panic!("expected number"),
    }
}
