// Copyright (C) 2025-2026 Aleksandr Bogdanov
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
//

mod common;

use audion::ast::Stmt;
use audion::hotreload;
use audion::value::Value;
use common::{eval, parse_source};

#[test]
fn catch_runs_when_body_errors() {
    let result = eval(
        "try { undefined_function_xyz(); } catch (e) { return 42; }",
    );
    assert_eq!(result, Value::Number(42.0));
}

#[test]
fn catch_var_is_bound_to_the_error_message() {
    let result = eval(
        "try { undefined_function_xyz(); } catch (e) { return e; }",
    );
    match result {
        Value::String(s) => assert!(s.contains("undefined_function_xyz"), "message was: {}", s),
        other => panic!("expected a String, got {:?}", other),
    }
}

#[test]
fn catch_does_not_run_when_body_succeeds() {
    let result = eval(
        "
        let hit_catch = 0;
        try { 1 + 1; } catch (e) { hit_catch = 1; }
        return hit_catch;
        ",
    );
    assert_eq!(result, Value::Number(0.0));
}

#[test]
fn uncaught_error_still_propagates_without_a_try() {
    // Same failure as the tests above, but with no try/catch around it —
    // confirms the default (no wrapping) behavior is unchanged: an error
    // still fails the whole run, exactly as it did before this feature.
    let stmts = parse_source("undefined_function_xyz();");
    let mut interp = common::fresh_test_interp();
    assert!(interp.run_line(&stmts).is_err());
}

#[test]
fn return_from_try_body_is_not_swallowed() {
    // Control flow (return/break/continue) is not a runtime error — it
    // must propagate through `try` same as through any other wrapper, and
    // must NOT invoke catch.
    let result = eval(
        "
        fn f() {
            try { return 7; } catch (e) { return -1; }
        }
        let r = f();
        return r;
        ",
    );
    assert_eq!(result, Value::Number(7.0));
}

#[test]
fn break_from_try_body_inside_a_loop_still_breaks() {
    let result = eval(
        "
        let i = 0;
        while (i < 10) {
            try {
                if (i == 3) { break; }
            } catch (e) {}
            i = i + 1;
        }
        return i;
        ",
    );
    assert_eq!(result, Value::Number(3.0));
}

#[test]
fn error_inside_catch_body_is_not_caught_again() {
    let stmts = parse_source(
        "try { undefined_function_xyz(); } catch (e) { also_undefined_xyz(); }",
    );
    let mut interp = common::fresh_test_interp();
    assert!(interp.run_line(&stmts).is_err());
}

#[test]
fn catch_var_does_not_leak_into_surrounding_scope() {
    let stmts = parse_source(
        "try { undefined_function_xyz(); } catch (e) {} e;",
    );
    let mut interp = common::fresh_test_interp();
    assert!(interp.run_line(&stmts).is_err());
}

// --- hotreload: the walker must recurse into try/catch, not treat it as opaque ---

#[test]
fn diff_reaches_a_fn_body_change_inside_a_try_block() {
    let old = parse_source("fn f() { try { fn helper() { return 1; } } catch (e) {} }");
    let new = parse_source("fn f() { try { fn helper() { return 2; } } catch (e) {} }");
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").expect("should be patchable");
    assert!(plan.fn_patches.iter().any(|(path, _)| path == "./test.au::f.helper"));
}

#[test]
fn diff_reaches_a_change_inside_a_catch_block() {
    let old = parse_source("fn f() { try { risky(); } catch (e) { fn helper() { return 1; } } }");
    let new = parse_source("fn f() { try { risky(); } catch (e) { fn helper() { return 2; } } }");
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").expect("should be patchable");
    assert!(plan.fn_patches.iter().any(|(path, _)| path == "./test.au::f.helper"));
}

#[test]
fn diff_falls_back_when_catch_var_name_changes() {
    // Unwrapped by a named fn: inside one, diff_fn_body's whole-body patch
    // would legitimately absorb this (see diff_reaches_function_nested_...
    // tests) — this checks the Try arm's own fallback trigger directly.
    let old = parse_source("try { risky(); } catch (e) { }");
    let new = parse_source("try { risky(); } catch (err) { }");
    assert!(hotreload::diff_for_patch(&old, &new, ".", "test.au").is_none());
}

#[test]
fn parses_as_try_stmt() {
    let stmts = parse_source("try { 1; } catch (e) { 2; }");
    assert!(matches!(&stmts[0], Stmt::Try { catch_var, .. } if catch_var == "e"));
}
