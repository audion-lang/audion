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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

use clap::{Parser, Subcommand};

mod ast;
mod builtins;
mod define_cache;
mod clock;
mod defcompile;
mod dmx;
mod dsp;
mod environment;
mod error;
mod hotreload;
mod interpreter;
mod lexer;
mod math;
mod ml;
mod midi;
mod strings;
mod sequences;
mod melodies;
mod osc;
mod osc_protocol;
mod parser;
mod repl;
mod sampler;
mod scheduler;
mod spec;
mod sqlite;
mod synthdef;
mod token;
mod ui;
mod value;

#[derive(clap::Parser)]
#[command(name = "audion", version, about = "Audion is a general purpose language for audiovisual art, performance and installation")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the language spec (for AI agents) to stdout
    Spec,

    /// Run an .au file
    Run {
        /// Path to .au file
        file: PathBuf,

        /// scsynth OSC address (host:port)
        #[arg(long, default_value = "127.0.0.1:57110")]
        server: String,

        /// Initial BPM, very opinionated indeed, 120 is just too meh
        #[arg(long, default_value_t = 128.0)]
        bpm: f64,

        /// Watch file for changes and reload on save
        #[arg(long)]
        watch: bool,

        /// Enable desktop UI (eframe window). Required when script uses ui_desktop().
        /// Runs interpreter on a background thread so eframe can own the main thread.
        #[arg(long)]
        ui: bool,

        /// Show generated SuperCollider code for debugging
        #[arg(long)]
        debug_sclang: bool,

        /// Extra arguments passed to the script via os_arguments()
        #[arg(trailing_var_arg = true, num_args = 0..)]
        script_args: Vec<String>,
    },
}

fn main() {
    // Scripts don't have to spawn `thread{}` blocks to do timing-critical work —
    // `main()` itself can loop on wait() directly, and that runs right here, on
    // this thread. exec_thread (interpreter.rs) elevates every *spawned* thread,
    // but this — the process's own main thread — is the one entry point that
    // isn't a spawned thread, so it needs the same treatment applied explicitly.
    if let Err(e) = thread_priority::ThreadPriority::Max.set_for_current() {
        eprintln!("warning: could not raise priority for main thread: {:?}", e);
    }

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Spec) => {
            print!("{}", spec::generate());
        }
        Some(Commands::Run { file, server, bpm, watch, ui, debug_sclang, script_args }) => {
            run_file(&file, &server, bpm, debug_sclang, watch, ui, &script_args);
        }
        None => {
            repl::run_repl("127.0.0.1:57110", 120.0);
        }
    }
}

fn read_source(path: &PathBuf) -> String {
    match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", path.display(), e);
            std::process::exit(1);
        }
    }
}

fn try_compile(source: &str, prefix: &str) -> Option<Vec<ast::Stmt>> {
    let mut lex = lexer::Lexer::new(source);
    let tokens = match lex.tokenize() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}{}", prefix, e);
            return None;
        }
    };
    let mut par = parser::Parser::new(tokens);
    match par.parse() {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("{}{}", prefix, e);
            None
        }
    }
}

fn make_interpreter(
    osc: &Arc<osc::OscClient>,
    midi: &Arc<midi::MidiClient>,
    dmx_client: &Arc<dmx::DmxClient>,
    osc_proto: &Arc<osc_protocol::OscProtocolClient>,
    clock_inst: &Arc<clock::Clock>,
    shutdown: &Arc<AtomicBool>,
    debug_sclang: bool,
    synthdef_cache: &Arc<Mutex<std::collections::HashMap<String, (u64, Vec<u8>)>>>,
    define_cache: &Arc<Mutex<define_cache::DefineCache>>,
    fn_registry: &interpreter::FnRegistry,
    loop_registry: &interpreter::LoopRegistry,
    args: &[String],
    base_path: &Option<PathBuf>,
) -> interpreter::Interpreter {
    let env = Arc::new(Mutex::new(environment::Environment::new()));
    let mut interp = interpreter::Interpreter::new(
        env, osc.clone(), midi.clone(), dmx_client.clone(), osc_proto.clone(), clock_inst.clone(), shutdown.clone(), debug_sclang, synthdef_cache.clone(), define_cache.clone(), fn_registry.clone(), loop_registry.clone(),
    );
    interp.set_args(args.to_vec());
    if let Some(ref bp) = base_path {
        interp.set_base_path(bp.clone());
    }
    interp
}

fn run_file(path: &PathBuf, server: &str, bpm: f64, debug_sclang: bool, watch: bool, enable_ui: bool, script_args: &[String]) {
    let osc = Arc::new(osc::OscClient::new(server));
    let midi = Arc::new(midi::MidiClient::new());
    let dmx_client = Arc::new(dmx::DmxClient::new());
    let osc_proto = Arc::new(osc_protocol::OscProtocolClient::new());
    let clock_inst = Arc::new(clock::Clock::new(bpm));
    let shutdown = Arc::new(AtomicBool::new(false));
    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let define_cache = Arc::new(Mutex::new(define_cache::DefineCache::new()));
    // Shared across every reload in watch mode (unlike synthdef_cache, this
    // *is* the running program's code) so a hot-patched function body stays
    // visible to already-running threads after the Interpreter that defined
    // it has been replaced. See hotreload.rs.
    let fn_registry: interpreter::FnRegistry = Arc::new(Mutex::new(std::collections::HashMap::new()));
    // Same lifetime/sharing story as fn_registry, for named threads' loop
    // bodies instead of named functions' bodies. See hotreload.rs.
    let loop_registry: interpreter::LoopRegistry = Arc::new(Mutex::new(std::collections::HashMap::new()));

    // Lookahead scheduler: sequencer threads run `latency()` seconds ahead and
    // this dispatches their events (timestamped OSC bundles to scsynth, exact
    // wall-clock MIDI/OSC) so timing survives interpreter jitter.
    let scheduler =
        scheduler::Scheduler::new_and_start(server, clock_inst.clone(), midi.clone(), osc_proto.clone());
    osc.set_scheduler(scheduler.clone());
    midi.set_scheduler(scheduler.clone());
    osc_proto.set_scheduler(scheduler.clone());

    // Set up Ctrl+C handler
    let osc_cleanup = osc.clone();
    let midi_cleanup = midi.clone();
    let sched_cleanup = scheduler.clone();
    let shutdown_flag = shutdown.clone();
    ctrlc::set_handler(move || {
        eprintln!("\nshutting down...");
        shutdown_flag.store(true, Ordering::Relaxed);
        sched_cleanup.clear();
        midi_cleanup.panic();
        osc_cleanup.free_all_nodes();
        osc_cleanup.free_all_buffers();
        std::thread::sleep(Duration::from_millis(100));
        std::process::exit(0);
    })
    .expect("failed to set Ctrl+C handler");

    let base_path = path.canonicalize().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    // args[0] = script file path, args[1..] = user-supplied extras
    let mut args: Vec<String> = vec![path.to_string_lossy().to_string()];
    args.extend_from_slice(script_args);

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

    if !watch {
        let source = read_source(path);
        let stmts = try_compile(&source, "").unwrap_or_else(|| std::process::exit(1));

        if enable_ui {
            // UI mode: interpreter runs on a background thread; eframe on main thread.
            let interp_done = Arc::new(AtomicBool::new(false));
            let interp_done_bg = interp_done.clone();
            let osc_bg = osc.clone();
            let mut interp = make_interpreter(&osc, &midi, &dmx_client, &osc_proto, &clock_inst, &shutdown, debug_sclang, &synthdef_cache, &define_cache, &fn_registry, &loop_registry, &args, &base_path);
            interp.current_file = file_name.clone();
            let _interp_thread = thread::spawn(move || {
                // This thread (not the process main thread, which eframe owns in
                // --ui mode) is the one actually running the script's sequencer
                // logic, so it needs the same priority bump `fn main()` gets above.
                if let Err(e) = thread_priority::ThreadPriority::Max.set_for_current() {
                    eprintln!("warning: could not raise priority for UI interpreter thread: {:?}", e);
                }
                if let Err(e) = interp.run(&stmts) {
                    eprintln!("{}", e);
                    osc_bg.free_all_nodes();
                    osc_bg.free_all_buffers();
                }
                interp_done_bg.store(true, Ordering::Relaxed);
            });
            let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/logo.png")).ok();
            let options = eframe::NativeOptions {
                renderer: eframe::Renderer::Wgpu,
                viewport: eframe::egui::ViewportBuilder::default().with_icon(
                    icon.unwrap_or_default()
                ),
                ..Default::default()
            };
            eframe::run_native("Audion", options, Box::new(|cc| {
                Ok(Box::new(ui::runner::AudionUiApp::new(cc, interp_done)))
            }))
                .unwrap_or_else(|e| eprintln!("UI error: {}", e));
            return;
        }

        let mut interp = make_interpreter(&osc, &midi, &dmx_client, &osc_proto, &clock_inst, &shutdown, debug_sclang, &synthdef_cache, &define_cache, &fn_registry, &loop_registry, &args, &base_path);
        interp.current_file = file_name.clone();
        if let Err(e) = interp.run(&stmts) {
            eprintln!("{}", e);
            osc.free_all_nodes();
            osc.free_all_buffers();
            std::process::exit(1);
        }
        // Let any scheduled MIDI/OSC tail drain before the process exits.
        scheduler.wait_until_empty(Duration::from_secs(5));
        if builtins::print_assert_stats() {
            std::process::exit(1);
        }
        return;
    }

    // -- Watch mode --
    let mut interp: Option<interpreter::Interpreter> = None;
    let source = read_source(path);
    // Tracks mtimes for the entry script *and* every file it `include`s, so
    // editing an include triggers a reload too, not just the entry script.
    // Repopulated after each full reload since includes can change.
    let mut watched_mtimes: std::collections::HashMap<PathBuf, Option<std::time::SystemTime>> =
        std::collections::HashMap::new();
    watched_mtimes.insert(path.clone(), std::fs::metadata(path).and_then(|m| m.modified()).ok());

    // Last-known-good parse of the entry script and of every included file,
    // scoped by canonical path. Used only to attempt the no-teardown
    // hot-patch path (see hotreload.rs) — a full reload always re-derives
    // these from scratch afterward, so a stale/missing entry here just means
    // falling back to a full reload, never incorrect behavior.
    let mut entry_stmts: Vec<ast::Stmt> = Vec::new();
    let mut include_stmts: std::collections::HashMap<PathBuf, Vec<ast::Stmt>> = std::collections::HashMap::new();
    let entry_base_path_str = base_path.clone().unwrap_or_else(|| PathBuf::from(".")).display().to_string();

    fn snapshot_includes(paths: &[PathBuf]) -> std::collections::HashMap<PathBuf, Vec<ast::Stmt>> {
        let mut map = std::collections::HashMap::new();
        for p in paths {
            if let Ok(src) = std::fs::read_to_string(p) {
                if let Some(stmts) = try_compile(&src, "watch: ") {
                    map.insert(p.clone(), stmts);
                }
            }
        }
        map
    }

    if let Some(stmts) = try_compile(&source, "watch: ") {
        let mut new_interp = make_interpreter(&osc, &midi, &dmx_client, &osc_proto, &clock_inst, &shutdown, debug_sclang, &synthdef_cache, &define_cache, &fn_registry, &loop_registry, &args, &base_path);
        new_interp.current_file = file_name.clone();
        match new_interp.run_without_join(&stmts) {
            Ok(_) => {
                let included = new_interp.included_file_paths();
                for p in &included {
                    let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok();
                    watched_mtimes.insert(p.clone(), mtime);
                }
                entry_stmts = stmts;
                include_stmts = snapshot_includes(&included);
                interp = Some(new_interp);
            }
            Err(e) => { eprintln!("{}", e); }
        }
    }
    eprintln!("watching {} for changes...", path.display());

    loop {
        std::thread::sleep(Duration::from_millis(500));

        let changed_paths: Vec<PathBuf> = watched_mtimes.iter().filter_map(|(p, last)| {
            let current = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            if current != *last { Some(p.clone()) } else { None }
        }).collect();
        if changed_paths.is_empty() {
            continue;
        }

        // Try the no-teardown path first: every changed file must itself
        // re-parse cleanly *and* diff as "only things this module knows how
        // to patch/restart changed" against what's currently running. Any
        // failure anywhere falls back to the full reload below — never
        // applies a partial plan.
        let mut fast_plan = hotreload::ReloadPlan::default();
        let mut fast_path_ok = interp.is_some() && !entry_stmts.is_empty();
        let mut new_entry_stmts: Option<Vec<ast::Stmt>> = None;
        let mut new_include_stmts: std::collections::HashMap<PathBuf, Vec<ast::Stmt>> = std::collections::HashMap::new();

        if fast_path_ok {
            for p in &changed_paths {
                let src = match std::fs::read_to_string(p) {
                    Ok(s) => s,
                    Err(_) => { fast_path_ok = false; break; }
                };
                let stmts = match try_compile(&src, "watch: ") {
                    Some(s) => s,
                    None => { fast_path_ok = false; break; }
                };
                let (old_stmts, base_str, file_str): (&[ast::Stmt], String, String) = if p == path {
                    (&entry_stmts, entry_base_path_str.clone(), file_name.clone())
                } else {
                    match include_stmts.get(p) {
                        Some(old) => (
                            old,
                            p.parent().unwrap_or(std::path::Path::new(".")).display().to_string(),
                            p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                        ),
                        // Never seen this included file's previous content
                        // (e.g. it was added since the last full reload) —
                        // can't diff it, so don't guess.
                        None => { fast_path_ok = false; break; }
                    }
                };
                match hotreload::diff_for_patch(old_stmts, &stmts, &base_str, &file_str) {
                    Some(mut plan) => {
                        fast_plan.fn_patches.append(&mut plan.fn_patches);
                        fast_plan.thread_actions.append(&mut plan.thread_actions);
                    }
                    None => { fast_path_ok = false; break; }
                }
                if p == path {
                    new_entry_stmts = Some(stmts);
                } else {
                    new_include_stmts.insert(p.clone(), stmts);
                }
            }
        }

        if fast_path_ok {
            let n_fns = fast_plan.fn_patches.len();
            {
                let mut registry = fn_registry.lock().unwrap();
                for (qualified_path, body) in fast_plan.fn_patches {
                    registry.insert(qualified_path, body);
                }
            }
            let mut n_loop_swaps = 0;
            let mut n_restarts = 0;
            if let Some(ref mut live) = interp {
                for (thread_name, action) in fast_plan.thread_actions {
                    match action {
                        hotreload::ThreadAction::LoopSwap { new_loop_body } => {
                            live.swap_loop_body(&thread_name, new_loop_body);
                            n_loop_swaps += 1;
                        }
                        hotreload::ThreadAction::Restart { new_body } => {
                            if live.restart_thread(&thread_name, new_body) {
                                n_restarts += 1;
                            } else {
                                eprintln!(
                                    "watch: couldn't restart thread '{}' (not spawned by the top-level script) — leaving it running on its old code",
                                    thread_name
                                );
                            }
                        }
                    }
                }
            }
            if let Some(s) = new_entry_stmts {
                entry_stmts = s;
            }
            for (p, s) in new_include_stmts {
                include_stmts.insert(p.clone(), s);
                let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
                watched_mtimes.insert(p, mtime);
            }
            if changed_paths.iter().any(|p| p == path) {
                watched_mtimes.insert(path.clone(), std::fs::metadata(path).and_then(|m| m.modified()).ok());
            }
            eprintln!(
                "hot-patched {} function(s), {} thread loop(s) swapped in place, {} thread(s) restarted",
                n_fns, n_loop_swaps, n_restarts
            );
            continue;
        }

        // Fall back: full teardown and rebuild, same as always.
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("watch: cannot read '{}': {}", path.display(), e);
                continue;
            }
        };

        let stmts = match try_compile(&source, "watch: ") {
            Some(s) => s,
            None => continue,
        };

        if let Some(ref mut old_interp) = interp {
            shutdown.store(true, Ordering::Relaxed);
            old_interp.join_threads();
            scheduler.clear();
            osc.free_all_nodes();
            osc.free_all_buffers();
            shutdown.store(false, Ordering::Relaxed);
        }

        builtins::reset_assert_stats();
        let mut new_interp = make_interpreter(&osc, &midi, &dmx_client, &osc_proto, &clock_inst, &shutdown, debug_sclang, &synthdef_cache, &define_cache, &fn_registry, &loop_registry, &args, &base_path);
        new_interp.current_file = file_name.clone();
        match new_interp.run_without_join(&stmts) {
            Ok(_) => {
                eprintln!("reloaded {}", path.display());
                watched_mtimes.clear();
                watched_mtimes.insert(path.clone(), std::fs::metadata(path).and_then(|m| m.modified()).ok());
                let included = new_interp.included_file_paths();
                for p in &included {
                    let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok();
                    watched_mtimes.insert(p.clone(), mtime);
                }
                entry_stmts = stmts;
                include_stmts = snapshot_includes(&included);
                interp = Some(new_interp);
            }
            Err(e) => {
                eprintln!("watch: runtime error: {}", e);
                interp = None;
                entry_stmts = Vec::new();
                include_stmts.clear();
            }
        }
    }
}
