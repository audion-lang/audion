use audion::lexer::Lexer;
use audion::token::TokenKind;

#[test]
fn test_basic_tokens() {
    let mut lexer = Lexer::new("let x = 42;");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::Let));
    assert!(matches!(&tokens[1].kind, TokenKind::Ident(s) if s == "x"));
    assert!(matches!(tokens[2].kind, TokenKind::Eq));
    assert!(matches!(tokens[3].kind, TokenKind::Number(n) if n == 42.0));
    assert!(matches!(tokens[4].kind, TokenKind::Semicolon));
    assert!(matches!(tokens[5].kind, TokenKind::Eof));
}

#[test]
fn test_string_literal() {
    let mut lexer = Lexer::new("\"hello world\"");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(&tokens[0].kind, TokenKind::StringLit(s) if s == "hello world"));
}

#[test]
fn test_function_def() {
    let mut lexer = Lexer::new("fn kick() { synth(\"default\", freq: 60); }");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::Fn));
    assert!(matches!(&tokens[1].kind, TokenKind::Ident(s) if s == "kick"));
    assert!(matches!(tokens[2].kind, TokenKind::LParen));
    assert!(matches!(tokens[3].kind, TokenKind::RParen));
    assert!(matches!(tokens[4].kind, TokenKind::LBrace));
}

#[test]
fn test_thread_keyword() {
    let mut lexer = Lexer::new("thread my_loop { }");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::Thread));
    assert!(matches!(&tokens[1].kind, TokenKind::Ident(s) if s == "my_loop"));
    assert!(matches!(tokens[2].kind, TokenKind::LBrace));
    assert!(matches!(tokens[3].kind, TokenKind::RBrace));
}

#[test]
fn test_comments() {
    let mut lexer = Lexer::new("x = 1; // comment\n/* block */ y = 2;");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(&tokens[0].kind, TokenKind::Ident(s) if s == "x"));
    assert!(matches!(tokens[1].kind, TokenKind::Eq));
    assert!(matches!(tokens[2].kind, TokenKind::Number(n) if n == 1.0));
    assert!(matches!(tokens[3].kind, TokenKind::Semicolon));
    assert!(matches!(&tokens[4].kind, TokenKind::Ident(s) if s == "y"));
}

#[test]
fn test_float_number() {
    let mut lexer = Lexer::new("3.14");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::Number(n) if (n - 3.14).abs() < 0.001));
}

#[test]
fn test_comparison_operators() {
    let mut lexer = Lexer::new("<= >= == !=");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::LtEq));
    assert!(matches!(tokens[1].kind, TokenKind::GtEq));
    assert!(matches!(tokens[2].kind, TokenKind::EqEq));
    assert!(matches!(tokens[3].kind, TokenKind::BangEq));
}
