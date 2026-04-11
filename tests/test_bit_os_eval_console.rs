mod common;
use audion::value::Value;
use common::eval;

// ---------------------------------------------------------------------------
// Bitwise Operators
// ---------------------------------------------------------------------------

#[test]
fn test_bitwise_and() {
    assert_eq!(eval("12 & 10;"), Value::Number(8.0)); // 1100 & 1010 = 1000
}

#[test]
fn test_bitwise_or() {
    assert_eq!(eval("12 | 10;"), Value::Number(14.0)); // 1100 | 1010 = 1110
}

#[test]
fn test_bitwise_xor() {
    assert_eq!(eval("12 ^ 10;"), Value::Number(6.0)); // 1100 ^ 1010 = 0110
}

#[test]
fn test_bitwise_not() {
    assert_eq!(eval("~5;"), Value::Number(-6.0)); // ~0101 = ...1010 (two's complement)
}

#[test]
fn test_left_shift() {
    assert_eq!(eval("5 << 2;"), Value::Number(20.0)); // 101 << 2 = 10100
}

#[test]
fn test_right_shift() {
    assert_eq!(eval("20 >> 2;"), Value::Number(5.0)); // 10100 >> 2 = 101
}

#[test]
fn test_bitwise_precedence() {
    // Test that bitwise operations have correct precedence
    assert_eq!(eval("1 | 2 & 3;"), Value::Number(3.0)); // (1 | (2 & 3)) = (1 | 2) = 3
    assert_eq!(eval("8 >> 1 + 1;"), Value::Number(2.0)); // (8 >> (1 + 1)) = (8 >> 2) = 2
}

// ---------------------------------------------------------------------------
// OS Builtins
// ---------------------------------------------------------------------------

#[test]
fn test_os_env_get_set() {
    // Set and get an environment variable
    assert_eq!(eval(r#"os_env_set("AUDION_TEST", "hello");"#), Value::Nil);
    assert_eq!(eval(r#"os_env_get("AUDION_TEST");"#), Value::String("hello".to_string()));
}

#[test]
fn test_os_env_get_missing() {
    // Getting a non-existent variable should return nil
    assert_eq!(eval(r#"os_env_get("NONEXISTENT_VAR_12345");"#), Value::Nil);
}

#[test]
fn test_os_process_id() {
    // PID should be a positive number
    let pid = eval("os_process_id();");
    match pid {
        Value::Number(n) if n > 0.0 => {},
        _ => panic!("Expected positive number for PID, got {:?}", pid),
    }
}

#[test]
fn test_os_pid_alias() {
    // Test that os_pid() is an alias for os_process_id()
    assert_eq!(eval("os_pid();"), eval("os_process_id();"));
}

#[test]
fn test_os_cwd() {
    // CWD should return a string
    let cwd = eval("os_cwd();");
    assert!(matches!(cwd, Value::String(_)));
}

#[test]
fn test_os_name() {
    // OS name should return a string (linux, macos, or windows)
    let os = eval("os_name();");
    match os {
        Value::String(s) if s == "macos" || s == "linux" || s == "windows" => {},
        _ => panic!("Unexpected OS name: {:?}", os),
    }
}

#[test]
fn test_os_username() {
    // Username should return a non-empty string
    let username = eval("os_username();");
    match username {
        Value::String(s) if !s.is_empty() => {},
        _ => panic!("Expected non-empty string for username, got {:?}", username),
    }
}

#[test]
fn test_os_home() {
    // Home directory should return a string
    let home = eval("os_home();");
    assert!(matches!(home, Value::String(_)));
}

// ---------------------------------------------------------------------------
// eval() Builtin
// ---------------------------------------------------------------------------

#[test]
fn test_eval_simple() {
    assert_eq!(eval(r#"eval("1 + 2;");"#), Value::Number(3.0));
}

#[test]
fn test_eval_string() {
    assert_eq!(eval(r#"eval("\"hello\";");"#), Value::String("hello".to_string()));
}

#[test]
fn test_eval_expression_return() {
    // eval should return the last expression value
    assert_eq!(eval(r#"eval("5 + 10;");"#), Value::Number(15.0));
}

#[test]
fn test_eval_variable_access() {
    // eval should have access to current scope
    assert_eq!(eval(r#"let x = 10; eval("x + 5;");"#), Value::Number(15.0));
}

#[test]
fn test_eval_variable_assignment() {
    // eval should be able to modify variables in current scope
    assert_eq!(eval(r#"let x = 10; eval("x = 20;"); x;"#), Value::Number(20.0));
}

// ---------------------------------------------------------------------------
// Console Error (we can't test interactive I/O easily)
// ---------------------------------------------------------------------------

#[test]
fn test_console_error() {
    // console_error should return nil
    assert_eq!(eval(r#"console_error("test");"#), Value::Nil);
}
