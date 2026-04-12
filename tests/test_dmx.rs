mod common;

use audion::value::Value;

fn eval(src: &str) -> Value {
    common::eval(src)
}

// ---------------------------------------------------------------------------
// dmx_connect
// ---------------------------------------------------------------------------

#[test]
fn test_dmx_connect_returns_bool() {
    let v = eval(r#"dmx_connect("127.0.0.1");"#);
    assert!(matches!(v, Value::Bool(_)), "dmx_connect should return bool");
}

#[test]
fn test_dmx_connect_with_port_returns_bool() {
    let v = eval(r#"dmx_connect("127.0.0.1", 6454);"#);
    assert!(matches!(v, Value::Bool(_)));
}

#[test]
fn test_dmx_connect_loopback_succeeds() {
    // Binding a UDP socket to 0.0.0.0:0 always succeeds on loopback
    let v = eval(r#"dmx_connect("127.0.0.1", 6454);"#);
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn test_dmx_connect_bad_host_fails() {
    // An unparseable address should return false, not panic
    let v = eval(r#"dmx_connect("not-a-valid-address!!::");"#);
    assert_eq!(v, Value::Bool(false));
}

// ---------------------------------------------------------------------------
// dmx_universe
// ---------------------------------------------------------------------------

#[test]
fn test_dmx_universe_returns_nil() {
    let v = eval(r#"
        dmx_connect("127.0.0.1");
        dmx_universe(0);
    "#);
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_universe_large_value() {
    // universe 32767 is the max 15-bit value — should not panic
    let v = eval(r#"
        dmx_connect("127.0.0.1");
        dmx_universe(32767);
    "#);
    assert_eq!(v, Value::Nil);
}

// ---------------------------------------------------------------------------
// dmx_set
// ---------------------------------------------------------------------------

#[test]
fn test_dmx_set_returns_nil() {
    let v = eval("dmx_set(1, 255);");
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_set_first_channel() {
    let v = eval("dmx_set(1, 0);");
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_set_last_channel() {
    let v = eval("dmx_set(512, 128);");
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_set_midrange_value() {
    let v = eval("dmx_set(100, 127);");
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_set_channel_zero_errors() {
    // Channel 0 is out of range (1-indexed)
    let result = std::panic::catch_unwind(|| {
        common::eval("dmx_set(0, 128);")
    });
    assert!(result.is_err(), "dmx_set(0, ...) should error");
}

#[test]
fn test_dmx_set_channel_513_errors() {
    let result = std::panic::catch_unwind(|| {
        common::eval("dmx_set(513, 128);")
    });
    assert!(result.is_err(), "dmx_set(513, ...) should error");
}

// ---------------------------------------------------------------------------
// dmx_set_range
// ---------------------------------------------------------------------------

#[test]
fn test_dmx_set_range_returns_nil() {
    let v = eval("dmx_set_range(1, [255, 0, 128]);");
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_set_range_single_element() {
    let v = eval("dmx_set_range(1, [200]);");
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_set_range_full_universe() {
    // Build an array of 512 values and set them all
    let v = eval(r#"
        let vals = [];
        for (let i = 0; i < 512; i += 1) {
            push(vals, 100);
        }
        dmx_set_range(1, vals);
    "#);
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_set_range_overflow_clips_silently() {
    // Range starting at 510 with 5 values — channels 513+ are ignored
    let v = eval("dmx_set_range(510, [10, 20, 30, 40, 50]);");
    assert_eq!(v, Value::Nil);
}

// ---------------------------------------------------------------------------
// dmx_send
// ---------------------------------------------------------------------------

#[test]
fn test_dmx_send_without_connect_returns_false() {
    // No connection → send should fail gracefully
    let v = eval("dmx_send();");
    assert_eq!(v, Value::Bool(false));
}

#[test]
fn test_dmx_send_after_connect_returns_bool() {
    let v = eval(r#"
        dmx_connect("127.0.0.1");
        dmx_set(1, 200);
        dmx_send();
    "#);
    assert!(matches!(v, Value::Bool(_)));
}

#[test]
fn test_dmx_send_multiple_times() {
    let v = eval(r#"
        dmx_connect("127.0.0.1");
        dmx_set(1, 100);
        dmx_send();
        dmx_set(1, 200);
        dmx_send();
    "#);
    assert!(matches!(v, Value::Bool(_)));
}

// ---------------------------------------------------------------------------
// dmx_blackout
// ---------------------------------------------------------------------------

#[test]
fn test_dmx_blackout_without_connect_returns_nil() {
    // blackout ignores send result — always nil
    let v = eval("dmx_blackout();");
    assert_eq!(v, Value::Nil);
}

#[test]
fn test_dmx_blackout_after_connect() {
    let v = eval(r#"
        dmx_connect("127.0.0.1");
        dmx_set(1, 255);
        dmx_send();
        dmx_blackout();
    "#);
    assert_eq!(v, Value::Nil);
}

// ---------------------------------------------------------------------------
// Workflow: connect → set → send → blackout
// ---------------------------------------------------------------------------

#[test]
fn test_dmx_full_workflow() {
    let v = eval(r#"
        let ok = dmx_connect("127.0.0.1", 6454);
        dmx_universe(0);
        dmx_set(1, 255);
        dmx_set(2, 128);
        dmx_set(3, 64);
        dmx_set_range(10, [200, 180, 160]);
        dmx_send();
        dmx_blackout();
        ok;
    "#);
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn test_dmx_universe_switch() {
    let v = eval(r#"
        dmx_connect("127.0.0.1");
        dmx_universe(0);
        dmx_set(1, 255);
        dmx_send();
        dmx_universe(1);
        dmx_set(1, 128);
        dmx_send();
        dmx_universe(0);
        dmx_blackout();
    "#);
    assert_eq!(v, Value::Nil);
}
