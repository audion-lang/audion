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

use clap::{Parser, Subcommand};

mod ast;
mod builtins;
mod clock;
mod dmx;
mod environment;
mod error;
mod interpreter;
mod lexer;
mod math;
mod midi;
mod strings;
mod sequences;
mod melodies;
mod osc;
mod osc_protocol;
mod parser;
mod repl;
mod sampler;
mod sclang;
mod spec;
mod sqlite;
mod synthdef;
mod token;
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

        /// Show generated SuperCollider code for debugging
        #[arg(long)]
        debug_sclang: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Spec) => {
            print!("{}", spec::generate());
        }
        Some(Commands::Run { file, server, bpm, watch, debug_sclang }) => {
            run_file(&file, &server, bpm, debug_sclang, watch);
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
    args: &[String],
    base_path: &Option<PathBuf>,
) -> interpreter::Interpreter {
    let env = Arc::new(Mutex::new(environment::Environment::new()));
    let mut interp = interpreter::Interpreter::new(
        env, osc.clone(), midi.clone(), dmx_client.clone(), osc_proto.clone(), clock_inst.clone(), shutdown.clone(), debug_sclang, synthdef_cache.clone(),
    );
    interp.set_args(args.to_vec());
    if let Some(ref bp) = base_path {
        interp.set_base_path(bp.clone());
    }
    interp
}

fn run_file(path: &PathBuf, server: &str, bpm: f64, debug_sclang: bool, watch: bool) {
    let osc = Arc::new(osc::OscClient::new(server));
    let midi = Arc::new(midi::MidiClient::new());
    let dmx_client = Arc::new(dmx::DmxClient::new());
    let osc_proto = Arc::new(osc_protocol::OscProtocolClient::new());
    let clock_inst = Arc::new(clock::Clock::new(bpm));
    let shutdown = Arc::new(AtomicBool::new(false));
    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));

    // Set up Ctrl+C handler
    let osc_cleanup = osc.clone();
    let midi_cleanup = midi.clone();
    let shutdown_flag = shutdown.clone();
    ctrlc::set_handler(move || {
        eprintln!("\nshutting down...");
        shutdown_flag.store(true, Ordering::Relaxed);
        midi_cleanup.panic();
        osc_cleanup.free_all_nodes();
        osc_cleanup.free_all_buffers();
        std::thread::sleep(Duration::from_millis(100));
        std::process::exit(0);
    })
    .expect("failed to set Ctrl+C handler");

    let base_path = path.canonicalize().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let args: Vec<String> = std::env::args().collect();

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

    if !watch {
        let source = read_source(path);
        let stmts = try_compile(&source, "").unwrap_or_else(|| std::process::exit(1));
        let mut interp = make_interpreter(&osc, &midi, &dmx_client, &osc_proto, &clock_inst, &shutdown, debug_sclang, &synthdef_cache, &args, &base_path);
        interp.current_file = file_name.clone();
        if let Err(e) = interp.run(&stmts) {
            eprintln!("{}", e);
            osc.free_all_nodes();
            osc.free_all_buffers();
            std::process::exit(1);
        }
        if builtins::print_assert_stats() {
            std::process::exit(1);
        }
        return;
    }

    // -- Watch mode --
    let mut interp: Option<interpreter::Interpreter> = None;
    let source = read_source(path);
    let mut last_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();

    if let Some(stmts) = try_compile(&source, "watch: ") {
        let mut new_interp = make_interpreter(&osc, &midi, &dmx_client, &osc_proto, &clock_inst, &shutdown, debug_sclang, &synthdef_cache, &args, &base_path);
        new_interp.current_file = file_name.clone();
        match new_interp.run_without_join(&stmts) {
            Ok(_) => { interp = Some(new_interp); }
            Err(e) => { eprintln!("{}", e); }
        }
    }
    eprintln!("watching {} for changes...", path.display());

    loop {
        std::thread::sleep(Duration::from_millis(500));

        let new_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if new_mtime == last_mtime {
            continue;
        }
        last_mtime = new_mtime;

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
            osc.free_all_nodes();
            osc.free_all_buffers();
            shutdown.store(false, Ordering::Relaxed);
        }

        builtins::reset_assert_stats();
        let mut new_interp = make_interpreter(&osc, &midi, &dmx_client, &osc_proto, &clock_inst, &shutdown, debug_sclang, &synthdef_cache, &args, &base_path);
        new_interp.current_file = file_name.clone();
        match new_interp.run_without_join(&stmts) {
            Ok(_) => {
                eprintln!("reloaded {}", path.display());
                interp = Some(new_interp);
            }
            Err(e) => {
                eprintln!("watch: runtime error: {}", e);
                interp = None;
            }
        }
    }
}
