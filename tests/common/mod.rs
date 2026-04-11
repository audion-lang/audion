use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use audion::clock::Clock;
use audion::environment::Environment;
use audion::interpreter::Interpreter;
use audion::lexer::Lexer;
use audion::midi::MidiClient;
use audion::osc::OscClient;
use audion::osc_protocol::OscProtocolClient;
use audion::parser::Parser;
use audion::value::Value;

pub fn eval(src: &str) -> Value {
    let mut lex = Lexer::new(src);
    let tokens = lex.tokenize().unwrap();
    let mut par = Parser::new(tokens);
    let stmts = par.parse().unwrap();

    let env = Arc::new(Mutex::new(Environment::new()));
    let osc = Arc::new(OscClient::new("127.0.0.1:57110"));
    let midi = Arc::new(MidiClient::new());
    let osc_protocol = Arc::new(OscProtocolClient::new());
    let clock = Arc::new(Clock::new(120.0));
    let shutdown = Arc::new(AtomicBool::new(false));
    let synthdef_cache = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut interp = Interpreter::new(env, osc, midi, osc_protocol, clock, shutdown, false, synthdef_cache);
    interp.run_line(&stmts).unwrap()
}

pub fn parse_source(src: &str) -> Vec<audion::ast::Stmt> {
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    parser.parse().unwrap()
}
