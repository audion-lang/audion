mod common;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use audion::clock::Clock;
use audion::dmx::DmxClient;
use audion::environment::Environment;
use audion::interpreter::Interpreter;
use audion::lexer::Lexer;
use audion::midi::MidiClient;
use audion::osc::OscClient;
use audion::osc_protocol::OscProtocolClient;
use audion::parser::Parser;
use audion::value::Value;

fn eval(src: &str) -> Value {
    common::eval(src)
}

#[test]
fn test_arithmetic() {
    assert_eq!(eval("2 + 3 * 4;"), Value::Number(14.0));
}

#[test]
fn test_variables() {
    assert_eq!(eval("let x = 10; x + 5;"), Value::Number(15.0));
}

#[test]
fn test_string_concat() {
    assert_eq!(
        eval("\"hello\" + \" \" + \"world\";"),
        Value::String("hello world".to_string())
    );
}

#[test]
fn test_function_call() {
    assert_eq!(
        eval("fn add(a, b) { return a + b; } add(3, 4);"),
        Value::Number(7.0)
    );
}

#[test]
fn test_if_else() {
    assert_eq!(
        eval("let x = 10; if (x > 5) { x = 1; } else { x = 2; } x;"),
        Value::Number(1.0)
    );
}

#[test]
fn test_while_loop() {
    assert_eq!(
        eval("let x = 0; while (x < 5) { x += 1; } x;"),
        Value::Number(5.0)
    );
}

#[test]
fn test_for_loop() {
    assert_eq!(
        eval("let sum = 0; for (let i = 0; i < 5; i += 1) { sum += i; } sum;"),
        Value::Number(10.0)
    );
}

#[test]
fn test_first_class_functions() {
    assert_eq!(
        eval("let f = fn(x) { return x * 2; }; f(21);"),
        Value::Number(42.0)
    );
}

#[test]
fn test_closure() {
    assert_eq!(
        eval("fn make_adder(n) { return fn(x) { return x + n; }; } let add5 = make_adder(5); add5(10);"),
        Value::Number(15.0)
    );
}

#[test]
fn test_bpm_builtin() {
    assert_eq!(eval("bpm();"), Value::Number(120.0));
}

#[test]
fn test_nested_function_calls() {
    assert_eq!(
        eval("fn double(x) { return x * 2; } fn quad(x) { return double(double(x)); } quad(3);"),
        Value::Number(12.0)
    );
}

#[test]
fn test_boolean_logic() {
    assert_eq!(eval("true && false;"), Value::Bool(false));
    assert_eq!(eval("true || false;"), Value::Bool(true));
    assert_eq!(eval("!true;"), Value::Bool(false));
}

#[test]
fn test_comparison() {
    assert_eq!(eval("5 > 3;"), Value::Bool(true));
    assert_eq!(eval("5 == 5;"), Value::Bool(true));
    assert_eq!(eval("5 != 3;"), Value::Bool(true));
}

#[test]
fn test_loop_break() {
    assert_eq!(
        eval("let x = 0; loop { x += 1; if (x == 10) { break; } } x;"),
        Value::Number(10.0)
    );
}

// --- Array tests ---

#[test]
fn test_array_literal_auto_index() {
    assert_eq!(
        eval("let a = [\"x\", \"y\", \"z\"]; a[0];"),
        Value::String("x".to_string())
    );
    assert_eq!(
        eval("let a = [\"x\", \"y\", \"z\"]; a[2];"),
        Value::String("z".to_string())
    );
}

#[test]
fn test_array_literal_key_value() {
    assert_eq!(
        eval("let a = [\"name\" => \"audion\", \"version\" => 1]; a[\"name\"];"),
        Value::String("audion".to_string())
    );
}

#[test]
fn test_array_mixed_keys() {
    assert_eq!(
        eval("let a = [\"key\" => 100, 42, 99]; a[0];"),
        Value::Number(42.0)
    );
    assert_eq!(
        eval("let a = [\"key\" => 100, 42, 99]; a[\"key\"];"),
        Value::Number(100.0)
    );
}

#[test]
fn test_array_index_assign() {
    assert_eq!(
        eval("let a = [1, 2, 3]; a[1] = 20; a[1];"),
        Value::Number(20.0)
    );
}

#[test]
fn test_array_index_assign_new_key() {
    assert_eq!(
        eval("let a = []; a[\"hello\"] = \"world\"; a[\"hello\"];"),
        Value::String("world".to_string())
    );
}

#[test]
fn test_array_nested() {
    assert_eq!(
        eval("let a = [\"inner\" => [10, 20, 30]]; a[\"inner\"][1];"),
        Value::Number(20.0)
    );
}

#[test]
fn test_array_deep_clone() {
    assert_eq!(
        eval("let a = [1, 2, 3]; let b = a; b[0] = 99; a[0];"),
        Value::Number(1.0)
    );
}

#[test]
fn test_array_count() {
    assert_eq!(
        eval("let a = [1, 2, 3]; count(a);"),
        Value::Number(3.0)
    );
}

#[test]
fn test_array_count_nested() {
    assert_eq!(
        eval("let a = [\"inner\" => [10, 20]]; count(a[\"inner\"]);"),
        Value::Number(2.0)
    );
}

#[test]
fn test_array_push() {
    assert_eq!(
        eval("let a = [10, 20]; push(a, 30); a[2];"),
        Value::Number(30.0)
    );
}

#[test]
fn test_array_pop() {
    assert_eq!(
        eval("let a = [10, 20, 30]; pop(a); count(a);"),
        Value::Number(2.0)
    );
}

#[test]
fn test_array_has_key() {
    assert_eq!(
        eval("let a = [\"name\" => \"test\"]; has_key(a, \"name\");"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("let a = [\"name\" => \"test\"]; has_key(a, \"nope\");"),
        Value::Bool(false)
    );
}

#[test]
fn test_array_remove() {
    assert_eq!(
        eval("let a = [\"x\" => 1, \"y\" => 2]; remove(a, \"x\"); count(a);"),
        Value::Number(1.0)
    );
}

#[test]
fn test_array_keys() {
    assert_eq!(
        eval("let a = [\"name\" => \"test\", \"ver\" => 1]; let k = keys(a); k[0];"),
        Value::String("name".to_string())
    );
}

#[test]
fn test_array_with_function() {
    assert_eq!(
        eval("let a = [\"f\" => fn(x) { return x * 2; }]; a[\"f\"](21);"),
        Value::Number(42.0)
    );
}

#[test]
fn test_array_empty() {
    assert_eq!(
        eval("let a = []; count(a);"),
        Value::Number(0.0)
    );
}

#[test]
fn test_array_trailing_comma() {
    assert_eq!(
        eval("let a = [1, 2, 3,]; count(a);"),
        Value::Number(3.0)
    );
}

#[test]
fn test_array_compound_index_assign() {
    assert_eq!(
        eval("let a = [10, 20, 30]; a[1] += 5; let r = a[1]; r;"),
        Value::Number(25.0)
    );
}

#[test]
fn test_array_nil_for_missing_key() {
    assert_eq!(
        eval("let a = [1, 2]; a[99];"),
        Value::Nil
    );
}

// --- Variable function tests ---

#[test]
fn test_variable_function_call() {
    assert_eq!(
        eval("fn greet(x) { return x * 2; } let f = \"greet\"; f(21);"),
        Value::Number(42.0)
    );
}

#[test]
fn test_variable_function_builtin() {
    assert_eq!(
        eval("let f = \"bpm\"; f();"),
        Value::Number(120.0)
    );
}

#[test]
fn test_variable_function_from_assign() {
    assert_eq!(
        eval("fn test(a, b) { return a + b; } let name = \"test\"; name(10, 20);"),
        Value::Number(30.0)
    );
}

#[test]
fn test_array_truthiness() {
    assert_eq!(eval("let a = [1]; !(!a);"), Value::Bool(true));
    assert_eq!(eval("let a = []; !(!a);"), Value::Bool(false));
}

// --- File IO tests ---

#[test]
fn test_file_write_and_read() {
    let path = "/tmp/audion_test_rw.txt";
    eval(&format!("file_write(\"{}\", \"hello audion\");", path));
    assert_eq!(
        eval(&format!("file_read(\"{}\");", path)),
        Value::String("hello audion".to_string())
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_file_write_returns_length() {
    let path = "/tmp/audion_test_len.txt";
    assert_eq!(
        eval(&format!("file_write(\"{}\", \"12345\");", path)),
        Value::Number(5.0)
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_file_read_nonexistent() {
    assert_eq!(
        eval("file_read(\"/tmp/audion_no_such_file_ever.txt\");"),
        Value::Bool(false)
    );
}

#[test]
fn test_file_read_partial_offset_length() {
    let path = "/tmp/audion_test_partial.txt";
    eval(&format!("file_write(\"{}\", \"abcdefghij\");", path));
    assert_eq!(
        eval(&format!("file_read(\"{}\", 2, 4);", path)),
        Value::String("cdef".to_string())
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_file_read_offset_only() {
    let path = "/tmp/audion_test_offset.txt";
    eval(&format!("file_write(\"{}\", \"abcdefghij\");", path));
    assert_eq!(
        eval(&format!("file_read(\"{}\", 5);", path)),
        Value::String("fghij".to_string())
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_file_append() {
    let path = "/tmp/audion_test_append.txt";
    let _ = std::fs::remove_file(path);
    std::fs::write(path, "hello").unwrap();
    eval(&format!(
        "let n = file_append(\"{}\", \" world\"); n;",
        path
    ));
    let contents = std::fs::read_to_string(path).unwrap();
    assert_eq!(contents, "hello world");
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_file_exists() {
    let path = "/tmp/audion_test_exists.txt";
    eval(&format!("file_write(\"{}\", \"x\");", path));
    assert_eq!(
        eval(&format!("file_exists(\"{}\");", path)),
        Value::Bool(true)
    );
    assert_eq!(
        eval("file_exists(\"/tmp/audion_nope_nope_nope.txt\");"),
        Value::Bool(false)
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_file_delete() {
    let path = "/tmp/audion_test_delete.txt";
    eval(&format!(
        "file_write(\"{}\", \"bye\"); let ok = file_delete(\"{}\"); ok;",
        path, path
    ));
    assert!(!std::path::Path::new(path).exists());
}

#[test]
fn test_file_delete_nonexistent() {
    assert_eq!(
        eval("file_delete(\"/tmp/audion_no_such_file_del.txt\");"),
        Value::Bool(false)
    );
}

#[test]
fn test_file_read_false_is_falsy() {
    assert_eq!(
        eval("let d = file_read(\"/tmp/audion_nope.txt\"); !d;"),
        Value::Bool(true)
    );
}

#[test]
fn test_dir_create_and_exists() {
    let _ = std::fs::remove_dir_all("/tmp/audion_test_dir");
    assert_eq!(
        eval("dir_create(\"/tmp/audion_test_dir\");"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("dir_exists(\"/tmp/audion_test_dir\");"),
        Value::Bool(true)
    );
    let _ = std::fs::remove_dir_all("/tmp/audion_test_dir");
}

#[test]
fn test_dir_create_recursive() {
    let _ = std::fs::remove_dir_all("/tmp/audion_test_nested");
    assert_eq!(
        eval("dir_create(\"/tmp/audion_test_nested/a/b/c\");"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("dir_exists(\"/tmp/audion_test_nested/a/b/c\");"),
        Value::Bool(true)
    );
    let _ = std::fs::remove_dir_all("/tmp/audion_test_nested");
}

#[test]
fn test_dir_exists_nonexistent() {
    assert_eq!(
        eval("dir_exists(\"/tmp/audion_nope_dir_xyz\");"),
        Value::Bool(false)
    );
}

#[test]
fn test_dir_exists_on_file() {
    std::fs::write("/tmp/audion_test_not_a_dir", "hi").unwrap();
    assert_eq!(
        eval("dir_exists(\"/tmp/audion_test_not_a_dir\");"),
        Value::Bool(false)
    );
    let _ = std::fs::remove_file("/tmp/audion_test_not_a_dir");
}

#[test]
fn test_dir_scan() {
    let _ = std::fs::remove_dir_all("/tmp/audion_test_scan");
    std::fs::create_dir_all("/tmp/audion_test_scan").unwrap();
    std::fs::write("/tmp/audion_test_scan/a.txt", "").unwrap();
    std::fs::write("/tmp/audion_test_scan/b.txt", "").unwrap();

    let result = eval("let files = dir_scan(\"/tmp/audion_test_scan\"); count(files);");
    assert_eq!(result, Value::Number(2.0));

    let _ = std::fs::remove_dir_all("/tmp/audion_test_scan");
}

#[test]
fn test_dir_scan_nonexistent() {
    assert_eq!(
        eval("dir_scan(\"/tmp/audion_nope_scan_xyz\");"),
        Value::Bool(false)
    );
}

#[test]
fn test_dir_delete() {
    let _ = std::fs::remove_dir_all("/tmp/audion_test_del");
    std::fs::create_dir_all("/tmp/audion_test_del/sub").unwrap();
    std::fs::write("/tmp/audion_test_del/sub/f.txt", "").unwrap();
    assert_eq!(
        eval("let r = dir_delete(\"/tmp/audion_test_del\"); r;"),
        Value::Bool(true)
    );
    assert!(!std::path::Path::new("/tmp/audion_test_del").exists());
}

#[test]
fn test_dir_delete_nonexistent() {
    assert_eq!(
        eval("dir_delete(\"/tmp/audion_nope_del_xyz\");"),
        Value::Bool(false)
    );
}

// --- Object / closure-object tests ---

#[test]
fn test_this_returns_object() {
    let result = eval("fn make() { let x = 42; return this; } let o = make(); o.x;");
    assert_eq!(result, Value::Number(42.0));
}

#[test]
fn test_this_with_closure_method() {
    let result = eval(
        "fn make_counter() { let count = 0; let increment = fn() { count += 1; return count; }; return this; } let c = make_counter(); let r = c.increment(); r;",
    );
    assert_eq!(result, Value::Number(1.0));
}

#[test]
fn test_this_closure_shared_state() {
    let result = eval(
        "fn make_counter() { let count = 0; let increment = fn() { count += 1; return count; }; let get = fn() { return count; }; return this; } let c = make_counter(); let r1 = c.increment(); let r2 = c.increment(); let r = c.get(); r;",
    );
    assert_eq!(result, Value::Number(2.0));
}

#[test]
fn test_this_live_state() {
    let result = eval(
        "fn make() { let count = 0; let inc = fn() { count += 1; }; return this; } let c = make(); let r1 = c.inc(); let r2 = c.inc(); c.count;",
    );
    assert_eq!(result, Value::Number(2.0));
}

#[test]
fn test_this_independent_instances() {
    let result = eval(
        "fn make() { let n = 0; let inc = fn() { n += 1; return n; }; return this; } let a = make(); let b = make(); let r1 = a.inc(); let r2 = a.inc(); let r3 = b.inc(); let r = a.inc(); r;",
    );
    assert_eq!(result, Value::Number(3.0));
}

#[test]
fn test_member_assign_on_object() {
    let result = eval(
        "fn make() { let x = 1; return this; } let o = make(); o.x = 99; let r = o.x; r;",
    );
    assert_eq!(result, Value::Number(99.0));
}

#[test]
fn test_compound_member_assign() {
    let result = eval(
        "fn make() { let x = 10; return this; } let o = make(); let r1 = o.x += 5; let r = o.x; r;",
    );
    assert_eq!(result, Value::Number(15.0));
}

#[test]
fn test_dot_access_on_array() {
    let result = eval("let a = [\"name\" => \"audion\"]; a.name;");
    assert_eq!(result, Value::String("audion".to_string()));
}

#[test]
fn test_dot_assign_on_array() {
    let result = eval("let a = [\"x\" => 1]; a.x = 99; a.x;");
    assert_eq!(result, Value::Number(99.0));
}

#[test]
fn test_dot_access_nil_for_missing() {
    let result = eval("let a = [\"x\" => 1]; a.y;");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_object_deep_clone() {
    let result = eval(
        "fn make() { let n = 0; let inc = fn() { n += 1; return n; }; return this; } let a = make(); let b = a; let r1 = a.inc(); let r2 = a.inc(); let r = b.inc(); r;",
    );
    assert_eq!(result, Value::Number(1.0));
}

// --- Include / namespace tests ---

#[test]
fn test_include_and_namespace() {
    let dir = "/tmp/audion_test_include";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        format!("{}/mylib.au", dir),
        "fn add(a, b) { return a + b; } let PI = 3;",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/mylib.au\" as mylib; mylib::add(10, 20);",
        dir
    ));
    assert_eq!(result, Value::Number(30.0));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_namespace_constant() {
    let dir = "/tmp/audion_test_ns_const";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        format!("{}/constants.au", dir),
        "let PI = 3; let TAU = 6;",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/constants.au\" as constants; constants::PI + constants::TAU;",
        dir
    ));
    assert_eq!(result, Value::Number(9.0));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_include_once() {
    let dir = "/tmp/audion_test_include_once";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        format!("{}/counter.au", dir),
        "let x = 1;",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/counter.au\" as counter; include \"{}/counter.au\" as counter; counter::x;",
        dir, dir
    ));
    assert_eq!(result, Value::Number(1.0));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_coloncolon_token() {
    let mut lexer = Lexer::new("a::b");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(&tokens[0].kind, audion::token::TokenKind::Ident(s) if s == "a"));
    assert!(matches!(tokens[1].kind, audion::token::TokenKind::ColonColon));
    assert!(matches!(&tokens[2].kind, audion::token::TokenKind::Ident(s) if s == "b"));
}

#[test]
fn test_this_keyword_token() {
    let mut lexer = Lexer::new("this");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, audion::token::TokenKind::This));
}

// --- Hierarchical namespace tests ---

#[test]
fn test_hierarchical_namespace() {
    let dir = "/tmp/audion_test_hier_ns";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(format!("{}/some/folder", dir)).unwrap();
    std::fs::write(
        format!("{}/some/folder/file.au", dir),
        "fn greet() { return 42; }",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/some/folder/file.au\" as some::folder::file; some::folder::file::greet();",
        dir
    ));
    assert_eq!(result, Value::Number(42.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_hierarchical_namespace_from_relative_path() {
    let dir = "/tmp/audion_test_hier_rel";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(format!("{}/lib/math", dir)).unwrap();
    std::fs::write(
        format!("{}/lib/math/utils.au", dir),
        "fn double(x) { return x * 2; }",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/lib/math/utils.au\" as lib::math::utils; lib::math::utils::double(5);",
        dir
    ));
    assert_eq!(result, Value::Number(10.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_include_as_single_alias() {
    let dir = "/tmp/audion_test_as_single";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        format!("{}/mylib.au", dir),
        "fn add(a, b) { return a + b; }",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/mylib.au\" as m; m::add(3, 4);",
        dir
    ));
    assert_eq!(result, Value::Number(7.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_include_as_multi_segment_alias() {
    let dir = "/tmp/audion_test_as_multi";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        format!("{}/mylib.au", dir),
        "let X = 99;",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/mylib.au\" as a::b; a::b::X;",
        dir
    ));
    assert_eq!(result, Value::Number(99.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_shared_intermediate_namespace() {
    let dir = "/tmp/audion_test_shared_ns";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(format!("{}/a.au", dir), "let VAL = 1;").unwrap();
    std::fs::write(format!("{}/b.au", dir), "let VAL = 2;").unwrap();

    let result = eval(&format!(
        "include \"{dir}/a.au\" as some::a; include \"{dir}/b.au\" as some::b; some::a::VAL + some::b::VAL;",
    ));
    assert_eq!(result, Value::Number(3.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_using_basic() {
    let dir = "/tmp/audion_test_using_basic";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        format!("{}/mylib.au", dir),
        "fn double(x) { return x * 2; } let C = 10;",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/mylib.au\" as mylib; using mylib; double(5) + C;",
        dir
    ));
    assert_eq!(result, Value::Number(20.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_using_overwrites() {
    let dir = "/tmp/audion_test_using_overwrite";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(format!("{}/a.au", dir), "fn value() { return 1; }").unwrap();
    std::fs::write(format!("{}/b.au", dir), "fn value() { return 2; }").unwrap();

    let result = eval(&format!(
        "include \"{dir}/a.au\" as a; include \"{dir}/b.au\" as b; using a; using b; value();",
    ));
    assert_eq!(result, Value::Number(2.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_using_in_function_scope() {
    let dir = "/tmp/audion_test_using_fn";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        format!("{}/mylib.au", dir),
        "fn get_val() { return 42; }",
    ).unwrap();

    let result = eval(&format!(
        "include \"{}/mylib.au\" as mylib; fn test() {{ using mylib; return get_val(); }} test();",
        dir
    ));
    assert_eq!(result, Value::Number(42.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_using_hierarchical_path() {
    let dir = "/tmp/audion_test_using_hier";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        format!("{}/file.au", dir),
        "fn greet() { return 99; }",
    ).unwrap();

    let result = eval(&format!(
        "include \"{dir}/file.au\" as some::folder::file; using some::folder::file; greet();",
    ));
    assert_eq!(result, Value::Number(99.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_include_once_with_different_alias() {
    let dir = "/tmp/audion_test_inc_once_alias";
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(format!("{}/counter.au", dir), "let x = 1;").unwrap();

    let result = eval(&format!(
        "include \"{dir}/counter.au\" as lib::counter; include \"{dir}/counter.au\" as mycount; mycount::x + lib::counter::x;",
    ));
    assert_eq!(result, Value::Number(2.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_as_keyword_token() {
    let mut lexer = Lexer::new("include \"file.au\" as mylib;");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, audion::token::TokenKind::Include));
    assert!(matches!(&tokens[1].kind, audion::token::TokenKind::StringLit(s) if s == "file.au"));
    assert!(matches!(tokens[2].kind, audion::token::TokenKind::As));
    assert!(matches!(&tokens[3].kind, audion::token::TokenKind::Ident(s) if s == "mylib"));
    assert!(matches!(tokens[4].kind, audion::token::TokenKind::Semicolon));
}

#[test]
fn test_using_keyword_token() {
    let mut lexer = Lexer::new("using some::folder::file;");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, audion::token::TokenKind::Using));
    assert!(matches!(&tokens[1].kind, audion::token::TokenKind::Ident(s) if s == "some"));
    assert!(matches!(tokens[2].kind, audion::token::TokenKind::ColonColon));
    assert!(matches!(&tokens[3].kind, audion::token::TokenKind::Ident(s) if s == "folder"));
    assert!(matches!(tokens[4].kind, audion::token::TokenKind::ColonColon));
    assert!(matches!(&tokens[5].kind, audion::token::TokenKind::Ident(s) if s == "file"));
    assert!(matches!(tokens[6].kind, audion::token::TokenKind::Semicolon));
}

// --- str_explode / str_join tests ---

#[test]
fn test_str_explode_simple() {
    let result = eval("str_explode(\",\", \"a,b,c\");");
    let expected = eval("[\"a\", \"b\", \"c\"];");
    assert_eq!(result, expected);
}

#[test]
fn test_str_explode_space() {
    let result = eval("str_explode(\" \", \"hello world foo\");");
    let expected = eval("[\"hello\", \"world\", \"foo\"];");
    assert_eq!(result, expected);
}

#[test]
fn test_str_explode_multi_char_delimiter() {
    let result = eval("str_explode(\"::\", \"a::b::c\");");
    let expected = eval("[\"a\", \"b\", \"c\"];");
    assert_eq!(result, expected);
}

#[test]
fn test_str_explode_regex_slash() {
    let result = eval("str_explode(\"/[,;]+/\", \"a,,b;c;;;d\");");
    let expected = eval("[\"a\", \"b\", \"c\", \"d\"];");
    assert_eq!(result, expected);
}

#[test]
fn test_str_explode_regex_braces() {
    let result = eval("str_explode(\"{\\\\s+}\", \"hello   world\tfoo\");");
    let expected = eval("[\"hello\", \"world\", \"foo\"];");
    assert_eq!(result, expected);
}

#[test]
fn test_str_explode_regex_percent() {
    let result = eval("str_explode(\"%%\\\\d+%%\", \"abc123def456ghi\");");
    let expected = eval("[\"abc\", \"def\", \"ghi\"];");
    assert_eq!(result, expected);
}

#[test]
fn test_str_explode_no_match() {
    let result = eval("str_explode(\";\", \"hello world\");");
    let expected = eval("[\"hello world\"];");
    assert_eq!(result, expected);
}

#[test]
fn test_str_explode_empty_string() {
    let result = eval("str_explode(\",\", \"\");");
    let expected = eval("[\"\"];");
    assert_eq!(result, expected);
}

#[test]
fn test_str_join_basic() {
    let result = eval("str_join(\", \", [\"a\", \"b\", \"c\"]);");
    assert_eq!(result, Value::String("a, b, c".to_string()));
}

#[test]
fn test_str_join_empty_glue() {
    let result = eval("str_join(\"\", [\"a\", \"b\", \"c\"]);");
    assert_eq!(result, Value::String("abc".to_string()));
}

#[test]
fn test_str_join_numbers() {
    let result = eval("str_join(\"-\", [1, 2, 3]);");
    assert_eq!(result, Value::String("1-2-3".to_string()));
}

#[test]
fn test_str_join_empty_array() {
    let result = eval("str_join(\",\", []);");
    assert_eq!(result, Value::String("".to_string()));
}

#[test]
fn test_str_join_single_element() {
    let result = eval("str_join(\",\", [\"only\"]);");
    assert_eq!(result, Value::String("only".to_string()));
}

#[test]
fn test_str_explode_then_join_roundtrip() {
    let result = eval("let parts = str_explode(\",\", \"a,b,c\"); str_join(\",\", parts);");
    assert_eq!(result, Value::String("a,b,c".to_string()));
}

// --- JSON encode/decode tests ---

#[test]
fn test_json_encode_object() {
    let result = eval("json_encode([\"name\" => \"audion\", \"version\" => 1]);");
    assert_eq!(
        result,
        Value::String("{\"name\":\"audion\",\"version\":1}".to_string())
    );
}

#[test]
fn test_json_encode_array() {
    let result = eval("json_encode([10, 20, 30]);");
    assert_eq!(result, Value::String("[10,20,30]".to_string()));
}

#[test]
fn test_json_encode_nested() {
    let result = eval("json_encode([\"items\" => [1, 2, 3], \"ok\" => true]);");
    assert_eq!(
        result,
        Value::String("{\"items\":[1,2,3],\"ok\":true}".to_string())
    );
}

#[test]
fn test_json_encode_string() {
    let result = eval("json_encode(\"hello\");");
    assert_eq!(result, Value::String("\"hello\"".to_string()));
}

#[test]
fn test_json_encode_number() {
    let result = eval("json_encode(42);");
    assert_eq!(result, Value::String("42".to_string()));
}

#[test]
fn test_json_encode_nil() {
    let result = eval("json_encode(nil);");
    assert_eq!(result, Value::String("null".to_string()));
}

#[test]
fn test_json_decode_object() {
    let result = eval("let d = json_decode(\"{\\\"name\\\":\\\"audion\\\"}\"); d[\"name\"];");
    assert_eq!(result, Value::String("audion".to_string()));
}

#[test]
fn test_json_decode_array() {
    let result = eval("let d = json_decode(\"[10, 20, 30]\"); d[1];");
    assert_eq!(result, Value::Number(20.0));
}

#[test]
fn test_json_decode_nested() {
    let result = eval(
        "let d = json_decode(\"{\\\"items\\\":[1,2,3]}\"); d[\"items\"][2];",
    );
    assert_eq!(result, Value::Number(3.0));
}

#[test]
fn test_json_decode_null() {
    let result = eval("json_decode(\"null\");");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_json_decode_bool() {
    let result = eval("json_decode(\"true\");");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_json_decode_invalid() {
    let result = eval("json_decode(\"not json{{\");");
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_json_roundtrip() {
    let result = eval(
        "let orig = [\"a\" => 1, \"b\" => [10, 20]]; let s = json_encode(orig); let d = json_decode(s); d[\"b\"][0];",
    );
    assert_eq!(result, Value::Number(10.0));
}

// --- Buffer builtin tests ---

#[test]
fn test_buffer_load_returns_number() {
    let result = eval("buffer_load(\"/tmp/nonexistent.wav\");");
    match result {
        Value::Number(_) => {}
        other => panic!("expected Number, got {:?}", other),
    }
}

#[test]
fn test_buffer_alloc_returns_number() {
    let result = eval("buffer_alloc(65536, 2);");
    match result {
        Value::Number(_) => {}
        other => panic!("expected Number, got {:?}", other),
    }
}

#[test]
fn test_buffer_alloc_default_channels() {
    let result = eval("buffer_alloc(1024);");
    match result {
        Value::Number(_) => {}
        other => panic!("expected Number, got {:?}", other),
    }
}

#[test]
fn test_buffer_free_returns_nil() {
    let result = eval("let b = buffer_load(\"/tmp/test.wav\"); buffer_free(b);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_buffer_read_returns_nil() {
    let result = eval("let b = buffer_alloc(65536); buffer_read(b, \"/tmp/test.wav\");");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_buffer_load_increments_ids() {
    let result = eval(
        "let a = buffer_load(\"/tmp/a.wav\"); let b = buffer_load(\"/tmp/b.wav\"); b - a;",
    );
    assert_eq!(result, Value::Number(1.0));
}

#[test]
fn test_seed_produces_deterministic_rand() {
    let a = eval("seed(42); rand();");
    let b = eval("seed(42); rand();");
    assert_eq!(a, b);
}

#[test]
fn test_seed_string_deterministic() {
    let a = eval("seed(\"hello\"); rand(0, 100);");
    let b = eval("seed(\"hello\"); rand(0, 100);");
    assert_eq!(a, b);
}

#[test]
fn test_seed_false_disables() {
    let result = eval("seed(42); seed(false); rand();");
    match result {
        Value::Number(_) => {}
        other => panic!("expected number, got {:?}", other),
    }
}

#[test]
fn test_array_rand_returns_element() {
    let result = eval("seed(99); let a = [10, 20, 30, 40, 50]; array_rand(a);");
    match result {
        Value::Number(n) => {
            assert!(
                n == 10.0 || n == 20.0 || n == 30.0 || n == 40.0 || n == 50.0,
                "expected one of the array values, got {}",
                n
            );
        }
        other => panic!("expected number, got {:?}", other),
    }
}

#[test]
fn test_array_rand_empty_returns_nil() {
    let result = eval("let a = []; array_rand(a);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_array_rand_deterministic_with_seed() {
    let a = eval("seed(123); let arr = [\"a\", \"b\", \"c\", \"d\"]; array_rand(arr);");
    let b = eval("seed(123); let arr = [\"a\", \"b\", \"c\", \"d\"]; array_rand(arr);");
    assert_eq!(a, b);
}

#[test]
fn test_seed_different_seeds_differ() {
    let a = eval("seed(1); rand();");
    let b = eval("seed(2); rand();");
    assert_ne!(a, b);
}

// --- MIDI builtin tests ---

#[test]
fn test_midi_config_no_args_returns_array() {
    let result = eval("midi_config();");
    match result {
        Value::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }
}

#[test]
fn test_midi_config_bad_index_returns_false() {
    let result = eval("midi_config(9999);");
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_midi_config_bad_name_returns_false() {
    let result = eval("midi_config(\"nonexistent_port_xyz_999\");");
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_midi_note_no_connection_does_not_crash() {
    let result = eval("midi_note(60, 100);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_note_off_via_zero_velocity() {
    let result = eval("midi_note(60, 0);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_note_with_channel() {
    let result = eval("midi_note(60, 100, 10);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_cc_no_crash() {
    let result = eval("midi_cc(1, 64);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_cc_with_channel() {
    let result = eval("midi_cc(7, 100, 2);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_program_no_crash() {
    let result = eval("midi_program(5);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_program_with_channel() {
    let result = eval("midi_program(0, 16);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_out_two_bytes() {
    let result = eval("midi_out(192, 5);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_out_three_bytes() {
    let result = eval("midi_out(144, 60, 127);");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_clock_no_crash() {
    let result = eval("midi_clock();");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_start_no_crash() {
    let result = eval("midi_start();");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_stop_no_crash() {
    let result = eval("midi_stop();");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_panic_no_crash() {
    let result = eval("midi_panic();");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_midi_note_requires_args() {
    let env = Arc::new(Mutex::new(Environment::new()));
    let osc = Arc::new(OscClient::new("127.0.0.1:57110"));
    let midi = Arc::new(MidiClient::new());
    let dmx = Arc::new(DmxClient::new());
    let osc_protocol = Arc::new(OscProtocolClient::new());
    let clock = Arc::new(Clock::new(120.0));
    let shutdown = Arc::new(AtomicBool::new(false));
    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut interp = Interpreter::new(env, osc, midi, dmx, osc_protocol, clock, shutdown, false, synthdef_cache);

    let mut lex = Lexer::new("midi_note(60);");
    let tokens = lex.tokenize().unwrap();
    let mut par = Parser::new(tokens);
    let stmts = par.parse().unwrap();
    assert!(interp.run_line(&stmts).is_err());
}

#[test]
fn test_midi_cc_requires_args() {
    let env = Arc::new(Mutex::new(Environment::new()));
    let osc = Arc::new(OscClient::new("127.0.0.1:57110"));
    let midi = Arc::new(MidiClient::new());
    let dmx = Arc::new(DmxClient::new());
    let osc_protocol = Arc::new(OscProtocolClient::new());
    let clock = Arc::new(Clock::new(120.0));
    let shutdown = Arc::new(AtomicBool::new(false));
    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut interp = Interpreter::new(env, osc, midi, dmx, osc_protocol, clock, shutdown, false, synthdef_cache);

    let mut lex = Lexer::new("midi_cc(1);");
    let tokens = lex.tokenize().unwrap();
    let mut par = Parser::new(tokens);
    let stmts = par.parse().unwrap();
    assert!(interp.run_line(&stmts).is_err());
}

// ----- Array cursor tests -----

#[test]
fn test_array_beginning() {
    assert_eq!(
        eval("let a = [10, 20, 30]; array_beginning(a);"),
        Value::Number(10.0)
    );
}

#[test]
fn test_array_end() {
    assert_eq!(
        eval("let a = [10, 20, 30]; array_end(a);"),
        Value::Number(30.0)
    );
}

#[test]
fn test_array_next() {
    assert_eq!(
        eval("let a = [10, 20, 30]; let r1 = array_beginning(a); let r2 = array_next(a); r2;"),
        Value::Number(20.0)
    );
}

#[test]
fn test_array_prev() {
    assert_eq!(
        eval("let a = [10, 20, 30]; let r1 = array_end(a); let r2 = array_prev(a); r2;"),
        Value::Number(20.0)
    );
}

#[test]
fn test_array_current() {
    assert_eq!(
        eval("let a = [10, 20, 30]; let r = array_beginning(a); let r2 = array_next(a); array_current(a);"),
        Value::Number(20.0)
    );
}

#[test]
fn test_array_key_cursor() {
    assert_eq!(
        eval("let a = [\"name\" => \"test\", \"ver\" => 1]; let r = array_beginning(a); array_key(a);"),
        Value::String("name".to_string())
    );
}

#[test]
fn test_array_cursor_past_end() {
    assert_eq!(
        eval("let a = [10]; let r = array_end(a); array_next(a, false);"),
        Value::Nil
    );
}

#[test]
fn test_array_cursor_before_beginning() {
    assert_eq!(
        eval("let a = [10]; let r = array_beginning(a); array_prev(a, false);"),
        Value::Nil
    );
}

#[test]
fn test_array_current_empty() {
    assert_eq!(
        eval("let a = []; array_current(a);"),
        Value::Nil
    );
}

#[test]
fn test_array_push_alias() {
    assert_eq!(
        eval("let a = [10]; array_push(a, 20); a[1];"),
        Value::Number(20.0)
    );
}

#[test]
fn test_array_pop_alias() {
    assert_eq!(
        eval("let a = [10, 20]; let r = array_pop(a); r;"),
        Value::Number(20.0)
    );
}

// --- OSC protocol tests ---

#[test]
fn test_osc_config_no_args_returns_nil() {
    let result = eval("osc_config();");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_osc_config_sets_target() {
    let result = eval("osc_config(\"127.0.0.1:9000\");");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_osc_config_returns_target_after_set() {
    let result = eval("osc_config(\"127.0.0.1:9000\"); osc_config();");
    assert_eq!(result, Value::String("127.0.0.1:9000".to_string()));
}

#[test]
fn test_osc_send_without_config_returns_false() {
    let result = eval("osc_send(\"/test\", 42);");
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_osc_send_with_config_returns_true() {
    let result = eval("osc_config(\"127.0.0.1:9999\"); osc_send(\"/test\", 1, 2, 3);");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_osc_send_no_args_is_error() {
    let env = Arc::new(Mutex::new(Environment::new()));
    let osc = Arc::new(OscClient::new("127.0.0.1:57110"));
    let midi = Arc::new(MidiClient::new());
    let dmx = Arc::new(DmxClient::new());
    let osc_protocol = Arc::new(OscProtocolClient::new());
    let clock = Arc::new(Clock::new(120.0));
    let shutdown = Arc::new(AtomicBool::new(false));
    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut interp = Interpreter::new(env, osc, midi, dmx, osc_protocol, clock, shutdown, false, synthdef_cache);

    let mut lex = Lexer::new("osc_send();");
    let tokens = lex.tokenize().unwrap();
    let mut par = Parser::new(tokens);
    let stmts = par.parse().unwrap();
    assert!(interp.run_line(&stmts).is_err());
}

#[test]
fn test_osc_recv_no_listener_returns_nil() {
    let result = eval("osc_recv();");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_osc_close_no_crash() {
    let result = eval("osc_close();");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_osc_close_all_no_crash() {
    let result = eval("osc_config(\"127.0.0.1:9000\"); osc_close(\"all\");");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_osc_close_sender_no_crash() {
    let result = eval("osc_config(\"127.0.0.1:9000\"); osc_close(\"sender\");");
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_osc_send_loopback() {
    let env = Arc::new(Mutex::new(Environment::new()));
    let osc = Arc::new(OscClient::new("127.0.0.1:57110"));
    let midi = Arc::new(MidiClient::new());
    let dmx = Arc::new(DmxClient::new());
    let osc_protocol = Arc::new(OscProtocolClient::new());
    let clock = Arc::new(Clock::new(120.0));
    let shutdown = Arc::new(AtomicBool::new(false));
    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut interp = Interpreter::new(env, osc, midi, dmx, osc_protocol, clock, shutdown, false, synthdef_cache);

    let src = r#"
        osc_listen(19876);
        osc_config("127.0.0.1:19876");
        osc_send("/hello", 42, "world");
        wait_ms(50);
        let msg = osc_recv();
        msg[0];
    "#;
    let mut lex = Lexer::new(src);
    let tokens = lex.tokenize().unwrap();
    let mut par = Parser::new(tokens);
    let stmts = par.parse().unwrap();
    let result = interp.run_line(&stmts).unwrap();
    assert_eq!(result, Value::String("/hello".to_string()));
}

// ── Tail Call Optimization Tests ──

#[test]
fn test_tco_self_recursion() {
    // Without TCO this would overflow the stack
    let result = eval(r#"
        fn countdown(n) {
            if (n <= 0) { return n; }
            return countdown(n - 1);
        }
        countdown(50000);
    "#);
    assert_eq!(result, Value::Number(0.0));
}

#[test]
fn test_tco_accumulator_pattern() {
    let result = eval(r#"
        fn sum(n, acc) {
            if (n <= 0) { return acc; }
            return sum(n - 1, acc + n);
        }
        sum(50000, 0);
    "#);
    assert_eq!(result, Value::Number(1250025000.0));
}

#[test]
fn test_tco_mutual_recursion() {
    let result = eval(r#"
        fn is_even(n) {
            if (n == 0) { return true; }
            return is_odd(n - 1);
        }
        fn is_odd(n) {
            if (n == 0) { return false; }
            return is_even(n - 1);
        }
        is_even(50000);
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_tco_mutual_recursion_odd() {
    let result = eval(r#"
        fn is_even(n) {
            if (n == 0) { return true; }
            return is_odd(n - 1);
        }
        fn is_odd(n) {
            if (n == 0) { return false; }
            return is_even(n - 1);
        }
        is_odd(50001);
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_tco_preserves_closures() {
    let result = eval(r#"
        fn make_adder(x) {
            return fn(y) { return x + y; };
        }
        let add5 = make_adder(5);
        add5(10);
    "#);
    assert_eq!(result, Value::Number(15.0));
}

#[test]
fn test_tco_tail_call_to_different_closure() {
    let result = eval(r#"
        let base = 100;
        fn add_base(n) {
            return base + n;
        }
        fn go(n) {
            if (n <= 0) { return add_base(n); }
            return go(n - 1);
        }
        go(10);
    "#);
    assert_eq!(result, Value::Number(100.0));
}

#[test]
fn test_tco_return_this_not_affected() {
    // return this; is NOT a tail call (it's Expr::This, not Expr::Call)
    // Make sure objects still work correctly
    // Note: use let bindings to avoid run_line double-eval of ExprStmt
    let result = eval(r#"
        fn Counter(start) {
            let count = start;
            fn get() { return count; }
            fn increment() {
                count += 1;
                return this;
            }
            return this;
        }
        let c = Counter(0);
        let c = c.increment();
        let c = c.increment();
        c.get();
    "#);
    assert_eq!(result, Value::Number(2.0));
}

#[test]
fn test_tco_tail_call_returning_object() {
    // return make_object(); IS a tail call — object should still work
    let result = eval(r#"
        fn make_obj(val) {
            let x = val;
            fn get() { return x; }
            return this;
        }
        fn wrapper(n) {
            return make_obj(n * 2);
        }
        let obj = wrapper(21);
        obj.get();
    "#);
    assert_eq!(result, Value::Number(42.0));
}

#[test]
fn test_tco_non_tail_recursion_still_works() {
    // Non-tail recursion (result used after call) should still work normally
    let result = eval(r#"
        fn factorial(n) {
            if (n <= 1) { return 1; }
            return n * factorial(n - 1);
        }
        factorial(10);
    "#);
    assert_eq!(result, Value::Number(3628800.0));
}

#[test]
fn test_tco_tail_call_in_loop() {
    let result = eval(r#"
        fn helper(n) {
            return n + 1;
        }
        fn go(n) {
            if (n >= 10) { return helper(n); }
            return go(n + 1);
        }
        go(0);
    "#);
    assert_eq!(result, Value::Number(11.0));
}

#[test]
fn test_tco_tail_call_to_builtin() {
    let result = eval(r#"
        fn go(n) {
            if (n <= 0) { return math_abs(-42); }
            return go(n - 1);
        }
        go(100);
    "#);
    assert_eq!(result, Value::Number(42.0));
}

#[test]
fn test_tco_fibonacci_accumulator() {
    let result = eval(r#"
        fn fib(n, a, b) {
            if (n <= 0) { return a; }
            return fib(n - 1, b, a + b);
        }
        fib(50, 0, 1);
    "#);
    assert_eq!(result, Value::Number(12586269025.0));
}

// ===== Default arguments =====

#[test]
fn test_default_arg_used_when_omitted() {
    let result = eval(r#"
        fn greet(name, greeting = "Hello") {
            return greeting + " " + name;
        }
        greet("world");
    "#);
    assert_eq!(result, Value::String("Hello world".to_string()));
}

#[test]
fn test_default_arg_overridden_positionally() {
    let result = eval(r#"
        fn greet(name, greeting = "Hello") {
            return greeting + " " + name;
        }
        greet("world", "Hey");
    "#);
    assert_eq!(result, Value::String("Hey world".to_string()));
}

#[test]
fn test_multiple_defaults() {
    let result = eval(r#"
        fn add(a, b = 10, c = 100) {
            return a + b + c;
        }
        add(1);
    "#);
    assert_eq!(result, Value::Number(111.0));
}

#[test]
fn test_partial_defaults_positional() {
    let result = eval(r#"
        fn add(a, b = 10, c = 100) {
            return a + b + c;
        }
        add(1, 2);
    "#);
    assert_eq!(result, Value::Number(103.0));
}

#[test]
fn test_all_defaults_overridden() {
    let result = eval(r#"
        fn add(a, b = 10, c = 100) {
            return a + b + c;
        }
        add(1, 2, 3);
    "#);
    assert_eq!(result, Value::Number(6.0));
}

#[test]
fn test_default_numeric_expression() {
    let result = eval(r#"
        fn offset(x, delta = 2 * 3) {
            return x + delta;
        }
        offset(4);
    "#);
    assert_eq!(result, Value::Number(10.0));
}

#[test]
fn test_default_nil() {
    let result = eval(r#"
        fn maybe(x, fallback = nil) {
            return fallback;
        }
        maybe(1);
    "#);
    assert_eq!(result, Value::Nil);
}

#[test]
fn test_default_bool() {
    let result = eval(r#"
        fn flag(x, enabled = true) {
            return enabled;
        }
        flag(0);
    "#);
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_lambda_with_default() {
    let result = eval(r#"
        let f = fn(x, y = 5) { return x + y; };
        f(3);
    "#);
    assert_eq!(result, Value::Number(8.0));
}

#[test]
fn test_lambda_with_default_overridden() {
    let result = eval(r#"
        let f = fn(x, y = 5) { return x + y; };
        f(3, 10);
    "#);
    assert_eq!(result, Value::Number(13.0));
}

#[test]
fn test_too_many_args_error() {
    let result = std::panic::catch_unwind(|| {
        eval(r#"
            fn f(a, b) { return a + b; }
            f(1, 2, 3);
        "#)
    });
    assert!(result.is_err());
}

#[test]
fn test_missing_required_arg_error() {
    let result = std::panic::catch_unwind(|| {
        eval(r#"
            fn f(a, b) { return a + b; }
            f(1);
        "#)
    });
    assert!(result.is_err());
}

// ===== Named arguments to userland functions =====

#[test]
fn test_named_arg_basic() {
    let result = eval(r#"
        fn sub(a, b) { return a - b; }
        sub(b: 3, a: 10);
    "#);
    assert_eq!(result, Value::Number(7.0));
}

#[test]
fn test_named_arg_with_positional() {
    let result = eval(r#"
        fn sub(a, b) { return a - b; }
        sub(10, b: 3);
    "#);
    assert_eq!(result, Value::Number(7.0));
}

#[test]
fn test_named_arg_fills_default() {
    let result = eval(r#"
        fn greet(name, greeting = "Hello") {
            return greeting + " " + name;
        }
        greet("world", greeting: "Howdy");
    "#);
    assert_eq!(result, Value::String("Howdy world".to_string()));
}

#[test]
fn test_named_arg_skip_middle_default() {
    let result = eval(r#"
        fn add(a, b = 10, c = 100) {
            return a + b + c;
        }
        add(1, c: 5);
    "#);
    assert_eq!(result, Value::Number(16.0));
}

#[test]
fn test_named_arg_all_named() {
    let result = eval(r#"
        fn box_vol(w, h, d) { return w * h * d; }
        box_vol(d: 3, h: 2, w: 4);
    "#);
    assert_eq!(result, Value::Number(24.0));
}

#[test]
fn test_named_arg_unknown_param_error() {
    let result = std::panic::catch_unwind(|| {
        eval(r#"
            fn f(a, b) { return a + b; }
            f(1, z: 2);
        "#)
    });
    assert!(result.is_err());
}

#[test]
fn test_named_arg_duplicate_error() {
    let result = std::panic::catch_unwind(|| {
        eval(r#"
            fn f(a, b) { return a + b; }
            f(1, a: 99);
        "#)
    });
    assert!(result.is_err());
}

#[test]
fn test_named_arg_in_lambda() {
    let result = eval(r#"
        let f = fn(x, y) { return x - y; };
        f(y: 1, x: 9);
    "#);
    assert_eq!(result, Value::Number(8.0));
}

#[test]
fn test_default_and_named_combined() {
    let result = eval(r#"
        fn describe(thing, adj = "cool", verb = "is") {
            return thing + " " + verb + " " + adj;
        }
        describe("audion", adj: "awesome");
    "#);
    assert_eq!(result, Value::String("audion is awesome".to_string()));
}
