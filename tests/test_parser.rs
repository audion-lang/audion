mod common;

use audion::ast::*;

#[test]
fn test_let_stmt() {
    let stmts = common::parse_source("let x = 42;");
    assert!(matches!(&stmts[0], Stmt::Let { name, init: Some(Expr::Number(n)) } if name == "x" && *n == 42.0));
}

#[test]
fn test_fn_decl() {
    let stmts = common::parse_source("fn kick(freq, amp) { return freq + amp; }");
    assert!(matches!(&stmts[0], Stmt::FnDecl { name, params, .. } if name == "kick" && params.len() == 2));
}

#[test]
fn test_thread_block() {
    let stmts = common::parse_source("thread my_loop { let x = 1; }");
    assert!(matches!(&stmts[0], Stmt::Thread { name, .. } if name == "my_loop"));
}

#[test]
fn test_named_args() {
    let stmts = common::parse_source("synth(\"default\", freq: 440, amp: 0.5);");
    if let Stmt::ExprStmt(Expr::Call { args, .. }) = &stmts[0] {
        assert_eq!(args.len(), 3);
        assert!(matches!(&args[0], Arg::Positional(Expr::StringLit(_))));
        assert!(matches!(&args[1], Arg::Named { name, .. } if name == "freq"));
        assert!(matches!(&args[2], Arg::Named { name, .. } if name == "amp"));
    } else {
        panic!("expected call expression");
    }
}

#[test]
fn test_loop_stmt() {
    let stmts = common::parse_source("loop { break; }");
    assert!(matches!(&stmts[0], Stmt::Loop { .. }));
}

#[test]
fn test_for_stmt() {
    let stmts = common::parse_source("for (let i = 0; i < 10; i += 1) { print(i); }");
    assert!(matches!(&stmts[0], Stmt::For { .. }));
}

#[test]
fn test_if_else() {
    let stmts = common::parse_source("if (x > 0) { print(x); } else { print(0); }");
    assert!(matches!(&stmts[0], Stmt::If { else_: Some(_), .. }));
}

#[test]
fn test_fn_expr() {
    let stmts = common::parse_source("let f = fn(x) { return x + 1; };");
    if let Stmt::Let { init: Some(Expr::FnExpr { params, .. }), .. } = &stmts[0] {
        assert_eq!(params.len(), 1);
    } else {
        panic!("expected fn expr");
    }
}

#[test]
fn test_synthdef() {
    let stmts = common::parse_source("define mysaw(freq, amp, gate) { out(0, saw(freq) * env(gate) * amp); }");
    if let Stmt::SynthDef { name, params, body } = &stmts[0] {
        assert_eq!(name, "mysaw");
        assert_eq!(params, &["freq", "amp", "gate"]);
        assert!(matches!(body, UGenExpr::UGenCall { name, .. } if name == "out"));
    } else {
        panic!("expected synthdef");
    }
}

#[test]
fn test_synthdef_nested() {
    let stmts = common::parse_source("define bass(freq, gate) { out(0, lpf(saw(freq), 2000) * env(gate)); }");
    assert!(matches!(&stmts[0], Stmt::SynthDef { name, .. } if name == "bass"));
}

#[test]
fn test_synthdef_with_sample() {
    let stmts = common::parse_source(
        r#"define drum(freq, amp, gate) { out(0, sample("kick.wav", root: 60, vel_lo: 0, vel_hi: 80) * env(gate) * amp); }"#,
    );
    if let Stmt::SynthDef { name, params, body } = &stmts[0] {
        assert_eq!(name, "drum");
        assert_eq!(params, &["freq", "amp", "gate"]);
        if let UGenExpr::UGenCall { name, .. } = body {
            assert_eq!(name, "out");
        } else {
            panic!("expected UGenCall 'out'");
        }
    } else {
        panic!("expected synthdef");
    }
}

#[test]
fn test_synthdef_multi_sample_layers() {
    let stmts = common::parse_source(
        r#"define piano(freq, amp, gate) { out(0, sample("soft.wav", root: 60) * amp + sample("hard.wav", root: 60) * amp); }"#,
    );
    assert!(matches!(&stmts[0], Stmt::SynthDef { name, .. } if name == "piano"));
}

#[test]
fn test_operator_precedence() {
    let stmts = common::parse_source("let x = 1 + 2 * 3;");
    // Should parse as 1 + (2 * 3)
    if let Stmt::Let { init: Some(Expr::BinOp { op: BinOp::Add, right, .. }), .. } = &stmts[0] {
        assert!(matches!(right.as_ref(), Expr::BinOp { op: BinOp::Mul, .. }));
    } else {
        panic!("expected binop");
    }
}

#[test]
fn test_synthdef_with_let() {
    let stmts = common::parse_source(
        "define verb(freq, amp, gate) { let sig = saw(freq) * env(gate) * amp; out(0, reverb(sig, 0.5, 0.8, 0.5)); }",
    );
    if let Stmt::SynthDef { name, params, body } = &stmts[0] {
        assert_eq!(name, "verb");
        assert_eq!(params, &["freq", "amp", "gate"]);
        if let UGenExpr::Block { lets, results } = body {
            assert_eq!(lets.len(), 1);
            assert_eq!(lets[0].0, "sig");
            assert_eq!(results.len(), 1);
            if let UGenExpr::UGenCall { name, .. } = results[0].as_ref() {
                assert_eq!(name, "out");
            } else {
                panic!("expected UGenCall 'out' as result");
            }
        } else {
            panic!("expected Block with let bindings");
        }
    } else {
        panic!("expected synthdef");
    }
}

#[test]
fn test_synthdef_multiple_lets() {
    let stmts = common::parse_source(
        "define fx(freq, gate) { let dry = saw(freq); let wet = delay(dry, 0.2, 2); out(0, dry + wet); }",
    );
    if let Stmt::SynthDef { body, .. } = &stmts[0] {
        if let UGenExpr::Block { lets, .. } = body {
            assert_eq!(lets.len(), 2);
            assert_eq!(lets[0].0, "dry");
            assert_eq!(lets[1].0, "wet");
        } else {
            panic!("expected Block with 2 let bindings");
        }
    } else {
        panic!("expected synthdef");
    }
}

#[test]
fn test_synthdef_no_let_stays_flat() {
    // Without let, body should NOT be wrapped in Block
    let stmts = common::parse_source("define s(freq) { out(0, sine(freq)); }");
    if let Stmt::SynthDef { body, .. } = &stmts[0] {
        assert!(matches!(body, UGenExpr::UGenCall { .. }), "no lets = no Block wrapper");
    } else {
        panic!("expected synthdef");
    }
}
