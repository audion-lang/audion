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

//! Watch-mode reload without tearing down the running program.
//!
//! Today's full reload (see `main.rs`'s watch loop) is simple and always
//! correct: join every thread, free every node/buffer, reparse, run from
//! scratch. That's also always *disruptive* — every running sequencer
//! restarts, audio stops, timing state resets, no matter how small the edit.
//!
//! This module finds the cases where that's unnecessary and returns a
//! `ReloadPlan` describing exactly what to do instead — or `None` if
//! something changed that genuinely needs a full reload (a new/removed/
//! reordered top-level statement, a changed non-function `let`, a brand new
//! or removed thread). Three things can be handled without tearing anything
//! down:
//!
//! 1. **A named function's body changed** (`fn x(){}` or `let x = fn(){}`,
//!    at any nesting depth). Every `Value::Function` minted from that
//!    declaration site — no matter how many times it's been called, no
//!    matter which already-running thread is holding a copy — calls through
//!    `Interpreter::fn_registry` by qualified path (see `interpreter.rs`'s
//!    two call trampolines), so overwriting that one registry entry is the
//!    entire patch.
//!
//! 2. **Only a named thread's trailing loop body changed** (the part after
//!    its one-time setup — see `ast::split_thread_body`). The thread isn't
//!    killed; `Interpreter::swap_loop_body` just updates `loop_registry`,
//!    and the already-running thread picks it up at its next iteration
//!    boundary (see `run_thread_body_hot`), with no lost beat and no reset
//!    state.
//!
//! 3. **A named thread's setup changed** (or its loop/no-loop shape itself
//!    changed). The setup already ran once and won't run again on its own,
//!    so this needs `Interpreter::restart_thread`: kill just that one
//!    thread, respawn it against the same environment it was originally
//!    spawned from. Everything *else* in the program is untouched.
//!
//! The qualified-path scheme for (1) must exactly match
//! `Interpreter::qualify_path`: top-level declarations are scoped by file
//! (`"{base_path}/{file}::{name}"`, since two `include`d files can each
//! declare a same-named top-level function and must not collide), and
//! declarations nested inside an already-tracked function extend its path
//! (`"{parent}.{name}"`) — computed the same way regardless of nesting
//! depth, which is why this walker recurses *into* every matched function's
//! body rather than patching it as one opaque blob.

use crate::ast::{self, Expr, Stmt};
use std::sync::Arc;

/// What to do about one named thread that changed — see the module docs.
#[derive(Debug, Clone)]
pub enum ThreadAction {
    /// Only the trailing loop changed; swap it in place via
    /// `Interpreter::swap_loop_body`. No thread is killed.
    LoopSwap { new_loop_body: Arc<Stmt> },
    /// Setup changed (or the loop/no-loop shape itself did); kill and
    /// respawn just this one thread via `Interpreter::restart_thread`.
    Restart { new_body: Arc<Stmt> },
}

/// Everything needed to apply a hot-reload without a full teardown: function
/// bodies to overwrite in `Interpreter::fn_registry`, and per-thread actions
/// to apply via `Interpreter::swap_loop_body`/`restart_thread`.
#[derive(Debug, Default)]
pub struct ReloadPlan {
    pub fn_patches: Vec<(String, Arc<Stmt>)>,
    pub thread_actions: Vec<(String, ThreadAction)>,
}

/// Attempts to explain the difference between `old` and `new` (the
/// previously-running program's top-level statements, and a freshly
/// re-parsed version of the same file) entirely as changes this module
/// knows how to apply without a full reload. Returns `None` if some other
/// part of the program changed and a full reload is required instead.
pub fn diff_for_patch(old: &[Stmt], new: &[Stmt], base_path: &str, file: &str) -> Option<ReloadPlan> {
    let file_scope = format!("{}/{}::", base_path, file);
    let mut plan = ReloadPlan::default();
    if diff_stmt_list(old, new, &file_scope, &mut plan) {
        Some(plan)
    } else {
        None
    }
}

/// `path_prefix` is either a file scope ending in `"::"` (top level) or an
/// already-qualified parent path ending in `"."` (nested one level inside a
/// tracked function) — either way, appending a name directly gives the
/// right qualified path.
fn qualify(path_prefix: &str, name: &str) -> String {
    format!("{}{}", path_prefix, name)
}

/// Compares two statement lists that occupy the same lexical position
/// (either both top-level, or both the direct contents of the same kind of
/// wrapper/function body). Returns `true` (diffable) and appends any
/// patches/actions found, or `false` the moment it finds a difference it
/// can't attribute to a tracked function or thread.
fn diff_stmt_list(old: &[Stmt], new: &[Stmt], path_prefix: &str, plan: &mut ReloadPlan) -> bool {
    if old.len() != new.len() {
        return false;
    }
    old.iter().zip(new.iter()).all(|(o, n)| diff_stmt(o, n, path_prefix, plan))
}

fn diff_stmt(old: &Stmt, new: &Stmt, path_prefix: &str, plan: &mut ReloadPlan) -> bool {
    match (old, new) {
        (
            Stmt::FnDecl { name: on, params: op, body: ob },
            Stmt::FnDecl { name: nn, params: np, body: nb },
        ) if on == nn && dbg_eq(op, np) => diff_fn_body(&qualify(path_prefix, nn), ob, nb, plan),
        (
            Stmt::Let { name: on, init: Some(Expr::FnExpr { params: op, body: ob }), .. },
            Stmt::Let { name: nn, init: Some(Expr::FnExpr { params: np, body: nb }), .. },
        ) if on == nn && dbg_eq(op, np) => diff_fn_body(&qualify(path_prefix, nn), ob, nb, plan),
        (Stmt::Block(ob), Stmt::Block(nb)) => diff_stmt_list(ob, nb, path_prefix, plan),
        (
            Stmt::If { cond: oc, then: ot, else_: oe },
            Stmt::If { cond: nc, then: nt, else_: ne },
        ) => {
            dbg_eq(oc, nc)
                && diff_stmt(ot, nt, path_prefix, plan)
                && match (oe, ne) {
                    (Some(oe), Some(ne)) => diff_stmt(oe, ne, path_prefix, plan),
                    (None, None) => true,
                    _ => false,
                }
        }
        (Stmt::While { cond: oc, body: ob }, Stmt::While { cond: nc, body: nb }) => {
            dbg_eq(oc, nc) && diff_stmt(ob, nb, path_prefix, plan)
        }
        (Stmt::Loop { body: ob }, Stmt::Loop { body: nb }) => diff_stmt(ob, nb, path_prefix, plan),
        (
            Stmt::For { init: oi, cond: oc, incr: oinc, body: ob },
            Stmt::For { init: ni, cond: nc, incr: ninc, body: nb },
        ) => {
            dbg_eq(oc, nc)
                && dbg_eq(oinc, ninc)
                && match (oi, ni) {
                    (Some(oi), Some(ni)) => diff_stmt(oi, ni, path_prefix, plan),
                    (None, None) => true,
                    _ => false,
                }
                && diff_stmt(ob, nb, path_prefix, plan)
        }
        (
            Stmt::ForIn { var: ov, iter: oit, body: ob },
            Stmt::ForIn { var: nv, iter: nit, body: nb },
        ) => ov == nv && dbg_eq(oit, nit) && diff_stmt(ob, nb, path_prefix, plan),
        (Stmt::Thread { name: on, body: ob }, Stmt::Thread { name: nn, body: nb }) => {
            // Unlike every other wrapper, a thread difference never forces
            // a full reload by itself — it always resolves to a
            // LoopSwap/Restart action instead. A name mismatch at the same
            // position is a structural reorder, though, and still falls
            // back (added/removed threads need their spawning code to
            // actually run, which this module doesn't do).
            if on != nn {
                return false;
            }
            diff_thread(on, ob, nb, path_prefix, plan);
            true
        }
        // Anything else (ExprStmt, non-function Let, Return, Break,
        // Continue, SynthDef, Include, Using, or a shape mismatch between
        // old/new) is treated as an opaque, atomic unit: must match exactly.
        _ => dbg_eq(old, new),
    }
}

/// A matched function declaration (same name + params old vs new). If the
/// body is identical there's nothing to do — and nothing nested inside it
/// can have changed either, so no need to recurse. If it differs, this
/// function's own registry entry needs the complete new body (so calling
/// it directly works), *and* we still recurse into it to find any more
/// deeply nested function declarations, since those get their own
/// independent registry entry at runtime and may be called independently
/// of their enclosing function.
fn diff_fn_body(qualified_path: &str, old_body: &Stmt, new_body: &Stmt, plan: &mut ReloadPlan) -> bool {
    if dbg_eq(old_body, new_body) {
        return true;
    }
    plan.fn_patches.push((qualified_path.to_string(), Arc::new(new_body.clone())));
    let nested_prefix = format!("{}.", qualified_path);
    // Best-effort: if the inner shape changed too much to align nested
    // declarations precisely, that's fine — the whole-body patch above
    // already covers this function's own behavior correctly. We only use
    // the nested result to decide whether more *specific* patches get
    // added on top, never to undo the whole-body patch already queued.
    diff_stmt(old_body, new_body, &nested_prefix, plan);
    true
}

/// A matched `thread{}` (same name old vs new). Splits both bodies into
/// one-time setup and an optional trailing loop (`ast::split_thread_body`)
/// and decides which of LoopSwap/Restart applies — see the module docs.
/// Always resolves one way or the other; never signals "give up" to its
/// caller, which is why `diff_stmt`'s `Thread` arm always returns `true`.
fn diff_thread(name: &str, old_body: &Stmt, new_body: &Stmt, path_prefix: &str, plan: &mut ReloadPlan) {
    if dbg_eq(old_body, new_body) {
        return;
    }
    let (old_setup, old_loop) = ast::split_thread_body(old_body);
    let (new_setup, new_loop) = ast::split_thread_body(new_body);

    let shape_matches = old_loop.is_some() == new_loop.is_some();
    // Nested fn declarations in the setup area are scoped exactly like any
    // other wrapper's contents (not extended with the thread's own name —
    // threads don't push anything onto the runtime's `fn_path_stack`
    // either, so this must match that to use the same registry keys).
    let setup_ok = shape_matches && diff_stmt_list(old_setup, new_setup, path_prefix, plan);

    if !setup_ok {
        plan.thread_actions.push((name.to_string(), ThreadAction::Restart {
            new_body: Arc::new(new_body.clone()),
        }));
        return;
    }

    if let (Some(ol), Some(nl)) = (old_loop, new_loop) {
        if !dbg_eq(ol, nl) {
            plan.thread_actions.push((name.to_string(), ThreadAction::LoopSwap {
                new_loop_body: Arc::new(nl.clone()),
            }));
            // Best-effort, same reasoning as diff_fn_body: an independently
            // addressable nested fn inside the new loop body still gets its
            // own registry entry on top of the wholesale loop swap.
            let nested_prefix = format!("{}.", qualify(path_prefix, name));
            diff_stmt(ol, nl, &nested_prefix, plan);
        }
    }
    // (None, None): no trailing loop construct at all — the setup
    // comparison above already covered the entire body.
}

fn dbg_eq<T: std::fmt::Debug>(a: &T, b: &T) -> bool {
    format!("{:?}", a) == format!("{:?}", b)
}
