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

use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::ast::*;
use crate::builtins;
use crate::clock::Clock;
use crate::environment::Environment;
use crate::error::{AudionError, Result};
use crate::midi::MidiClient;
use crate::osc::OscClient;
use crate::osc_protocol::OscProtocolClient;
use crate::value::{AudionArray, Value};

pub enum ControlFlow {
    None,
    Break,
    Continue,
    Return(Value),
    TailCall {
        callee: Value,
        positional: Vec<Value>,
        named: Vec<(String, Value)>,
    },
}

/// Convert an include path string to namespace segments.
/// e.g. "some/folder/file.au" → ["some", "folder", "file"]
fn path_to_namespace_segments(path: &str) -> Vec<String> {
    let p = std::path::Path::new(path);
    let without_ext = p.with_extension("");
    without_ext
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
            _ => None, // Skip RootDir (/), CurDir (.), ParentDir (..)
        })
        .collect()
}

/// Hash a SynthDef AST for caching purposes.
/// Uses Debug representation to create a stable hash of the full AST.
fn hash_synthdef(name: &str, params: &[String], body: &UGenExpr) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    for param in params {
        param.hash(&mut hasher);
    }
    // Use Debug format as a stable representation of the AST
    format!("{:?}", body).hash(&mut hasher);
    hasher.finish()
}

/// Cache entry for a compiled SynthDef: (ast_hash, compiled_bytes)
pub type SynthDefCache = Arc<Mutex<HashMap<String, (u64, Vec<u8>)>>>;

pub struct Interpreter {
    pub env: Arc<Mutex<Environment>>,
    pub osc: Arc<OscClient>,
    pub midi: Arc<MidiClient>,
    pub osc_protocol: Arc<OscProtocolClient>,
    pub clock: Arc<Clock>,
    pub shutdown: Arc<AtomicBool>,
    thread_handles: HashMap<String, JoinHandle<()>>,

    pub base_path: PathBuf,
    included_files: Arc<Mutex<HashSet<PathBuf>>>,
    included_envs: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<Environment>>>>>,
    pub debug_sclang: bool,
    /// In-memory cache of compiled SynthDefs (watch mode only).
    /// Maps synthdef name -> (ast_hash, compiled_bytes).
    /// Persists across reloads within a single watch session.
    pub synthdef_cache: SynthDefCache,
}

impl Interpreter {
    pub fn new(
        env: Arc<Mutex<Environment>>,
        osc: Arc<OscClient>,
        midi: Arc<MidiClient>,
        osc_protocol: Arc<OscProtocolClient>,
        clock: Arc<Clock>,
        shutdown: Arc<AtomicBool>,
        debug_sclang: bool,
        synthdef_cache: SynthDefCache,
    ) -> Self {
        // Register builtins in the environment (single source of truth: builtins::BUILTIN_NAMES)
        {
            let mut e = env.lock().unwrap();
            for name in builtins::BUILTIN_NAMES {
                e.define(name.to_string(), Value::BuiltinFn(name.to_string()));
            }
        }

        Interpreter {
            env,
            osc,
            midi,
            osc_protocol,
            clock,
            shutdown,
            thread_handles: HashMap::new(),

            base_path: PathBuf::from("."),
            included_files: Arc::new(Mutex::new(HashSet::new())),
            included_envs: Arc::new(Mutex::new(HashMap::new())),
            debug_sclang,
            synthdef_cache,
        }
    }

    pub fn new_for_thread(
        env: Arc<Mutex<Environment>>,
        osc: Arc<OscClient>,
        midi: Arc<MidiClient>,
        osc_protocol: Arc<OscProtocolClient>,
        clock: Arc<Clock>,
        shutdown: Arc<AtomicBool>,
        debug_sclang: bool,
        synthdef_cache: SynthDefCache,
        base_path: PathBuf,
    ) -> Self {
        Interpreter {
            env,
            osc,
            midi,
            osc_protocol,
            clock,
            shutdown,
            thread_handles: HashMap::new(),

            base_path,
            included_files: Arc::new(Mutex::new(HashSet::new())),
            included_envs: Arc::new(Mutex::new(HashMap::new())),
            debug_sclang,
            synthdef_cache,
        }
    }

    pub fn set_base_path(&mut self, path: PathBuf) {
        self.base_path = path;
    }

    /// Store command-line arguments in the environment as __ARGS__ array
    pub fn set_args(&mut self, args: Vec<String>) {
        let mut arr = AudionArray::new();
        for (i, arg) in args.iter().enumerate() {
            arr.set(Value::Number(i as f64), Value::String(arg.clone()));
        }
        self.env.lock().unwrap().define(
            "__ARGS__".to_string(),
            Value::Array(Arc::new(Mutex::new(arr))),
        );
    }

    pub fn run(&mut self, stmts: &[Stmt]) -> Result<Value> {
        let last = self.run_without_join(stmts)?;
        self.join_threads();
        Ok(last)
    }

    /// Run all statements and call main() if defined, but do NOT join threads.
    /// Used by --watch mode so threads keep running until a file change is detected.
    pub fn run_without_join(&mut self, stmts: &[Stmt]) -> Result<Value> {
        let mut last = Value::Nil;
        for stmt in stmts {
            match self.exec_stmt(stmt)? {
                ControlFlow::Return(v) => return Ok(v),
                ControlFlow::TailCall { .. } => return Ok(Value::Nil),
                ControlFlow::Break => {
                    return Err(AudionError::RuntimeError {
                        msg: "break outside of loop".to_string(),
                    })
                }
                ControlFlow::Continue => {
                    return Err(AudionError::RuntimeError {
                        msg: "continue outside of loop".to_string(),
                    })
                }
                ControlFlow::None => {}
            }
            // Track last expression value for REPL
            if let Stmt::ExprStmt(_) = stmt {
                // we just need something — grab from env or recalculate
            }
        }

        // After top-level execution, check for main()
        let has_main = {
            let e = self.env.lock().unwrap();
            matches!(e.get("main"), Some(Value::Function { .. }))
        };
        if has_main {
            last = self.call_function("main", &[], &[])?;
        }

        Ok(last)
    }

    pub fn run_line(&mut self, stmts: &[Stmt]) -> Result<Value> {
        let mut last = Value::Nil;
        for stmt in stmts {
            // For ExprStmt, evaluate once and capture the value directly
            // (exec_stmt evaluates but discards it, so we handle it here to avoid double evaluation)
            if let Stmt::ExprStmt(expr) = stmt {
                last = self.eval_expr(expr)?;
                continue;
            }
            match self.exec_stmt(stmt)? {
                ControlFlow::Return(v) => return Ok(v),
                ControlFlow::TailCall { .. } => return Ok(Value::Nil),
                ControlFlow::Break | ControlFlow::Continue => {}
                ControlFlow::None => {}
            }
        }
        Ok(last)
    }

    pub fn exec_stmt(&mut self, stmt: &Stmt) -> Result<ControlFlow> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Ok(ControlFlow::Return(Value::Nil));
        }

        match stmt {
            Stmt::ExprStmt(expr) => {
                self.eval_expr(expr)?;
                Ok(ControlFlow::None)
            }
            Stmt::Let { name, init } => {
                let val = match init {
                    Some(expr) => self.eval_expr(expr)?.deep_clone(),
                    None => Value::Nil,
                };
                self.env.lock().unwrap().define(name.clone(), val);
                Ok(ControlFlow::None)
            }
            Stmt::Block(stmts) => {
                let parent = self.env.clone();
                let child = Arc::new(Mutex::new(Environment::new_child(parent.clone())));
                let old_env = std::mem::replace(&mut self.env, child);
                let result = self.exec_block(stmts);
                self.env = old_env;
                result
            }
            Stmt::If { cond, then, else_ } => {
                let val = self.eval_expr(cond)?;
                if val.is_truthy() {
                    self.exec_stmt(then)
                } else if let Some(else_stmt) = else_ {
                    self.exec_stmt(else_stmt)
                } else {
                    Ok(ControlFlow::None)
                }
            }
            Stmt::While { cond, body } => {
                loop {
                    if self.shutdown.load(Ordering::Relaxed) {
                        return Ok(ControlFlow::Return(Value::Nil));
                    }
                    let val = self.eval_expr(cond)?;
                    if !val.is_truthy() {
                        break;
                    }
                    match self.exec_stmt(body)? {
                        ControlFlow::Break => break,
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                        tc @ ControlFlow::TailCall { .. } => return Ok(tc),
                        _ => {}
                    }
                }
                Ok(ControlFlow::None)
            }
            Stmt::Loop { body } => {
                loop {
                    if self.shutdown.load(Ordering::Relaxed) {
                        return Ok(ControlFlow::Return(Value::Nil));
                    }
                    match self.exec_stmt(body)? {
                        ControlFlow::Break => break,
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                        tc @ ControlFlow::TailCall { .. } => return Ok(tc),
                        _ => {}
                    }
                }
                Ok(ControlFlow::None)
            }
            Stmt::For {
                init,
                cond,
                incr,
                body,
            } => {
                // Create a scope for the for loop
                let parent = self.env.clone();
                let child = Arc::new(Mutex::new(Environment::new_child(parent.clone())));
                let old_env = std::mem::replace(&mut self.env, child);

                if let Some(init_stmt) = init {
                    self.exec_stmt(init_stmt)?;
                }

                loop {
                    if self.shutdown.load(Ordering::Relaxed) {
                        self.env = old_env;
                        return Ok(ControlFlow::Return(Value::Nil));
                    }
                    if let Some(cond_expr) = cond {
                        let val = self.eval_expr(cond_expr)?;
                        if !val.is_truthy() {
                            break;
                        }
                    }
                    match self.exec_stmt(body)? {
                        ControlFlow::Break => break,
                        ControlFlow::Return(v) => {
                            self.env = old_env;
                            return Ok(ControlFlow::Return(v));
                        }
                        tc @ ControlFlow::TailCall { .. } => {
                            self.env = old_env;
                            return Ok(tc);
                        }
                        _ => {}
                    }
                    if let Some(incr_expr) = incr {
                        self.eval_expr(incr_expr)?;
                    }
                }
                self.env = old_env;
                Ok(ControlFlow::None)
            }
            Stmt::Return(expr) => {
                match expr {
                    Some(Expr::Call { callee, args }) => {
                        // Tail call optimization: don't evaluate the call,
                        // return a TailCall so the trampoline can reuse the frame
                        let callee_val = self.eval_expr(callee)?;
                        let mut eval_args: Vec<(Value, Option<String>)> = Vec::new();
                        for arg in args {
                            match arg {
                                Arg::Positional(expr) => {
                                    let val = self.eval_expr(expr)?;
                                    eval_args.push((val, None));
                                }
                                Arg::Named { name, value } => {
                                    let val = self.eval_expr(value)?;
                                    eval_args.push((val, Some(name.clone())));
                                }
                            }
                        }
                        let (positional, named) = builtins::split_args(&eval_args);

                        // Resolve string callee to function value
                        let callee_val = if let Value::String(ref name) = callee_val {
                            self.env.lock().unwrap().get(name).ok_or_else(|| {
                                AudionError::RuntimeError {
                                    msg: format!("undefined function '{}'", name),
                                }
                            })?
                        } else {
                            callee_val
                        };

                        Ok(ControlFlow::TailCall { callee: callee_val, positional, named })
                    }
                    Some(e) => Ok(ControlFlow::Return(self.eval_expr(e)?)),
                    None => Ok(ControlFlow::Return(Value::Nil)),
                }
            }
            Stmt::Break => Ok(ControlFlow::Break),
            Stmt::Continue => Ok(ControlFlow::Continue),
            Stmt::FnDecl { name, params, body } => {
                let func = Value::Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: *body.clone(),
                    closure: self.env.clone(),
                };
                self.env.lock().unwrap().define(name.clone(), func);
                Ok(ControlFlow::None)
            }
            Stmt::Thread { name, body } => {
                self.exec_thread(name, body);
                Ok(ControlFlow::None)
            }
            Stmt::SynthDef { name, params, body } => {
                // ALWAYS load samples into buffers (even on cache hit)
                // because watch mode calls free_all_buffers() on reload
                let sample_paths = crate::synthdef::collect_sample_paths(body);
                let mut buffers = Vec::new();
                for path_str in &sample_paths {
                    let path = std::path::Path::new(path_str);
                    let abs_path = if path.is_absolute() {
                        path_str.clone()
                    } else {
                        std::env::current_dir()
                            .map(|d| d.join(path_str).to_string_lossy().to_string())
                            .unwrap_or_else(|_| path_str.clone())
                    };

                    let num_channels =
                        crate::sampler::detect_channels(std::path::Path::new(&abs_path));
                    let buffer_id = self.osc.buffer_alloc_read(&abs_path);

                    buffers.push(crate::synthdef::BufferInfo {
                        file_path: abs_path,
                        buffer_id,
                        num_channels,
                    });
                }

                // Compute hash of this SynthDef's AST
                let ast_hash = hash_synthdef(name, params, body);

                // Check cache for compiled SynthDef bytecode
                let cached_bytes = {
                    let cache = self.synthdef_cache.lock().unwrap();
                    cache.get(name).and_then(|(cached_hash, bytes)| {
                        if *cached_hash == ast_hash {
                            Some(bytes.clone())
                        } else {
                            None
                        }
                    })
                };

                let bytes = if let Some(cached) = cached_bytes {
                    // Cache hit - skip sclang compilation, reuse compiled bytes
                    if buffers.is_empty() {
                        eprintln!("  reusing cached synth '{}'", name);
                    } else {
                        eprintln!(
                            "  reusing cached synth '{}' ({} sample{})",
                            name,
                            buffers.len(),
                            if buffers.len() == 1 { "" } else { "s" }
                        );
                    }
                    cached
                } else {
                    // Cache miss - compile via sclang
                    let out_dir = crate::sclang::synthdef_output_dir();
                    let sclang_code =
                        crate::synthdef::generate_sclang(name, params, body, &out_dir, &buffers);
                    if self.debug_sclang {
                        eprintln!("\n=== SC code for '{}' ===\n{}", name, sclang_code);
                    }
                    let compiled = crate::sclang::compile_synthdef(name, &sclang_code)?;

                    // Store in cache
                    {
                        let mut cache = self.synthdef_cache.lock().unwrap();
                        cache.insert(name.clone(), (ast_hash, compiled.clone()));
                    }

                    if buffers.is_empty() {
                        println!("defined synth '{}'", name);
                    } else {
                        println!(
                            "defined synth '{}' ({} sample{})",
                            name,
                            buffers.len(),
                            if buffers.len() == 1 { "" } else { "s" }
                        );
                    }

                    compiled
                };

                // Load the SynthDef (cached or freshly compiled) onto the server
                self.osc.load_synthdef(&bytes);

                Ok(ControlFlow::None)
            }
            Stmt::Include { path, alias } => {
                self.exec_include(path, alias.as_deref())?;
                Ok(ControlFlow::None)
            }
            Stmt::Using { path } => {
                self.exec_using(path)?;
                Ok(ControlFlow::None)
            }
        }
    }

    fn exec_include(&mut self, path: &str, alias: Option<&[String]>) -> Result<()> {
        // Resolve path relative to current file's directory
        let file_path = self.base_path.join(path);
        let canonical = file_path.canonicalize().map_err(|e| AudionError::RuntimeError {
            msg: format!("cannot resolve include path '{}': {}", path, e),
        })?;

        // Include-once: skip re-execution but still install namespace under (possibly new) alias
        {
            let included = self.included_files.lock().unwrap();
            if included.contains(&canonical) {
                let envs = self.included_envs.lock().unwrap();
                if let Some(cached_env) = envs.get(&canonical) {
                    let segments = if let Some(alias_segments) = alias {
                        alias_segments.to_vec()
                    } else {
                        path_to_namespace_segments(path)
                    };
                    self.install_namespace(&segments, cached_env.clone());
                }
                return Ok(());
            }
        }

        // Read the file
        let source = std::fs::read_to_string(&canonical).map_err(|e| AudionError::RuntimeError {
            msg: format!("cannot read '{}': {}", path, e),
        })?;

        // Lex and parse
        let mut lex = crate::lexer::Lexer::new(&source);
        let tokens = lex.tokenize()?;
        let mut par = crate::parser::Parser::new(tokens);
        let stmts = par.parse()?;

        // Execute in a fresh child environment (inherits builtins from parent)
        let include_env = Arc::new(Mutex::new(Environment::new_child(self.env.clone())));
        let old_env = std::mem::replace(&mut self.env, include_env.clone());
        let old_base = std::mem::replace(
            &mut self.base_path,
            canonical.parent().unwrap_or(&PathBuf::from(".")).to_path_buf(),
        );

        // Execute all statements (but don't auto-call main)
        for stmt in &stmts {
            match self.exec_stmt(stmt)? {
                ControlFlow::Return(_) | ControlFlow::TailCall { .. } => break,
                _ => {}
            }
        }

        // Restore environment and base path
        self.env = old_env;
        self.base_path = old_base;

        // Mark as included and cache the environment
        {
            self.included_files.lock().unwrap().insert(canonical.clone());
        }
        {
            self.included_envs.lock().unwrap().insert(canonical, include_env.clone());
        }

        // Determine namespace segments and install
        let segments = if let Some(alias_segments) = alias {
            alias_segments.to_vec()
        } else {
            path_to_namespace_segments(path)
        };
        self.install_namespace(&segments, include_env);

        Ok(())
    }

    fn install_namespace(&self, segments: &[String], leaf_env: Arc<Mutex<Environment>>) {
        assert!(!segments.is_empty(), "namespace segments must not be empty");

        if segments.len() == 1 {
            self.env
                .lock()
                .unwrap()
                .define(segments[0].clone(), Value::Namespace(leaf_env));
            return;
        }

        // Multi-segment: walk/create intermediate namespaces
        let mut current_env = self.env.clone();

        for (i, segment) in segments.iter().enumerate() {
            let is_last = i == segments.len() - 1;

            if is_last {
                current_env
                    .lock()
                    .unwrap()
                    .define(segment.clone(), Value::Namespace(leaf_env.clone()));
            } else {
                // Get or create intermediate namespace
                let existing = current_env.lock().unwrap().get(segment);
                match existing {
                    Some(Value::Namespace(ns_env)) => {
                        current_env = ns_env;
                    }
                    _ => {
                        let new_ns_env = Arc::new(Mutex::new(Environment::new()));
                        current_env
                            .lock()
                            .unwrap()
                            .define(segment.clone(), Value::Namespace(new_ns_env.clone()));
                        current_env = new_ns_env;
                    }
                }
            }
        }
    }

    fn exec_using(&mut self, path: &[String]) -> Result<()> {
        let target_env = self.resolve_namespace_path(path)?;

        // Copy all local bindings from the namespace into the current scope
        let bindings: Vec<(String, Value)> = {
            let env = target_env.lock().unwrap();
            env.values()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };

        {
            let mut current = self.env.lock().unwrap();
            for (name, value) in bindings {
                current.define(name, value);
            }
        }

        Ok(())
    }

    fn resolve_namespace_path(
        &self,
        path: &[String],
    ) -> Result<Arc<Mutex<Environment>>> {
        assert!(!path.is_empty());

        let first = self.env.lock().unwrap().get(&path[0]);
        let mut current = match first {
            Some(Value::Namespace(env)) => env,
            Some(other) => {
                return Err(AudionError::RuntimeError {
                    msg: format!("'{}' is not a namespace (is {})", path[0], other.type_name()),
                });
            }
            None => {
                return Err(AudionError::RuntimeError {
                    msg: format!("undefined namespace '{}'", path[0]),
                });
            }
        };

        for segment in &path[1..] {
            let next = current.lock().unwrap().get(segment);
            current = match next {
                Some(Value::Namespace(env)) => env,
                Some(other) => {
                    return Err(AudionError::RuntimeError {
                        msg: format!("'{}' is not a namespace (is {})", segment, other.type_name()),
                    });
                }
                None => {
                    return Err(AudionError::RuntimeError {
                        msg: format!("undefined '{}' in namespace path", segment),
                    });
                }
            };
        }

        Ok(current)
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<ControlFlow> {
        for stmt in stmts {
            let flow = self.exec_stmt(stmt)?;
            match flow {
                ControlFlow::None => {}
                other => return Ok(other),
            }
        }
        Ok(ControlFlow::None)
    }

    fn exec_thread(&mut self, name: &str, body: &Stmt) {
        let child_env = Arc::new(Mutex::new(Environment::new_child(self.env.clone())));
        let body = body.clone();
        let osc = self.osc.clone();
        let midi = self.midi.clone();
        let osc_protocol = self.osc_protocol.clone();
        let clock = self.clock.clone();
        let shutdown = self.shutdown.clone();
        let debug_sclang = self.debug_sclang;
        let thread_name = name.to_string();

        let synthdef_cache = self.synthdef_cache.clone();
        let base_path = self.base_path.clone();
        let handle = std::thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || {
                let mut interp =
                    Interpreter::new_for_thread(child_env, osc, midi, osc_protocol, clock, shutdown, debug_sclang, synthdef_cache, base_path);
                if let Err(e) = interp.exec_stmt(&body) {
                    eprintln!("thread '{}' error: {}", thread_name, e);
                }
            })
            .expect("failed to spawn thread");

        self.thread_handles.insert(name.to_string(), handle);
    }

    pub fn join_threads(&mut self) {
        let handles: HashMap<String, JoinHandle<()>> =
            std::mem::take(&mut self.thread_handles);
        for (name, handle) in handles {
            if let Err(_) = handle.join() {
                eprintln!("thread '{}' panicked", name);
            }
        }
    }

    // --- Expressions ---

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::StringLit(s) => Ok(Value::String(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Nil => Ok(Value::Nil),
            Expr::Ident(name) => {
                let val = self.env.lock().unwrap().get(name);
                match val {
                    Some(v) => Ok(v),
                    None => Err(AudionError::RuntimeError {
                        msg: format!("undefined variable '{}'", name),
                    }),
                }
            }
            Expr::Assign { name, value } => {
                let val = self.eval_expr(value)?.deep_clone();
                if !self.env.lock().unwrap().set(name, val.clone()) {
                    // If not found in any scope, define in current scope
                    self.env.lock().unwrap().define(name.clone(), val.clone());
                }
                Ok(val)
            }
            Expr::CompoundAssign { name, op, value } => {
                let current = self.env.lock().unwrap().get(name);
                let current = current.ok_or_else(|| AudionError::RuntimeError {
                    msg: format!("undefined variable '{}'", name),
                })?;
                let rhs = self.eval_expr(value)?;
                let result = self.eval_binop(op, &current, &rhs)?;
                self.env.lock().unwrap().set(name, result.clone());
                Ok(result)
            }
            Expr::BinOp { left, op, right } => {
                // Short-circuit for && and ||
                if matches!(op, BinOp::And) {
                    let l = self.eval_expr(left)?;
                    if !l.is_truthy() {
                        return Ok(l);
                    }
                    return self.eval_expr(right);
                }
                if matches!(op, BinOp::Or) {
                    let l = self.eval_expr(left)?;
                    if l.is_truthy() {
                        return Ok(l);
                    }
                    return self.eval_expr(right);
                }

                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                self.eval_binop(op, &l, &r)
            }
            Expr::UnaryOp { op, expr } => {
                let val = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg => match val {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        _ => Err(AudionError::RuntimeError {
                            msg: format!("cannot negate {}", val.type_name()),
                        }),
                    },
                    UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
                    UnaryOp::BitNot => match val {
                        Value::Number(n) => Ok(Value::Number((!(n as i64)) as f64)),
                        _ => Err(AudionError::RuntimeError {
                            msg: format!("cannot apply bitwise NOT to {}", val.type_name()),
                        }),
                    },
                }
            }
            Expr::Call { callee, args } => {
                let callee_val = self.eval_expr(callee)?;

                // Evaluate all arguments, tracking positional vs named
                let mut eval_args: Vec<(Value, Option<String>)> = Vec::new();
                for arg in args {
                    match arg {
                        Arg::Positional(expr) => {
                            let val = self.eval_expr(expr)?;
                            eval_args.push((val, None));
                        }
                        Arg::Named { name, value } => {
                            let val = self.eval_expr(value)?;
                            eval_args.push((val, Some(name.clone())));
                        }
                    }
                }

                let (positional, named) = builtins::split_args(&eval_args);

                // If callee is a string, resolve it by name (variable functions)
                let callee_val = if let Value::String(ref name) = callee_val {
                    self.env.lock().unwrap().get(name).ok_or_else(|| {
                        AudionError::RuntimeError {
                            msg: format!("undefined function '{}'", name),
                        }
                    })?
                } else {
                    callee_val
                };

                match callee_val {
                    Value::BuiltinFn(name) => {
                        // Special handling for eval() - needs interpreter context
                        if name == "eval" {
                            if positional.is_empty() {
                                return Err(AudionError::RuntimeError {
                                    msg: "eval() requires a code string".to_string(),
                                });
                            }
                            let code = match &positional[0] {
                                Value::String(s) => s.clone(),
                                _ => return Err(AudionError::RuntimeError {
                                    msg: "eval() requires a string argument".to_string(),
                                }),
                            };

                            // Tokenize and parse the code
                            let mut lexer = crate::lexer::Lexer::new(&code);
                            let tokens = lexer.tokenize()?;
                            let mut parser = crate::parser::Parser::new(tokens);
                            let stmts = parser.parse()?;

                            // Execute statements and return last value (like run_line)
                            let mut last = Value::Nil;
                            for stmt in &stmts {
                                match self.exec_stmt(stmt)? {
                                    ControlFlow::Return(v) => return Ok(v),
                                    ControlFlow::TailCall { .. } => return Ok(Value::Nil),
                                    ControlFlow::Break | ControlFlow::Continue => {}
                                    ControlFlow::None => {}
                                }
                                if let Stmt::ExprStmt(expr) = stmt {
                                    last = self.eval_expr(expr)?;
                                }
                            }
                            Ok(last)
                        } else {
                            builtins::call_builtin(&name, &positional, &named, &self.osc, &self.midi, &self.osc_protocol, &self.clock, &self.env, &self.shutdown, &self.base_path)
                        }
                    }
                    Value::Function {
                        name,
                        params,
                        body,
                        closure,
                    } => {
                        // Trampoline loop for tail call optimization
                        let mut cur_name = name;
                        let mut cur_params = params;
                        let mut cur_body = body;
                        let mut cur_closure = closure;
                        let mut cur_positional = positional;
                        let mut _cur_named = named;

                        loop {
                            let bindings = self.bind_args(&cur_name, &cur_params, &cur_positional, &_cur_named)?;

                            // Create new scope from closure (not current env)
                            let call_env = Arc::new(Mutex::new(Environment::new_child(cur_closure)));
                            {
                                let mut env = call_env.lock().unwrap();
                                for (name, val) in &bindings {
                                    env.define(name.clone(), val.clone());
                                }
                            }

                            let old_env = std::mem::replace(&mut self.env, call_env);
                            let result = self.exec_stmt(&cur_body);
                            self.env = old_env;

                            match result? {
                                ControlFlow::Return(v) => return Ok(v),
                                ControlFlow::TailCall { callee, positional: tc_pos, named: tc_named } => {
                                    // Resolve string callee
                                    let callee = if let Value::String(ref s) = callee {
                                        self.env.lock().unwrap().get(s).ok_or_else(|| {
                                            AudionError::RuntimeError {
                                                msg: format!("undefined function '{}'", s),
                                            }
                                        })?
                                    } else {
                                        callee
                                    };

                                    match callee {
                                        Value::Function { name, params, body, closure } => {
                                            cur_name = name;
                                            cur_params = params;
                                            cur_body = body;
                                            cur_closure = closure;
                                            cur_positional = tc_pos;
                                            _cur_named = tc_named;
                                            continue;
                                        }
                                        Value::BuiltinFn(bname) => {
                                            if bname == "eval" {
                                                // eval needs interpreter context, can't trampoline
                                                return Err(AudionError::RuntimeError {
                                                    msg: "tail call to eval() is not supported".to_string(),
                                                });
                                            }
                                            return builtins::call_builtin(&bname, &tc_pos, &tc_named, &self.osc, &self.midi, &self.osc_protocol, &self.clock, &self.env, &self.shutdown, &self.base_path);
                                        }
                                        other => {
                                            return Err(AudionError::RuntimeError {
                                                msg: format!("'{}' is not callable", other.type_name()),
                                            });
                                        }
                                    }
                                }
                                _ => return Ok(Value::Nil),
                            }
                        }
                    }
                    other => Err(AudionError::RuntimeError {
                        msg: format!("'{}' is not callable", other.type_name()),
                    }),
                }
            }
            Expr::FnExpr { params, body } => Ok(Value::Function {
                name: "<anonymous>".to_string(),
                params: params.clone(),
                body: *body.clone(),
                closure: self.env.clone(),
            }),
            Expr::ArrayLit { elements } => {
                let mut arr = AudionArray::new();
                for (key_expr, val_expr) in elements {
                    let val = self.eval_expr(val_expr)?;
                    if let Some(k) = key_expr {
                        let key = self.eval_expr(k)?;
                        arr.set(key, val);
                    } else {
                        arr.push_auto(val);
                    }
                }
                Ok(Value::Array(Arc::new(Mutex::new(arr))))
            }
            Expr::Index { object, index } => {
                let obj = self.eval_expr(object)?;
                let idx = self.eval_expr(index)?;
                match obj {
                    Value::Array(arr) => {
                        let guard = arr.lock().unwrap();
                        match guard.get(&idx) {
                            Some(v) => Ok(v.deep_clone()),
                            None => Ok(Value::Nil),
                        }
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("cannot index into {}", obj.type_name()),
                    }),
                }
            }
            Expr::IndexAssign {
                object,
                index,
                value,
            } => {
                let obj = self.eval_expr(object)?;
                let idx = self.eval_expr(index)?;
                let val = self.eval_expr(value)?;
                match obj {
                    Value::Array(arr) => {
                        let mut guard = arr.lock().unwrap();
                        guard.set(idx, val.clone());
                        Ok(val)
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("cannot index-assign into {}", obj.type_name()),
                    }),
                }
            }
            Expr::CompoundIndexAssign {
                object,
                index,
                op,
                value,
            } => {
                let obj = self.eval_expr(object)?;
                let idx = self.eval_expr(index)?;
                let rhs = self.eval_expr(value)?;
                match obj {
                    Value::Array(arr) => {
                        let mut guard = arr.lock().unwrap();
                        if let Some(current) = guard.get(&idx) {
                            let current_clone = current.clone();
                            let result = self.eval_binop(op, &current_clone, &rhs)?;
                            guard.set(idx, result.clone());
                            Ok(result)
                        } else {
                            Err(AudionError::RuntimeError {
                                msg: "array key not found for compound assignment".to_string(),
                            })
                        }
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("cannot index-assign into {}", obj.type_name()),
                    }),
                }
            }
            Expr::This => {
                Ok(Value::Object(self.env.clone()))
            }
            Expr::MemberAccess { object, field } => {
                let obj = self.eval_expr(object)?;
                match obj {
                    Value::Object(env) => {
                        let e = env.lock().unwrap();
                        Ok(e.get(field).unwrap_or(Value::Nil))
                    }
                    Value::Namespace(env) => {
                        let e = env.lock().unwrap();
                        e.get(field).ok_or_else(|| AudionError::RuntimeError {
                            msg: format!("undefined member '{}' in namespace", field),
                        })
                    }
                    Value::Array(arr) => {
                        let guard = arr.lock().unwrap();
                        let key = Value::String(field.clone());
                        match guard.get(&key) {
                            Some(v) => Ok(v.deep_clone()),
                            None => Ok(Value::Nil),
                        }
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("cannot access member '{}' on {}", field, obj.type_name()),
                    }),
                }
            }
            Expr::MemberAssign { object, field, value } => {
                let obj = self.eval_expr(object)?;
                let val = self.eval_expr(value)?;
                match obj {
                    Value::Object(env) => {
                        let mut e = env.lock().unwrap();
                        if !e.set(field, val.clone()) {
                            e.define(field.clone(), val.clone());
                        }
                        Ok(val)
                    }
                    Value::Array(arr) => {
                        let mut guard = arr.lock().unwrap();
                        let key = Value::String(field.clone());
                        guard.set(key, val.clone());
                        Ok(val)
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("cannot assign member '{}' on {}", field, obj.type_name()),
                    }),
                }
            }
            Expr::CompoundMemberAssign { object, field, op, value } => {
                let obj = self.eval_expr(object)?;
                let rhs = self.eval_expr(value)?;
                match obj {
                    Value::Object(env) => {
                        let mut e = env.lock().unwrap();
                        let current = e.get(field).ok_or_else(|| AudionError::RuntimeError {
                            msg: format!("undefined member '{}' for compound assignment", field),
                        })?;
                        let result = self.eval_binop(op, &current, &rhs)?;
                        e.set(field, result.clone());
                        Ok(result)
                    }
                    Value::Array(arr) => {
                        let mut guard = arr.lock().unwrap();
                        let key = Value::String(field.clone());
                        if let Some(current) = guard.get(&key) {
                            let current_clone = current.clone();
                            let result = self.eval_binop(op, &current_clone, &rhs)?;
                            guard.set(key, result.clone());
                            Ok(result)
                        } else {
                            Err(AudionError::RuntimeError {
                                msg: format!("member '{}' not found for compound assignment", field),
                            })
                        }
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("cannot compound-assign member '{}' on {}", field, obj.type_name()),
                    }),
                }
            }
            Expr::NamespaceAccess { namespace, name } => {
                let ns = self.eval_expr(namespace)?;
                match ns {
                    Value::Namespace(env) => {
                        let e = env.lock().unwrap();
                        e.get(name).ok_or_else(|| AudionError::RuntimeError {
                            msg: format!("undefined '{}' in namespace", name),
                        })
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("'{}' is not a namespace", ns.type_name()),
                    }),
                }
            }
        }
    }

    fn eval_binop(&self, op: &BinOp, left: &Value, right: &Value) -> Result<Value> {
        match (op, left, right) {
            // Number arithmetic
            (BinOp::Add, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            (BinOp::Sub, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
            (BinOp::Mul, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
            (BinOp::Div, Value::Number(a), Value::Number(b)) => {
                if *b == 0.0 {
                    Err(AudionError::RuntimeError {
                        msg: "division by zero".to_string(),
                    })
                } else {
                    Ok(Value::Number(a / b))
                }
            }
            (BinOp::Mod, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),

            // String concatenation
            (BinOp::Add, Value::String(a), Value::String(b)) => {
                Ok(Value::String(format!("{}{}", a, b)))
            }
            (BinOp::Add, Value::String(a), Value::Number(b)) => {
                Ok(Value::String(format!("{}{}", a, b)))
            }
            (BinOp::Add, Value::Number(a), Value::String(b)) => {
                Ok(Value::String(format!("{}{}", a, b)))
            }

            // Comparison
            (BinOp::Lt, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a < b)),
            (BinOp::Gt, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a > b)),
            (BinOp::LtEq, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a <= b)),
            (BinOp::GtEq, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a >= b)),

            // Equality (works for all types)
            (BinOp::Eq, _, _) => Ok(Value::Bool(left == right)),
            (BinOp::NotEq, _, _) => Ok(Value::Bool(left != right)),

            // Bitwise operations (convert f64 to i64, operate, convert back)
            (BinOp::BitAnd, Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number((((*a) as i64) & ((*b) as i64)) as f64))
            }
            (BinOp::BitOr, Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number((((*a) as i64) | ((*b) as i64)) as f64))
            }
            (BinOp::BitXor, Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number((((*a) as i64) ^ ((*b) as i64)) as f64))
            }
            (BinOp::LeftShift, Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number((((*a) as i64) << ((*b) as i64)) as f64))
            }
            (BinOp::RightShift, Value::Number(a), Value::Number(b)) => {
                Ok(Value::Number((((*a) as i64) >> ((*b) as i64)) as f64))
            }

            _ => Err(AudionError::RuntimeError {
                msg: format!(
                    "cannot apply {:?} to {} and {}",
                    op,
                    left.type_name(),
                    right.type_name()
                ),
            }),
        }
    }

    /// Bind positional + named call args to function params, filling defaults where needed.
    /// Returns a vec of (param_name, value) in declaration order.
    fn bind_args(
        &mut self,
        fn_name: &str,
        params: &[Param],
        positional: &[Value],
        named: &[(String, Value)],
    ) -> Result<Vec<(String, Value)>> {
        if positional.len() > params.len() {
            return Err(AudionError::RuntimeError {
                msg: format!(
                    "{}() expected at most {} arguments, got {}",
                    fn_name,
                    params.len(),
                    positional.len()
                ),
            });
        }
        // Check for unknown named args
        for (n, _) in named {
            if !params.iter().any(|p| &p.name == n) {
                return Err(AudionError::RuntimeError {
                    msg: format!("{}() has no parameter '{}'", fn_name, n),
                });
            }
        }
        let mut result: Vec<(String, Value)> = Vec::with_capacity(params.len());
        for (i, param) in params.iter().enumerate() {
            if i < positional.len() {
                // Check not also supplied by name
                if named.iter().any(|(n, _)| n == &param.name) {
                    return Err(AudionError::RuntimeError {
                        msg: format!(
                            "{}() argument '{}' provided both positionally and by name",
                            fn_name, param.name
                        ),
                    });
                }
                result.push((param.name.clone(), positional[i].clone()));
            } else if let Some((_, val)) = named.iter().find(|(n, _)| n == &param.name) {
                result.push((param.name.clone(), val.clone()));
            } else if let Some(default_expr) = &param.default {
                let val = self.eval_expr(&default_expr.clone())?;
                result.push((param.name.clone(), val));
            } else {
                return Err(AudionError::RuntimeError {
                    msg: format!("{}() missing required argument '{}'", fn_name, param.name),
                });
            }
        }
        Ok(result)
    }

    fn call_function(
        &mut self,
        name: &str,
        positional: &[Value],
        named: &[(String, Value)],
    ) -> Result<Value> {
        let func = self.env.lock().unwrap().get(name);
        match func {
            Some(Value::Function {
                name: fname,
                params,
                body,
                closure,
            }) => {
                // Trampoline loop for tail call optimization
                let mut cur_name = fname;
                let mut cur_params = params;
                let mut cur_body = body;
                let mut cur_closure = closure;
                let mut cur_positional = positional.to_vec();
                let mut _cur_named = named.to_vec();

                loop {
                    let bindings = self.bind_args(&cur_name, &cur_params, &cur_positional, &_cur_named)?;

                    let call_env = Arc::new(Mutex::new(Environment::new_child(cur_closure)));
                    {
                        let mut env = call_env.lock().unwrap();
                        for (name, val) in &bindings {
                            env.define(name.clone(), val.clone());
                        }
                    }

                    let old_env = std::mem::replace(&mut self.env, call_env);
                    let result = self.exec_stmt(&cur_body);
                    self.env = old_env;

                    match result? {
                        ControlFlow::Return(v) => return Ok(v),
                        ControlFlow::TailCall { callee, positional: tc_pos, named: tc_named } => {
                            let callee = if let Value::String(ref s) = callee {
                                self.env.lock().unwrap().get(s).ok_or_else(|| {
                                    AudionError::RuntimeError {
                                        msg: format!("undefined function '{}'", s),
                                    }
                                })?
                            } else {
                                callee
                            };

                            match callee {
                                Value::Function { name, params, body, closure } => {
                                    cur_name = name;
                                    cur_params = params;
                                    cur_body = body;
                                    cur_closure = closure;
                                    cur_positional = tc_pos;
                                    _cur_named = tc_named;
                                    continue;
                                }
                                Value::BuiltinFn(bname) => {
                                    if bname == "eval" {
                                        return Err(AudionError::RuntimeError {
                                            msg: "tail call to eval() is not supported".to_string(),
                                        });
                                    }
                                    return builtins::call_builtin(&bname, &tc_pos, &tc_named, &self.osc, &self.midi, &self.osc_protocol, &self.clock, &self.env, &self.shutdown, &self.base_path);
                                }
                                other => {
                                    return Err(AudionError::RuntimeError {
                                        msg: format!("'{}' is not callable", other.type_name()),
                                    });
                                }
                            }
                        }
                        _ => return Ok(Value::Nil),
                    }
                }
            }
            Some(Value::BuiltinFn(name)) => {
                // Special handling for eval() - needs interpreter context
                if name == "eval" {
                    if positional.is_empty() {
                        return Err(AudionError::RuntimeError {
                            msg: "eval() requires a code string".to_string(),
                        });
                    }
                    let code = match &positional[0] {
                        Value::String(s) => s.clone(),
                        _ => return Err(AudionError::RuntimeError {
                            msg: "eval() requires a string argument".to_string(),
                        }),
                    };

                    // Tokenize and parse the code
                    let mut lexer = crate::lexer::Lexer::new(&code);
                    let tokens = lexer.tokenize()?;
                    let mut parser = crate::parser::Parser::new(tokens);
                    let stmts = parser.parse()?;

                    // Execute statements and return last value (like run_line)
                    let mut last = Value::Nil;
                    for stmt in &stmts {
                        match self.exec_stmt(stmt)? {
                            ControlFlow::Return(v) => return Ok(v),
                            ControlFlow::TailCall { .. } => return Ok(Value::Nil),
                            ControlFlow::Break | ControlFlow::Continue => {}
                            ControlFlow::None => {}
                        }
                        if let Stmt::ExprStmt(expr) = stmt {
                            last = self.eval_expr(expr)?;
                        }
                    }
                    Ok(last)
                } else {
                    builtins::call_builtin(&name, positional, named, &self.osc, &self.midi, &self.osc_protocol, &self.clock, &self.env, &self.shutdown, &self.base_path)
                }
            }
            _ => Err(AudionError::RuntimeError {
                msg: format!("undefined function '{}'", name),
            }),
        }
    }
}

