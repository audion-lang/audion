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

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use audion::clock::Clock;
use audion::dmx::DmxClient;
use audion::environment::Environment;
use audion::hotreload::{self, ThreadAction};
use audion::interpreter::Interpreter;
use audion::midi::MidiClient;
use audion::osc::OscClient;
use audion::osc_protocol::OscProtocolClient;
use audion::value::Value;
use common::parse_source;

fn fresh_interp(fn_registry: audion::interpreter::FnRegistry) -> Interpreter {
    let env = Arc::new(Mutex::new(Environment::new()));
    let osc = Arc::new(OscClient::new("127.0.0.1:57110"));
    let midi = Arc::new(MidiClient::new());
    let dmx = Arc::new(DmxClient::new());
    let osc_protocol = Arc::new(OscProtocolClient::new());
    let clock = Arc::new(Clock::new(120.0));
    let shutdown = Arc::new(AtomicBool::new(false));
    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let define_cache = Arc::new(Mutex::new(audion::define_cache::DefineCache::new()));
    let loop_registry = Arc::new(Mutex::new(std::collections::HashMap::new()));
    Interpreter::new(env, osc, midi, dmx, osc_protocol, clock, shutdown, false, synthdef_cache, define_cache, fn_registry, loop_registry)
}

// --- diff_for_patch: function-body patching ---

#[test]
fn diff_detects_simple_top_level_fn_body_change() {
    let old = parse_source("fn foo() { return 1; }");
    let new = parse_source("fn foo() { return 2; }");
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").expect("should be patchable");
    assert_eq!(plan.fn_patches.len(), 1);
    assert_eq!(plan.fn_patches[0].0, "./test.au::foo");
    assert!(plan.thread_actions.is_empty());
}

#[test]
fn diff_is_noop_when_nothing_changed() {
    let old = parse_source("fn foo() { return 1; }");
    let new = parse_source("fn foo() { return 1; }");
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").expect("should be patchable");
    assert!(plan.fn_patches.is_empty());
    assert!(plan.thread_actions.is_empty());
}

#[test]
fn diff_reaches_function_nested_inside_a_factory() {
    let old = parse_source(
        "let make = fn() { fn helper() { return 1; } return this; };",
    );
    let new = parse_source(
        "let make = fn() { fn helper() { return 2; } return this; };",
    );
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").expect("should be patchable");
    // Two patches, by design: the outer `make` entry gets its complete new
    // body (in case something calls `make()` again), and `helper` gets its
    // own independent entry (what every already-running `obj.helper()`
    // caller actually resolves through) — see `diff_fn_body` in hotreload.rs.
    assert_eq!(plan.fn_patches.len(), 2);
    assert!(plan.fn_patches.iter().any(|(path, _)| path == "./test.au::make.helper"));
    assert!(plan.fn_patches.iter().any(|(path, _)| path == "./test.au::make"));
}

#[test]
fn diff_falls_back_on_new_top_level_statement() {
    let old = parse_source("fn foo() { return 1; }");
    let new = parse_source("fn foo() { return 1; } let x = 5;");
    assert!(hotreload::diff_for_patch(&old, &new, ".", "test.au").is_none());
}

#[test]
fn diff_scopes_same_named_fn_in_different_files_separately() {
    let old = parse_source("fn value() { return 1; }");
    let new = parse_source("fn value() { return 2; }");
    let a = hotreload::diff_for_patch(&old, &new, ".", "a.au").unwrap();
    let b = hotreload::diff_for_patch(&old, &new, ".", "b.au").unwrap();
    assert_ne!(a.fn_patches[0].0, b.fn_patches[0].0);
}

// --- diff_for_patch: thread setup vs. loop-body changes ---

#[test]
fn diff_produces_loop_swap_for_changed_loop_body() {
    let old = parse_source("thread t { loop { wait(1); } }");
    let new = parse_source("thread t { loop { wait(2); } }");
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").expect("thread diffs never fall back");
    assert!(plan.fn_patches.is_empty());
    assert_eq!(plan.thread_actions.len(), 1);
    assert_eq!(plan.thread_actions[0].0, "t");
    assert!(matches!(plan.thread_actions[0].1, ThreadAction::LoopSwap { .. }));
}

#[test]
fn diff_produces_restart_for_changed_setup() {
    let old = parse_source("thread t { let n = 0; loop { wait(1); } }");
    let new = parse_source("thread t { let n = 1; loop { wait(1); } }");
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").unwrap();
    assert_eq!(plan.thread_actions.len(), 1);
    assert_eq!(plan.thread_actions[0].0, "t");
    assert!(matches!(plan.thread_actions[0].1, ThreadAction::Restart { .. }));
}

#[test]
fn diff_produces_restart_when_loop_shape_itself_changes() {
    // Going from a trailing `loop{}` to no loop at all (or vice versa) isn't
    // a loop-body edit — it's a structural change to the thread, so it
    // needs a restart, not a swap.
    let old = parse_source("thread t { loop { wait(1); } }");
    let new = parse_source("thread t { wait(1); }");
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").unwrap();
    assert_eq!(plan.thread_actions.len(), 1);
    assert!(matches!(plan.thread_actions[0].1, ThreadAction::Restart { .. }));
}

#[test]
fn diff_still_patches_a_fn_declared_inside_a_changed_thread() {
    // A named fn inside a thread's setup is patched like anywhere else
    // (not restarted), *and* the loop-body change alongside it still gets
    // its own action — the two are independent.
    let old = parse_source("thread t { fn helper() { return 1; } loop { wait(1); } }");
    let new = parse_source("thread t { fn helper() { return 2; } loop { wait(2); } }");
    let plan = hotreload::diff_for_patch(&old, &new, ".", "test.au").unwrap();
    assert_eq!(plan.fn_patches.len(), 1);
    assert_eq!(plan.fn_patches[0].0, "./test.au::helper");
    assert_eq!(plan.thread_actions.len(), 1);
    assert!(matches!(plan.thread_actions[0].1, ThreadAction::LoopSwap { .. }));
}

#[test]
fn diff_falls_back_on_thread_renamed_at_same_position() {
    let old = parse_source("thread t { loop { wait(1); } }");
    let new = parse_source("thread u { loop { wait(1); } }");
    assert!(hotreload::diff_for_patch(&old, &new, ".", "test.au").is_none());
}

// --- End to end: patch actually takes effect on a live, already-called instance ---

#[test]
fn patched_registry_is_picked_up_by_already_running_instance() {
    let fn_registry: audion::interpreter::FnRegistry = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut interp = fresh_interp(fn_registry.clone());
    interp.current_file = "test.au".to_string();

    let v1 = "let make = fn() { fn helper() { return 1; } return this; };";
    let old_stmts = parse_source(v1);
    interp.run_line(&old_stmts).unwrap();
    interp.run_line(&parse_source("let obj = make();")).unwrap();

    // Before patching: calls the original body.
    let result = interp.run_line(&parse_source("obj.helper();")).unwrap();
    assert_eq!(result, Value::Number(1.0));

    // Patch, without ever re-running `make()` or touching `obj`.
    let v2 = "let make = fn() { fn helper() { return 2; } return this; };";
    let new_stmts = parse_source(v2);
    let plan = hotreload::diff_for_patch(&old_stmts, &new_stmts, ".", "test.au").unwrap();
    // Outer `make` entry plus `helper`'s own independent entry — see the
    // comment in diff_reaches_function_nested_inside_a_factory.
    assert_eq!(plan.fn_patches.len(), 2);
    {
        let mut registry = fn_registry.lock().unwrap();
        for (path, body) in plan.fn_patches {
            registry.insert(path, body);
        }
    }

    // Same `obj`, same closure, but the call now runs the new body.
    let result = interp.run_line(&parse_source("obj.helper();")).unwrap();
    assert_eq!(result, Value::Number(2.0));
}

#[test]
fn unrelated_live_state_survives_a_patch() {
    // The whole point: patching a sibling function must not reset state
    // that a long-lived object is carrying.
    let fn_registry: audion::interpreter::FnRegistry = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut interp = fresh_interp(fn_registry.clone());
    interp.current_file = "test.au".to_string();

    let v1 = "
        let make = fn() {
            let counter = 0;
            fn bump() { counter = counter + 1; return counter; }
            fn other() { return 1; }
            return this;
        };
    ";
    let old_stmts = parse_source(v1);
    interp.run_line(&old_stmts).unwrap();
    interp.run_line(&parse_source("let obj = make();")).unwrap();

    assert_eq!(interp.run_line(&parse_source("obj.bump();")).unwrap(), Value::Number(1.0));
    assert_eq!(interp.run_line(&parse_source("obj.bump();")).unwrap(), Value::Number(2.0));

    // Patch only `other`; `bump`'s counter state must be untouched.
    let v2 = "
        let make = fn() {
            let counter = 0;
            fn bump() { counter = counter + 1; return counter; }
            fn other() { return 42; }
            return this;
        };
    ";
    let new_stmts = parse_source(v2);
    let plan = hotreload::diff_for_patch(&old_stmts, &new_stmts, ".", "test.au").unwrap();
    assert_eq!(plan.fn_patches.len(), 2);
    assert!(plan.fn_patches.iter().any(|(path, _)| path == "./test.au::make.other"));
    {
        let mut registry = fn_registry.lock().unwrap();
        for (path, body) in plan.fn_patches {
            registry.insert(path, body);
        }
    }

    assert_eq!(interp.run_line(&parse_source("obj.other();")).unwrap(), Value::Number(42.0));
    // Counter kept counting from where it left off — not reset to 0.
    assert_eq!(interp.run_line(&parse_source("obj.bump();")).unwrap(), Value::Number(3.0));
}

// --- A loop-body swap through the real public API is immediately visible ---
//
// This doesn't spawn a real `thread{}` (timing-sensitive and nondeterministic
// in a fast unit test, and not a pattern used elsewhere in this suite) — it
// exercises `Interpreter::swap_loop_body`/`current_loop_body`, the exact
// pair a real spawned thread's hot-loop poll relies on (see
// `run_thread_body_hot`). The full real-process version of this (edit a
// running script's thread body, watch the counter keep incrementing with
// new behavior and no reset) was verified manually against the `audion`
// binary in watch mode.
#[test]
fn swap_loop_body_is_visible_through_current_loop_body() {
    let fn_registry: audion::interpreter::FnRegistry = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let interp = fresh_interp(fn_registry);
    assert!(interp.current_loop_body("t").is_none());

    let patched = parse_source("loop { wait(2); }");
    interp.swap_loop_body("t", Arc::new(patched[0].clone()));

    let current = interp.current_loop_body("t").expect("swap should have registered an entry");
    assert_eq!(format!("{:?}", current), format!("{:?}", patched[0]));
}
