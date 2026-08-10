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
use crate::dmx::DmxClient;
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

pub type SharedDefineCache = Arc<Mutex<crate::define_cache::DefineCache>>;

pub struct Interpreter {
    pub env: Arc<Mutex<Environment>>,
    pub osc: Arc<OscClient>,
    pub midi: Arc<MidiClient>,
    pub dmx: Arc<DmxClient>,
    pub osc_protocol: Arc<OscProtocolClient>,
    pub clock: Arc<Clock>,
    pub shutdown: Arc<AtomicBool>,
    thread_handles: HashMap<String, JoinHandle<()>>,

    pub base_path: PathBuf,
    pub current_file: String,
    included_files: Arc<Mutex<HashSet<PathBuf>>>,
    included_envs: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<Environment>>>>>,
    pub debug_sclang: bool,
    /// In-memory cache of compiled SynthDefs (watch mode only).
    /// Maps synthdef name -> (ast_hash, compiled_bytes).
    /// Persists across reloads within a single watch session.
    pub synthdef_cache: SynthDefCache,
    /// Persistent disk cache for compiled SynthDefs.
    /// Survives process restarts; consulted when the in-memory cache misses.
    pub define_cache: SharedDefineCache,
}

impl Interpreter {
    pub fn new(
        env: Arc<Mutex<Environment>>,
        osc: Arc<OscClient>,
        midi: Arc<MidiClient>,
        dmx: Arc<DmxClient>,
        osc_protocol: Arc<OscProtocolClient>,
        clock: Arc<Clock>,
        shutdown: Arc<AtomicBool>,
        debug_sclang: bool,
        synthdef_cache: SynthDefCache,
        define_cache: SharedDefineCache,
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
            dmx,
            osc_protocol,
            clock,
            shutdown,
            thread_handles: HashMap::new(),

            base_path: PathBuf::from("."),
            current_file: String::new(),
            included_files: Arc::new(Mutex::new(HashSet::new())),
            included_envs: Arc::new(Mutex::new(HashMap::new())),
            debug_sclang,
            synthdef_cache,
            define_cache,
        }
    }

    pub fn new_for_thread(
        env: Arc<Mutex<Environment>>,
        osc: Arc<OscClient>,
        midi: Arc<MidiClient>,
        dmx: Arc<DmxClient>,
        osc_protocol: Arc<OscProtocolClient>,
        clock: Arc<Clock>,
        shutdown: Arc<AtomicBool>,
        debug_sclang: bool,
        synthdef_cache: SynthDefCache,
        define_cache: SharedDefineCache,
        base_path: PathBuf,
    ) -> Self {
        Interpreter {
            env,
            osc,
            midi,
            dmx,
            osc_protocol,
            clock,
            shutdown,
            thread_handles: HashMap::new(),

            base_path,
            current_file: String::new(),
            included_files: Arc::new(Mutex::new(HashSet::new())),
            included_envs: Arc::new(Mutex::new(HashMap::new())),
            debug_sclang,
            synthdef_cache,
            define_cache,
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
        // Call shutdown() if defined — runs after all threads have finished
        let has_shutdown = {
            let e = self.env.lock().unwrap();
            matches!(e.get("shutdown"), Some(Value::Function { .. }))
        };
        if has_shutdown {
            self.call_function("shutdown", &[], &[])?;
        }
        Ok(last)
    }

    /// Run all statements and call initialise() then main() if defined, but do NOT join threads.
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
            if let Stmt::ExprStmt(..) = stmt {
                // we just need something — grab from env or recalculate
            }
        }

        // Call initialise() if defined — runs before main()
        let has_initialise = {
            let e = self.env.lock().unwrap();
            matches!(e.get("initialise"), Some(Value::Function { .. }))
        };
        if has_initialise {
            self.call_function("initialise", &[], &[])?;
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
            if let Stmt::ExprStmt(expr, line) = stmt {
                last = self.eval_expr(expr).map_err(|e| e.at_line(*line, &self.current_file))?;
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
            Stmt::ExprStmt(expr, line) => {
                self.eval_expr(expr).map_err(|e| e.at_line(*line, &self.current_file))?;
                Ok(ControlFlow::None)
            }
            Stmt::Let { name, init, line } => {
                let val = match init {
                    Some(expr) => self.eval_expr(expr).map_err(|e| e.at_line(*line, &self.current_file))?.deep_clone(),
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
            Stmt::ForIn { var, iter, body } => {
                self.exec_for_in(var, &iter, body)
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
                    body: body.clone(),
                    closure: self.env.clone(),
                    file: self.current_file.clone(),
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

                // Tier 1: in-memory cache (fastest, no I/O)
                let mem_hit = {
                    let cache = self.synthdef_cache.lock().unwrap();
                    cache.get(name).and_then(|(cached_hash, bytes)| {
                        if *cached_hash == ast_hash { Some(bytes.clone()) } else { None }
                    })
                };

                let bytes = if let Some(cached) = mem_hit {
                    eprintln!("  cached synth '{}' (memory)", name);
                    cached
                } else {
                    // Tier 2: disk .adc cache — derive the source file's absolute path
                    let source_abs: Option<PathBuf> = if !self.current_file.is_empty() {
                        let p = PathBuf::from(&self.current_file);
                        Some(if p.is_absolute() { p } else { self.base_path.join(p) })
                    } else {
                        None
                    };

                    let disk_hit = source_abs.as_deref().and_then(|src| {
                        self.define_cache.lock().unwrap().get(src, name, ast_hash)
                    });

                    if let Some(cached) = disk_hit {
                        eprintln!("  cached synth '{}' (disk)", name);
                        // Promote to in-memory cache so subsequent reloads skip disk I/O
                        self.synthdef_cache.lock().unwrap().insert(name.clone(), (ast_hash, cached.clone()));
                        cached
                    } else {
                        // Tier 3: compile via sclang
                        let out_dir = crate::sclang::synthdef_output_dir();
                        let sclang_code =
                            crate::synthdef::generate_sclang(name, params, body, &out_dir, &buffers);
                        if self.debug_sclang {
                            eprintln!("\n=== SC code for '{}' ===\n{}", name, sclang_code);
                        }
                        let compiled = crate::sclang::compile_synthdef(name, &sclang_code)?;

                        // Store in both caches
                        self.synthdef_cache.lock().unwrap().insert(name.clone(), (ast_hash, compiled.clone()));
                        if let Some(src) = source_abs.as_deref() {
                            self.define_cache.lock().unwrap().put(src, name, ast_hash, compiled.clone());
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
                    }
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
        let old_file = std::mem::replace(&mut self.current_file, path.to_string());

        // Execute all statements (but don't auto-call main)
        for stmt in &stmts {
            match self.exec_stmt(stmt)? {
                ControlFlow::Return(_) | ControlFlow::TailCall { .. } => break,
                _ => {}
            }
        }

        // Restore environment, base path, and current file
        self.env = old_env;
        self.base_path = old_base;
        self.current_file = old_file;

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

    fn exec_for_in(&mut self, var: &str, iter: &Expr, body: &Stmt) -> Result<ControlFlow> {
        let arr = match self.eval_expr(iter)? {
            Value::Array(a) => a,
            other => return Err(AudionError::RuntimeError {
                msg: format!("for-in requires an array, got {}", other.type_name()),
            }),
        };
        let items: Vec<Value> = arr.lock().unwrap().entries().iter().map(|(_, v): &(Value, Value)| v.clone()).collect();
        let parent = self.env.clone();
        let child = Arc::new(Mutex::new(Environment::new_child(parent.clone())));
        let old_env = std::mem::replace(&mut self.env, child);
        'forin: for item in items {
            self.env.lock().unwrap().define(var.to_string(), item);
            match self.exec_stmt(body) {
                Err(e) => { self.env = old_env; return Err(e); }
                Ok(ControlFlow::Break) => break 'forin,
                Ok(ControlFlow::Continue) => continue 'forin,
                Ok(ControlFlow::Return(v)) => { self.env = old_env; return Ok(ControlFlow::Return(v)); }
                Ok(tc @ ControlFlow::TailCall { .. }) => { self.env = old_env; return Ok(tc); }
                Ok(ControlFlow::None) => {}
            }
        }
        self.env = old_env;
        Ok(ControlFlow::None)
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
        let dmx = self.dmx.clone();
        let osc_protocol = self.osc_protocol.clone();
        let clock = self.clock.clone();
        let shutdown = self.shutdown.clone();
        let debug_sclang = self.debug_sclang;
        let thread_name = name.to_string();

        let synthdef_cache = self.synthdef_cache.clone();
        let define_cache = self.define_cache.clone();
        let base_path = self.base_path.clone();
        let current_file = self.current_file.clone();
        let handle = std::thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || {
                let mut interp =
                    Interpreter::new_for_thread(child_env, osc, midi, dmx, osc_protocol, clock, shutdown, debug_sclang, synthdef_cache, define_cache, base_path);
                interp.current_file = current_file;
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

    // -----------------------------------------------------------------------
    // UI method dispatch
    // -----------------------------------------------------------------------

    fn call_ui_method(&self, receiver: &Value, method: &str, args: &[Value]) -> Result<Value> {
        use crate::ui::{self, WidgetConfig, WidgetKind};

        match receiver {
            // ui.window("title", w, h)
            Value::UiContext(handle) if method == "window" => {
                let title = args.first().and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                    .unwrap_or_else(|| "Audion".to_string());
                let width = args.get(1).and_then(|v| if let Value::Number(n) = v { Some(*n as f32) } else { None })
                    .unwrap_or(800.0);
                let height = args.get(2).and_then(|v| if let Value::Number(n) = v { Some(*n as f32) } else { None })
                    .unwrap_or(600.0);
                let mut cfg = handle.config.lock().unwrap();
                cfg.title = title;
                cfg.width = width;
                cfg.height = height;
                cfg.size_dirty = true;
                Ok(Value::Nil)
            }

            // ui.background(r, g, b) / ui.background(r, g, b, a)
            Value::UiContext(handle) if method == "background" => {
                let r = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as u8;
                let g = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as u8;
                let b = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as u8;
                let a = args.get(3).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                handle.config.lock().unwrap().bg_color = Some([r, g, b, a]);
                Ok(Value::Nil)
            }

            // ui.background_image(path) / ui.background_image(path, mode) / ui.background_image(path, mode, alpha)
            Value::UiContext(handle) if method == "background_image" => {
                let path_raw = args.first()
                    .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                    .ok_or_else(|| AudionError::RuntimeError {
                        msg: "ui.background_image() requires a path string as first argument".to_string(),
                    })?;
                // Resolve relative paths against the script's directory
                let resolved = if std::path::Path::new(&path_raw).is_absolute() {
                    path_raw
                } else {
                    self.base_path.join(&path_raw).to_string_lossy().into_owned()
                };
                let mode_str = args.get(1)
                    .and_then(|v| if let Value::String(s) = v { Some(s.as_str()) } else { None })
                    .unwrap_or("fill");
                let alpha = args.get(2).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                let mut cfg = handle.config.lock().unwrap();
                cfg.bg_image       = Some(resolved);
                cfg.bg_image_mode  = crate::ui::BgImageMode::from_str(mode_str);
                cfg.bg_image_alpha = alpha;
                Ok(Value::Nil)
            }

            // ui.background_clear()
            Value::UiContext(handle) if method == "background_clear" => {
                let mut cfg = handle.config.lock().unwrap();
                cfg.bg_color = None;
                cfg.bg_image = None;
                Ok(Value::Nil)
            }

            // ui.widgets.slider("id")
            Value::UiNs(handle, ns) if ns == "widgets" => {
                let id = args.first().and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                    .ok_or_else(|| AudionError::RuntimeError {
                        msg: format!("ui.widgets.{}() requires a string id as first argument", method),
                    })?;

                // canvas is special — returns Canvas2dRef, not WidgetRef
                if method == "canvas" {
                    let width  = args.get(1).and_then(|v| v.as_number()).unwrap_or(400.0) as f32;
                    let height = args.get(2).and_then(|v| v.as_number()).unwrap_or(200.0) as f32;
                    let data_arc = crate::ui::create_canvas2d(handle, &id, width, height);
                    return Ok(Value::Canvas2dRef(data_arc));
                }

                let kind = match method {
                    "slider"       => WidgetKind::SliderH,
                    "slider_v"     => WidgetKind::SliderV,
                    "slider_range" => WidgetKind::SliderRange,
                    "button"       => WidgetKind::Button,
                    "toggle"       => WidgetKind::Toggle,
                    "knob"         => WidgetKind::Knob,
                    "number"       => WidgetKind::Number,
                    "dropdown"     => WidgetKind::Dropdown,
                    "text_label"   => WidgetKind::TextLabel,
                    "text_input"   => WidgetKind::TextInput,
                    "array"        => {
                        let n = args.get(1)
                            .and_then(|v| if let Value::Number(n) = v { Some(*n as usize) } else { None })
                            .unwrap_or(8);
                        WidgetKind::Array(n)
                    }
                    "array_numbers" => {
                        let n = args.get(1)
                            .and_then(|v| if let Value::Number(n) = v { Some(*n as usize) } else { None })
                            .unwrap_or(4);
                        WidgetKind::ArrayNumbers(n)
                    }
                    "file_picker" => {
                        let filters = args.iter().skip(1)
                            .filter_map(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .collect();
                        WidgetKind::FilePicker { filters }
                    }
                    "folder_picker" => WidgetKind::FolderPicker,
                    "piano" => WidgetKind::Piano,
                    _ => return Err(AudionError::RuntimeError {
                        msg: format!("unknown widget type: ui.widgets.{}", method),
                    }),
                };

                let mut config = WidgetConfig::new(kind);

                // For dropdown: remaining string args are the option list.
                if matches!(config.kind, WidgetKind::Dropdown) {
                    config.options = args.iter().skip(1)
                        .filter_map(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                        .collect();
                }

                // Merge .aui overrides and record the aui_path on the handle for edit-mode saves
                {
                    use crate::ui::aui_file;
                    let au_path = self.base_path.join(&self.current_file);
                    let aui_path = aui_file::aui_path_for(&au_path);
                    config = aui_file::load_widget_config(&aui_path, &id, config);
                    let mut cfg = handle.config.lock().unwrap();
                    if cfg.aui_path.is_none() {
                        cfg.aui_path = Some(aui_path);
                    }
                }

                let state = ui::get_or_create_widget(handle, &id, config);

                // piano("id", octaves, start_note) — set piano dimensions
                if method == "piano" {
                    let octaves    = args.get(1).and_then(|v| v.as_number()).unwrap_or(2.0) as u8;
                    let start_note = args.get(2).and_then(|v| v.as_number()).unwrap_or(60.0) as u8;
                    let st = state.lock().unwrap();
                    if let crate::ui::WidgetValue::Piano(piano_arc) = &st.value {
                        let mut piano = piano_arc.lock().unwrap();
                        piano.octaves    = octaves;
                        piano.start_note = start_note;
                    }
                }

                // text_label("id", "initial text") — set the display string on creation.
                if method == "text_label" {
                    if let Some(Value::String(s)) = args.get(1) {
                        state.lock().unwrap().value = crate::ui::WidgetValue::Str(s.clone());
                    }
                }

                Ok(Value::WidgetRef(state))
            }

            // widget.value()
            Value::WidgetRef(state_arc) if method == "value" => {
                use crate::ui::WidgetValue;
                let state = state_arc.lock().unwrap();
                let val = match &state.value {
                    WidgetValue::Float(f) => Value::Number(*f),
                    WidgetValue::Bool(b)  => Value::Bool(*b),
                    WidgetValue::Str(s)   => Value::String(s.clone()),
                    WidgetValue::Range(lo, hi) => {
                        let mut arr = crate::value::AudionArray::new();
                        arr.push_auto(Value::Number(*lo));
                        arr.push_auto(Value::Number(*hi));
                        Value::Array(std::sync::Arc::new(std::sync::Mutex::new(arr)))
                    }
                    WidgetValue::Array(bits) => {
                        let mut arr = crate::value::AudionArray::new();
                        for b in bits {
                            arr.push_auto(Value::Bool(*b));
                        }
                        Value::Array(std::sync::Arc::new(std::sync::Mutex::new(arr)))
                    }
                    WidgetValue::ArrayF(nums) => {
                        let mut arr = crate::value::AudionArray::new();
                        for n in nums {
                            arr.push_auto(Value::Number(*n));
                        }
                        Value::Array(std::sync::Arc::new(std::sync::Mutex::new(arr)))
                    }
                    WidgetValue::Three(_) => Value::Nil,
                    WidgetValue::Canvas2d(_) => Value::Nil,
                    WidgetValue::Piano(piano_arc) => {
                        let piano = piano_arc.lock().unwrap();
                        let mut arr = crate::value::AudionArray::new();
                        let mut notes: Vec<u8> = piano.active_notes.iter().copied().collect();
                        notes.sort_unstable();
                        for n in notes { arr.push_auto(Value::Number(n as f64)); }
                        Value::Array(std::sync::Arc::new(std::sync::Mutex::new(arr)))
                    }
                };
                Ok(val)
            }

            // widget.has_changed() — one-shot: clears dirty flag on read
            Value::WidgetRef(state_arc) if method == "has_changed" => {
                let mut state = state_arc.lock().unwrap();
                let was_dirty = state.dirty;
                state.dirty = false;
                // Button: also reset value after read so it doesn't stay "true"
                if was_dirty {
                    if matches!(state.value, crate::ui::WidgetValue::Bool(true))
                        && matches!(state.config.kind, WidgetKind::Button)
                    {
                        state.value = crate::ui::WidgetValue::Bool(false);
                    }
                }
                Ok(Value::Bool(was_dirty))
            }

            // widget.set(value) — programmatic value assignment
            Value::WidgetRef(state_arc) if method == "set" => {
                use crate::ui::WidgetValue;
                let v = args.first().ok_or_else(|| AudionError::RuntimeError {
                    msg: "widget.set() requires a value argument".to_string(),
                })?;
                let mut state = state_arc.lock().unwrap();
                match v {
                    Value::Number(n) => { state.value = WidgetValue::Float(*n); }
                    Value::Bool(b)   => { state.value = WidgetValue::Bool(*b); }
                    Value::String(s) => { state.value = WidgetValue::Str(s.clone()); }
                    _ => {}
                }
                Ok(Value::Nil)
            }

            // ui.three.canvas("id") / ui.three.canvas("id", w, h)
            Value::UiNs(handle, ns) if ns == "three" => {
                use crate::ui;
                match method {
                    "canvas" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError {
                                msg: "ui.three.canvas() requires a string id as first argument".to_string(),
                            })?;
                        let width  = args.get(1).and_then(|v| v.as_number()).unwrap_or(640.0) as f32;
                        let height = args.get(2).and_then(|v| v.as_number()).unwrap_or(480.0) as f32;
                        let scene_arc = ui::create_canvas(handle, &id, width, height);
                        Ok(Value::ThreeRef(scene_arc))
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("unknown method ui.three.{}", method),
                    }),
                }
            }

            // canvas.camera(ex,ey,ez, tx,ty,tz) / canvas.clear(r,g,b) / canvas.mesh(...) etc.
            Value::ThreeRef(scene_arc) => {
                use crate::ui::three::MeshKind;
                use glam::Vec3;
                let mut scene = scene_arc.lock().unwrap();
                match method {
                    "camera" => {
                        let ex = args.get(0).and_then(|v| v.as_number()).unwrap_or(3.0) as f32;
                        let ey = args.get(1).and_then(|v| v.as_number()).unwrap_or(3.0) as f32;
                        let ez = args.get(2).and_then(|v| v.as_number()).unwrap_or(5.0) as f32;
                        let tx = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let ty = args.get(4).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let tz = args.get(5).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        scene.camera.eye    = Vec3::new(ex, ey, ez);
                        scene.camera.target = Vec3::new(tx, ty, tz);
                        Ok(Value::Nil)
                    }
                    "fov" => {
                        scene.camera.fov_deg = args.first().and_then(|v| v.as_number()).unwrap_or(60.0) as f32;
                        Ok(Value::Nil)
                    }
                    "clear" => {
                        let r = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.05) as f32;
                        let g = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.05) as f32;
                        let b = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.1)  as f32;
                        scene.clear_color = [r, g, b];
                        Ok(Value::Nil)
                    }
                    "mesh" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError {
                                msg: "canvas.mesh() requires a string id as first argument".to_string(),
                            })?;
                        let kind_str = args.get(1)
                            .and_then(|v| if let Value::String(s) = v { Some(s.as_str().to_string()) } else { None })
                            .unwrap_or_else(|| "box".to_string());
                        let kind = match kind_str.as_str() {
                            "box" | "cube"    => MeshKind::Box,
                            "plane"           => MeshKind::Plane,
                            "sphere" | "ball" => MeshKind::Sphere,
                            "axes" | "axis"   => MeshKind::Axes,
                            other => return Err(AudionError::RuntimeError {
                                msg: format!("unknown mesh kind '{}' — use box, plane, sphere, axes", other),
                            }),
                        };
                        scene.get_or_create_mesh(&id, kind);
                        Ok(Value::Nil)
                    }
                    "color" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.color() requires mesh id".to_string() })?;
                        let r = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let g = args.get(2).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let b = args.get(3).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        if let Some(m) = scene.get_mesh_mut(&id) { m.color = [r, g, b]; }
                        Ok(Value::Nil)
                    }
                    "pos" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.pos() requires mesh id".to_string() })?;
                        let x = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let y = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let z = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        if let Some(m) = scene.get_mesh_mut(&id) { m.position = Vec3::new(x, y, z); }
                        Ok(Value::Nil)
                    }
                    "rot" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.rot() requires mesh id".to_string() })?;
                        let rx = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let ry = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let rz = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        if let Some(m) = scene.get_mesh_mut(&id) { m.rotation = Vec3::new(rx, ry, rz); }
                        Ok(Value::Nil)
                    }
                    "scale" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.scale() requires mesh id".to_string() })?;
                        let s = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        if let Some(m) = scene.get_mesh_mut(&id) { m.scale = Vec3::splat(s); }
                        Ok(Value::Nil)
                    }
                    "scale_xyz" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.scale_xyz() requires mesh id".to_string() })?;
                        let sx = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let sy = args.get(2).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let sz = args.get(3).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        if let Some(m) = scene.get_mesh_mut(&id) { m.scale = Vec3::new(sx, sy, sz); }
                        Ok(Value::Nil)
                    }
                    "show" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.show() requires mesh id".to_string() })?;
                        if let Some(m) = scene.get_mesh_mut(&id) { m.visible = true; }
                        Ok(Value::Nil)
                    }
                    "hide" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.hide() requires mesh id".to_string() })?;
                        if let Some(m) = scene.get_mesh_mut(&id) { m.visible = false; }
                        Ok(Value::Nil)
                    }
                    "remove" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.remove() requires mesh id".to_string() })?;
                        scene.meshes.retain(|m| m.id != id);
                        Ok(Value::Nil)
                    }

                    // ── Custom shaders ────────────────────────────────────
                    // scene.shader("name", fragment_wgsl)
                    // User writes only the @fragment fn fs(in: VOut) → vec4<f32> body.
                    // Standard Uniforms + vertex shader are prepended automatically.
                    "shader" => {
                        let name = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.shader() requires (name, wgsl_fragment_source)".to_string() })?;
                        let src = args.get(1)
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.shader() requires wgsl source as second argument".to_string() })?;
                        scene.shaders.insert(name, crate::ui::three::ShaderEntry::Fragment(src));
                        Ok(Value::Nil)
                    }
                    // scene.shader_full("name", complete_wgsl)
                    // User writes the complete WGSL module (must define vs + fs, same Uniforms struct).
                    "shader_full" => {
                        let name = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.shader_full() requires (name, wgsl_source)".to_string() })?;
                        let src = args.get(1)
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.shader_full() requires wgsl source as second argument".to_string() })?;
                        scene.shaders.insert(name, crate::ui::three::ShaderEntry::Full(src));
                        Ok(Value::Nil)
                    }
                    // canvas.mesh_shader("mesh_id", "shader_name")
                    "mesh_shader" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.mesh_shader() requires mesh id".to_string() })?;
                        let sh = args.get(1)
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None });
                        if let Some(m) = scene.get_mesh_mut(&id) { m.shader_id = sh; }
                        Ok(Value::Nil)
                    }

                    // ── Textures ──────────────────────────────────────────
                    // canvas.texture("tex_name", "path/to/image.png")
                    "texture" => {
                        let name = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.texture() requires (name, path)".to_string() })?;
                        let rel = args.get(1)
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.texture() requires path as second argument".to_string() })?;
                        let path = if std::path::Path::new(&rel).is_absolute() {
                            std::path::PathBuf::from(&rel)
                        } else {
                            self.base_path.join(&rel)
                        };
                        match crate::ui::three_loader::load_texture(&path) {
                            Ok((pixels, w, h)) => {
                                scene.textures.insert(name, crate::ui::three::TextureEntry { pixels, width: w, height: h });
                            }
                            Err(e) => { eprintln!("three: texture error: {e}"); }
                        }
                        Ok(Value::Nil)
                    }
                    // canvas.mesh_texture("mesh_id", "tex_name")  — or "" to clear
                    "mesh_texture" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.mesh_texture() requires mesh id".to_string() })?;
                        let tex = args.get(1)
                            .and_then(|v| if let Value::String(s) = v { if s.is_empty() { None } else { Some(s.clone()) } } else { None });
                        if let Some(m) = scene.get_mesh_mut(&id) { m.texture_id = tex; }
                        Ok(Value::Nil)
                    }
                    // canvas.uv_scale("mesh_id", su, sv)
                    "uv_scale" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.uv_scale() requires mesh id".to_string() })?;
                        let su = args.get(1).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        let sv = args.get(2).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        if let Some(m) = scene.get_mesh_mut(&id) { m.uv_scale = [su, sv]; }
                        Ok(Value::Nil)
                    }

                    // ── Model loading ─────────────────────────────────────
                    // canvas.load("mesh_id", "path/to/model.obj|.glb|.gltf")
                    "load" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.load() requires (mesh_id, path)".to_string() })?;
                        let rel = args.get(1)
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.load() requires path as second argument".to_string() })?;
                        let path = if std::path::Path::new(&rel).is_absolute() {
                            std::path::PathBuf::from(&rel)
                        } else {
                            self.base_path.join(&rel)
                        };
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        let result = match ext.as_str() {
                            "obj"  => crate::ui::three_loader::load_obj(&path),
                            "glb" | "gltf" => crate::ui::three_loader::load_gltf(&path),
                            other  => Err(format!("canvas.load(): unsupported format '.{other}' — use .obj or .glb/.gltf")),
                        };
                        match result {
                            Ok(verts) => {
                                let arc = std::sync::Arc::new(verts);
                                scene.get_or_create_mesh(&id, MeshKind::Loaded(arc));
                            }
                            Err(e) => { eprintln!("three: load error: {e}"); }
                        }
                        Ok(Value::Nil)
                    }

                    // ── Per-mesh shader uniforms ───────────────────────────
                    // canvas.set("mesh_id", slot, value)   slot 0-3 → custom0.xyzw
                    //                                      slot 4-7 → custom1.xyzw
                    "set" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.set() requires (mesh_id, slot, value)".to_string() })?;
                        let slot = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
                        let val  = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        if let Some(m) = scene.get_mesh_mut(&id) {
                            match slot {
                                0 => m.custom0[0] = val, 1 => m.custom0[1] = val,
                                2 => m.custom0[2] = val, 3 => m.custom0[3] = val,
                                4 => m.custom1[0] = val, 5 => m.custom1[1] = val,
                                6 => m.custom1[2] = val, 7 => m.custom1[3] = val,
                                _ => {}
                            }
                        }
                        Ok(Value::Nil)
                    }
                    // canvas.set4("mesh_id", slot_base, x, y, z, w)  e.g. slot_base=0 → custom0
                    "set4" => {
                        let id = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                            .ok_or_else(|| AudionError::RuntimeError { msg: "canvas.set4() requires (mesh_id, slot_base, x, y, z, w)".to_string() })?;
                        let base = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as usize;
                        let x = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let y = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let z = args.get(4).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let w = args.get(5).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        if let Some(m) = scene.get_mesh_mut(&id) {
                            match base {
                                0 => m.custom0 = [x, y, z, w],
                                4 => m.custom1 = [x, y, z, w],
                                _ => {}
                            }
                        }
                        Ok(Value::Nil)
                    }

                    // ── Mouse input ────────────────────────────────────────
                    // canvas.mouse_x() / canvas.mouse_y() → normalized [-1, 1] pointer
                    // position over this canvas (x right, y down). canvas.mouse_down()
                    // → true while the primary button is held, having been pressed on
                    // this canvas.
                    "mouse_x" => Ok(Value::Number(scene.mouse_x as f64)),
                    "mouse_y" => Ok(Value::Number(scene.mouse_y as f64)),
                    "mouse_down" => Ok(Value::Bool(scene.mouse_down)),

                    _ => Err(AudionError::RuntimeError {
                        msg: format!("unknown canvas method '{}' — available: camera, fov, clear, mesh, color, pos, rot, scale, scale_xyz, show, hide, remove, shader, shader_full, mesh_shader, texture, mesh_texture, uv_scale, load, set, set4, mouse_x, mouse_y, mouse_down", method),
                    }),
                }
            }

            // widget.min(v) / widget.max(v) / widget.label(str) / widget.style(key, ...)
            // widget.highlight(n) / widget.highlight([n, ...]) — set playback-head cells
            Value::WidgetRef(state_arc) if method == "highlight" => {
                let mut state = state_arc.lock().unwrap();
                state.highlighted.clear();
                match args.first() {
                    Some(Value::Number(n)) => state.highlighted.push(*n as usize),
                    Some(Value::Array(arr)) => {
                        let arr = arr.lock().unwrap();
                        for (_, v) in arr.entries() {
                            if let Value::Number(n) = v { state.highlighted.push(*n as usize); }
                        }
                    }
                    _ => {} // no args = clear
                }
                Ok(Value::Nil)
            }

            // piano.hold(bool) / piano.keyboard(bool)
            Value::WidgetRef(state_arc) if matches!(method, "hold" | "keyboard") => {
                let state = state_arc.lock().unwrap();
                if let crate::ui::WidgetValue::Piano(piano_arc) = &state.value {
                    let mut piano = piano_arc.lock().unwrap();
                    let on = args.first().map(|v| v.is_truthy()).unwrap_or(true);
                    match method {
                        "hold"     => piano.hold_mode = on,
                        "keyboard" => piano.keyboard_mode = on,
                        _ => {}
                    }
                }
                Ok(Value::Nil)
            }

            Value::WidgetRef(state_arc) if matches!(method, "min" | "max" | "label" | "style" | "width" | "height") => {
                let mut state = state_arc.lock().unwrap();
                match method {
                    "min" => {
                        if let Some(v) = args.first().and_then(|v| v.as_number()) {
                            state.config.min = v;
                        }
                    }
                    "max" => {
                        if let Some(v) = args.first().and_then(|v| v.as_number()) {
                            state.config.max = v;
                        }
                    }
                    "label" => {
                        if let Some(Value::String(s)) = args.first() {
                            state.config.label = Some(s.clone());
                        }
                    }
                    "width" => {
                        if let Some(w) = args.first().and_then(|v| v.as_number()) {
                            state.config.style.width = Some(w as f32);
                        }
                    }
                    "height" => {
                        if let Some(h) = args.first().and_then(|v| v.as_number()) {
                            state.config.style.height = Some(h as f32);
                        }
                    }
                    "style" => {
                        let key = args.first()
                            .and_then(|v| if let Value::String(s) = v { Some(s.as_str()) } else { None })
                            .unwrap_or("");
                        match key {
                            "color" => {
                                let r = args.get(1).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                                let g = args.get(2).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                                let b = args.get(3).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                                state.config.style.color = Some([r, g, b]);
                            }
                            "bg_color" => {
                                let r = args.get(1).and_then(|v| v.as_number()).unwrap_or(30.0) as u8;
                                let g = args.get(2).and_then(|v| v.as_number()).unwrap_or(30.0) as u8;
                                let b = args.get(3).and_then(|v| v.as_number()).unwrap_or(30.0) as u8;
                                state.config.style.bg_color = Some([r, g, b]);
                            }
                            "highlight_color" => {
                                let r = args.get(1).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                                let g = args.get(2).and_then(|v| v.as_number()).unwrap_or(230.0) as u8;
                                let b = args.get(3).and_then(|v| v.as_number()).unwrap_or(60.0) as u8;
                                state.config.style.highlight_color = Some([r, g, b]);
                            }
                            "width" => {
                                if let Some(w) = args.get(1).and_then(|v| v.as_number()) {
                                    state.config.style.width = Some(w as f32);
                                }
                            }
                            "height" => {
                                if let Some(h) = args.get(1).and_then(|v| v.as_number()) {
                                    state.config.style.height = Some(h as f32);
                                }
                            }
                            "visible" => {
                                if let Some(v) = args.get(1).and_then(|v| v.as_number()) {
                                    state.config.style.visible = Some(v != 0.0);
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                Ok(Value::Nil)
            }

            // canvas2d.clear() / .fill(r,g,b) / .rect(...) / .circle(...) / .line(...) / .text(...)
            Value::Canvas2dRef(data_arc) => {
                use crate::ui::DrawCmd;
                let mut data = data_arc.lock().unwrap();
                match method {
                    "clear" => {
                        // Publish the completed pending frame to cmds (UI reads cmds),
                        // then start a fresh pending. This eliminates flicker: the UI
                        // always reads a complete previous frame, never a half-drawn one.
                        data.cmds = std::mem::take(&mut data.pending);
                    }
                    "fill" => {
                        let r = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as u8;
                        let g = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as u8;
                        let b = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as u8;
                        data.pending.push(DrawCmd::Fill([r, g, b]));
                    }
                    "rect" => {
                        let x = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let y = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let w = args.get(2).and_then(|v| v.as_number()).unwrap_or(10.0) as f32;
                        let h = args.get(3).and_then(|v| v.as_number()).unwrap_or(10.0) as f32;
                        let r = args.get(4).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let g = args.get(5).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let b = args.get(6).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        data.pending.push(DrawCmd::Rect { x, y, w, h, color: [r, g, b], filled: true });
                    }
                    "rect_outline" => {
                        let x = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let y = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let w = args.get(2).and_then(|v| v.as_number()).unwrap_or(10.0) as f32;
                        let h = args.get(3).and_then(|v| v.as_number()).unwrap_or(10.0) as f32;
                        let r = args.get(4).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let g = args.get(5).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let b = args.get(6).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        data.pending.push(DrawCmd::Rect { x, y, w, h, color: [r, g, b], filled: false });
                    }
                    "circle" => {
                        let cx = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let cy = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let r  = args.get(2).and_then(|v| v.as_number()).unwrap_or(10.0) as f32;
                        let cr = args.get(3).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let cg = args.get(4).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let cb = args.get(5).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        data.pending.push(DrawCmd::Circle { cx, cy, r, color: [cr, cg, cb], filled: true });
                    }
                    "circle_outline" => {
                        let cx = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let cy = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let r  = args.get(2).and_then(|v| v.as_number()).unwrap_or(10.0) as f32;
                        let cr = args.get(3).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let cg = args.get(4).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let cb = args.get(5).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        data.pending.push(DrawCmd::Circle { cx, cy, r, color: [cr, cg, cb], filled: false });
                    }
                    "line" => {
                        let x1 = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let y1 = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let x2 = args.get(2).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let y2 = args.get(3).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let r  = args.get(4).and_then(|v| v.as_number()).unwrap_or(200.0) as u8;
                        let g  = args.get(5).and_then(|v| v.as_number()).unwrap_or(200.0) as u8;
                        let b  = args.get(6).and_then(|v| v.as_number()).unwrap_or(200.0) as u8;
                        let lw = args.get(7).and_then(|v| v.as_number()).unwrap_or(1.0) as f32;
                        data.pending.push(DrawCmd::Line { x1, y1, x2, y2, color: [r, g, b], width: lw });
                    }
                    "text" => {
                        let x    = args.get(0).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let y    = args.get(1).and_then(|v| v.as_number()).unwrap_or(0.0) as f32;
                        let s    = args.get(2).and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { v.as_number().map(|n| n.to_string()) }).unwrap_or_default();
                        let size = args.get(3).and_then(|v| v.as_number()).unwrap_or(14.0) as f32;
                        let r    = args.get(4).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let g    = args.get(5).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        let b    = args.get(6).and_then(|v| v.as_number()).unwrap_or(255.0) as u8;
                        data.pending.push(DrawCmd::Text { x, y, s, size, color: [r, g, b] });
                    }
                    "size" => {
                        if let Some(w) = args.get(0).and_then(|v| v.as_number()) { data.width = w as f32; }
                        if let Some(h) = args.get(1).and_then(|v| v.as_number()) { data.height = h as f32; }
                    }
                    _ => return Err(AudionError::RuntimeError {
                        msg: format!("unknown canvas method '{}' — available: clear, fill, rect, rect_outline, circle, circle_outline, line, text, size", method),
                    }),
                }
                Ok(Value::Nil)
            }

            _ => Err(AudionError::RuntimeError {
                msg: format!("unknown ui method '{}'", method),
            }),
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
                // --- UI method call interception ---
                // When callee is a MemberAccess whose receiver is a UI type,
                // we intercept before resolving to a Value so we preserve context.
                if let Expr::MemberAccess { object, field } = callee.as_ref() {
                    let receiver = self.eval_expr(object)?;
                    let mut ui_eval_args: Vec<(Value, Option<String>)> = Vec::new();
                    match &receiver {
                        Value::UiContext(_) | Value::UiNs(_, _) | Value::WidgetRef(_) | Value::ThreeRef(_) | Value::Canvas2dRef(_) => {
                            for arg in args {
                                match arg {
                                    Arg::Positional(expr) => {
                                        ui_eval_args.push((self.eval_expr(expr)?, None));
                                    }
                                    Arg::Named { name, value } => {
                                        ui_eval_args.push((self.eval_expr(value)?, Some(name.clone())));
                                    }
                                }
                            }
                            let (pos, _named) = builtins::split_args(&ui_eval_args);
                            return self.call_ui_method(&receiver, field, &pos);
                        }
                        _ => {}
                    }
                }
                // --- end UI intercept ---

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
                                if let Stmt::ExprStmt(expr, line) = stmt {
                                    last = self.eval_expr(expr).map_err(|e| e.at_line(*line, &self.current_file))?;
                                }
                            }
                            Ok(last)
                        } else {
                            builtins::call_builtin(&name, &positional, &named, &self.osc, &self.midi, &self.dmx, &self.osc_protocol, &self.clock, &self.env, &self.shutdown, &self.base_path)
                        }
                    }
                    Value::Function {
                        name,
                        params,
                        body,
                        closure,
                        file,
                    } => {
                        // Trampoline loop for tail call optimization
                        let mut cur_name = name;
                        let mut cur_params = params;
                        let mut cur_body = body;
                        let mut cur_closure = closure;
                        let mut cur_file = file;
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
                            let old_file = std::mem::replace(&mut self.current_file, cur_file.clone());
                            let result = self.exec_stmt(&cur_body);
                            self.env = old_env;
                            self.current_file = old_file;

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
                                        Value::Function { name, params, body, closure, file } => {
                                            cur_name = name;
                                            cur_params = params;
                                            cur_body = body;
                                            cur_closure = closure;
                                            cur_file = file;
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
                                            return builtins::call_builtin(&bname, &tc_pos, &tc_named, &self.osc, &self.midi, &self.dmx, &self.osc_protocol, &self.clock, &self.env, &self.shutdown, &self.base_path);
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
                body: body.clone(),
                closure: self.env.clone(),
                file: self.current_file.clone(),
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
            Expr::ArrayPushAssign { object, value } => {
                let obj = self.eval_expr(object)?;
                let val = self.eval_expr(value)?;
                match obj {
                    Value::Array(arr) => {
                        let mut guard = arr.lock().unwrap();
                        guard.push_auto(val.clone());
                        Ok(val)
                    }
                    _ => Err(AudionError::RuntimeError {
                        msg: format!("[] push syntax requires an array, got {}", obj.type_name()),
                    }),
                }
            }
            Expr::ArrayPushLhs { .. } => Err(AudionError::RuntimeError {
                msg: "[] push syntax requires assignment: array[] = value".to_string(),
            }),
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
                    // UiContext.widgets / UiContext.three → UiNs
                    Value::UiContext(handle) => match field.as_str() {
                        "widgets" | "three" => Ok(Value::UiNs(handle.clone(), field.clone())),
                        _ => Err(AudionError::RuntimeError {
                            msg: format!("no member '{}' on ui object", field),
                        }),
                    },
                    // UiNs member access used outside call context — just pass through
                    Value::UiNs(_, _) => Err(AudionError::RuntimeError {
                        msg: format!("ui.{} must be called as a function", field),
                    }),
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
            (BinOp::Pow, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.powf(*b))),

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
                file,
            }) => {
                // Trampoline loop for tail call optimization
                let mut cur_name = fname;
                let mut cur_params = params;
                let mut cur_body = body;
                let mut cur_closure = closure;
                let mut cur_file = file;
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
                    let old_file = std::mem::replace(&mut self.current_file, cur_file.clone());
                    let result = self.exec_stmt(&cur_body);
                    self.env = old_env;
                    self.current_file = old_file;

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
                                Value::Function { name, params, body, closure, file } => {
                                    cur_name = name;
                                    cur_params = params;
                                    cur_body = body;
                                    cur_closure = closure;
                                    cur_file = file;
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
                                    return builtins::call_builtin(&bname, &tc_pos, &tc_named, &self.osc, &self.midi, &self.dmx, &self.osc_protocol, &self.clock, &self.env, &self.shutdown, &self.base_path);
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
                        if let Stmt::ExprStmt(expr, line) = stmt {
                            last = self.eval_expr(expr).map_err(|e| e.at_line(*line, &self.current_file))?;
                        }
                    }
                    Ok(last)
                } else {
                    builtins::call_builtin(&name, positional, named, &self.osc, &self.midi, &self.dmx, &self.osc_protocol, &self.clock, &self.env, &self.shutdown, &self.base_path)
                }
            }
            _ => Err(AudionError::RuntimeError {
                msg: format!("undefined function '{}'", name),
            }),
        }
    }
}

