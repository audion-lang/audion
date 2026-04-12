use audion::lexer::Lexer;
use audion::token::TokenKind;

// Helper: lex source and return the first token's kind
fn first_number(src: &str) -> f64 {
    let mut lexer = Lexer::new(src);
    let tokens = lexer.tokenize().unwrap();
    match tokens[0].kind {
        TokenKind::Number(n) => n,
        ref k => panic!("expected Number, got {:?}", k),
    }
}

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

// ---------------------------------------------------------------------------
// Hex literals  0x…
// ---------------------------------------------------------------------------

#[test]
fn test_hex_lowercase() {
    assert_eq!(first_number("0xff"), 255.0);
}

#[test]
fn test_hex_uppercase() {
    assert_eq!(first_number("0xFF"), 255.0);
}

#[test]
fn test_hex_zero() {
    assert_eq!(first_number("0x00"), 0.0);
}

#[test]
fn test_hex_large() {
    assert_eq!(first_number("0x1000"), 4096.0);
}

#[test]
fn test_hex_color() {
    assert_eq!(first_number("0xFF8800"), 16746496.0); // orange
}

#[test]
fn test_hex_with_underscores() {
    assert_eq!(first_number("0xFF_00_FF"), 16711935.0); // magenta
}

#[test]
fn test_hex_prefix_uppercase_x() {
    assert_eq!(first_number("0XAB"), 171.0);
}

// ---------------------------------------------------------------------------
// Binary literals  0b…
// ---------------------------------------------------------------------------

#[test]
fn test_binary_zero() {
    assert_eq!(first_number("0b0"), 0.0);
}

#[test]
fn test_binary_one() {
    assert_eq!(first_number("0b1"), 1.0);
}

#[test]
fn test_binary_byte() {
    assert_eq!(first_number("0b11111111"), 255.0);
}

#[test]
fn test_binary_pattern() {
    assert_eq!(first_number("0b10101010"), 170.0);
}

#[test]
fn test_binary_with_underscores() {
    assert_eq!(first_number("0b1010_1010"), 170.0);
}

#[test]
fn test_binary_nibbles() {
    assert_eq!(first_number("0b0000_1111"), 15.0);
}

// ---------------------------------------------------------------------------
// Octal literals  0o…
// ---------------------------------------------------------------------------

#[test]
fn test_octal_zero() {
    assert_eq!(first_number("0o0"), 0.0);
}

#[test]
fn test_octal_seven() {
    assert_eq!(first_number("0o7"), 7.0);
}

#[test]
fn test_octal_permissions() {
    assert_eq!(first_number("0o777"), 511.0);
}

#[test]
fn test_octal_with_underscores() {
    assert_eq!(first_number("0o7_7_7"), 511.0);
}

// ---------------------------------------------------------------------------
// Decimal underscores
// ---------------------------------------------------------------------------

#[test]
fn test_decimal_thousand_separator() {
    assert_eq!(first_number("1_000"), 1000.0);
}

#[test]
fn test_decimal_million() {
    assert_eq!(first_number("1_000_000"), 1_000_000.0);
}

#[test]
fn test_decimal_underscore_float() {
    assert!((first_number("3.141_592") - 3.141592).abs() < 1e-9);
}

#[test]
fn test_decimal_leading_underscore_digit() {
    // underscore between digits, not before leading digit
    assert_eq!(first_number("44_100"), 44100.0); // common sample rate
}

// ---------------------------------------------------------------------------
// Scientific notation  1e3, 2.5e-4, 1E6
// ---------------------------------------------------------------------------

#[test]
fn test_scientific_integer_exponent() {
    assert_eq!(first_number("1e3"), 1000.0);
}

#[test]
fn test_scientific_negative_exponent() {
    assert!((first_number("1e-3") - 0.001).abs() < 1e-12);
}

#[test]
fn test_scientific_float_base() {
    assert!((first_number("2.5e2") - 250.0).abs() < 1e-9);
}

#[test]
fn test_scientific_uppercase_e() {
    assert_eq!(first_number("1E6"), 1_000_000.0);
}

#[test]
fn test_scientific_positive_exponent_sign() {
    assert_eq!(first_number("5e+2"), 500.0);
}

#[test]
fn test_scientific_audio_frequency() {
    assert_eq!(first_number("44.1e3"), 44100.0); // 44.1 kHz
}

// ---------------------------------------------------------------------------
// Existing tests still pass (regression)
// ---------------------------------------------------------------------------

#[test]
fn test_comparison_operators() {
    let mut lexer = Lexer::new("<= >= == !=");
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::LtEq));
    assert!(matches!(tokens[1].kind, TokenKind::GtEq));
    assert!(matches!(tokens[2].kind, TokenKind::EqEq));
    assert!(matches!(tokens[3].kind, TokenKind::BangEq));
}
