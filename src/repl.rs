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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::clock::Clock;
use crate::dmx::DmxClient;
use crate::environment::Environment;
use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::midi::MidiClient;
use crate::osc::OscClient;
use crate::osc_protocol::OscProtocolClient;
use crate::parser::Parser;

pub fn run_repl(server: &str, bpm: f64) {
    let year = Utc::now().format("%Y");
    eprintln!("Audion {}  Copyright (C) 2025-{}  Aleksandr Bogdanov", env!("CARGO_PKG_VERSION"), year);
    eprintln!("This program comes with ABSOLUTELY NO WARRANTY;");
    eprintln!("This is free software, and you are welcome to redistribute it");
    eprintln!("under certain conditions; see https://www.gnu.org/licenses/gpl-3.0.txt");
    eprintln!();

    let mut rl = DefaultEditor::new().expect("failed to initialize readline");

    let env = Arc::new(Mutex::new(Environment::new()));
    let osc = Arc::new(OscClient::new(server));
    let midi = Arc::new(MidiClient::new());
    let dmx = Arc::new(DmxClient::new());
    let osc_proto = Arc::new(OscProtocolClient::new());
    let clock = Arc::new(Clock::new(bpm));
    let shutdown = Arc::new(AtomicBool::new(false));

    let osc_cleanup = osc.clone();
    let midi_cleanup = midi.clone();
    let shutdown_flag = shutdown.clone();
    ctrlc::set_handler(move || {
        shutdown_flag.store(true, Ordering::Relaxed);
        midi_cleanup.panic();
        osc_cleanup.free_all_nodes();
        std::process::exit(0);
    })
    .expect("failed to set Ctrl+C handler");

    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut interpreter = Interpreter::new(env, osc, midi, dmx, osc_proto, clock, shutdown, false, synthdef_cache);

    // Store command-line arguments
    let args: Vec<String> = std::env::args().collect();
    interpreter.set_args(args);

    println!("audion v{} — type 'exit' or Ctrl+D to quit", env!("CARGO_PKG_VERSION"));
    println!("connected to scsynth at {}", server);

    loop {
        match rl.readline(">> ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    break;
                }

                let _ = rl.add_history_entry(&line);

                // Multi-line: detect unbalanced braces
                let source = read_complete_input(&mut rl, &line);

                match run_source(&mut interpreter, &source) {
                    Ok(val) => {
                        if val != crate::value::Value::Nil {
                            println!("{}", val);
                        }
                    }
                    Err(e) => eprintln!("{}", e),
                }
            }
            Err(ReadlineError::Eof) => break,
            Err(ReadlineError::Interrupted) => {
                println!("(use 'exit' to quit)");
                continue;
            }
            Err(e) => {
                eprintln!("readline error: {}", e);
                break;
            }
        }
    }

    interpreter.join_threads();
}

fn read_complete_input(rl: &mut DefaultEditor, first_line: &str) -> String {
    let mut source = first_line.to_string();
    let mut depth: i32 = 0;

    for ch in source.chars() {
        if ch == '{' {
            depth += 1;
        }
        if ch == '}' {
            depth -= 1;
        }
    }

    while depth > 0 {
        match rl.readline(".. ") {
            Ok(line) => {
                source.push('\n');
                source.push_str(&line);
                for ch in line.chars() {
                    if ch == '{' {
                        depth += 1;
                    }
                    if ch == '}' {
                        depth -= 1;
                    }
                }
            }
            Err(_) => break,
        }
    }

    source
}

fn run_source(
    interpreter: &mut Interpreter,
    source: &str,
) -> crate::error::Result<crate::value::Value> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse()?;
    interpreter.run_line(&stmts)
}
