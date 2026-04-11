use audion::math::*;
use audion::value::Value;

fn num(n: f64) -> Value {
    Value::Number(n)
}

#[test]
fn test_math_abs() {
    assert_eq!(builtin_math_abs(&[num(-5.0)]).unwrap(), num(5.0));
    assert_eq!(builtin_math_abs(&[num(3.0)]).unwrap(), num(3.0));
    assert_eq!(builtin_math_abs(&[num(0.0)]).unwrap(), num(0.0));
}

#[test]
fn test_math_ceil_floor_round() {
    assert_eq!(builtin_math_ceil(&[num(4.3)]).unwrap(), num(5.0));
    assert_eq!(builtin_math_floor(&[num(4.7)]).unwrap(), num(4.0));
    assert_eq!(builtin_math_round(&[num(4.5)]).unwrap(), num(5.0));
    assert_eq!(builtin_math_round(&[num(4.4)]).unwrap(), num(4.0));
}

#[test]
fn test_math_round_precision() {
    assert_eq!(
        builtin_math_round(&[num(3.14159), num(2.0)]).unwrap(),
        num(3.14)
    );
    assert_eq!(
        builtin_math_round(&[num(3.14159), num(4.0)]).unwrap(),
        num(3.1416)
    );
}

#[test]
fn test_math_min_max() {
    assert_eq!(
        builtin_math_min(&[num(3.0), num(1.0), num(2.0)]).unwrap(),
        num(1.0)
    );
    assert_eq!(
        builtin_math_max(&[num(3.0), num(1.0), num(2.0)]).unwrap(),
        num(3.0)
    );
}

#[test]
fn test_math_pi_e() {
    assert_eq!(builtin_math_pi(&[]).unwrap(), num(std::f64::consts::PI));
    assert_eq!(builtin_math_e(&[]).unwrap(), num(std::f64::consts::E));
}

#[test]
fn test_math_sqrt_pow() {
    assert_eq!(builtin_math_sqrt(&[num(9.0)]).unwrap(), num(3.0));
    assert_eq!(builtin_math_pow(&[num(2.0), num(10.0)]).unwrap(), num(1024.0));
}

#[test]
fn test_math_trig() {
    assert_eq!(builtin_math_sin(&[num(0.0)]).unwrap(), num(0.0));
    assert_eq!(builtin_math_cos(&[num(0.0)]).unwrap(), num(1.0));
    assert_eq!(builtin_math_tan(&[num(0.0)]).unwrap(), num(0.0));
}

#[test]
fn test_math_log() {
    assert_eq!(builtin_math_log(&[num(1.0)]).unwrap(), num(0.0));
    assert_eq!(builtin_math_log10(&[num(100.0)]).unwrap(), num(2.0));
    assert_eq!(builtin_math_log2(&[num(8.0)]).unwrap(), num(3.0));
    // log with base
    assert_eq!(
        builtin_math_log(&[num(8.0), num(2.0)]).unwrap(),
        num(3.0)
    );
}

#[test]
fn test_math_sign() {
    assert_eq!(builtin_math_sign(&[num(42.0)]).unwrap(), num(1.0));
    assert_eq!(builtin_math_sign(&[num(-7.0)]).unwrap(), num(-1.0));
    assert_eq!(builtin_math_sign(&[num(0.0)]).unwrap(), num(0.0));
}

#[test]
fn test_math_clamp() {
    assert_eq!(
        builtin_math_clamp(&[num(15.0), num(0.0), num(10.0)]).unwrap(),
        num(10.0)
    );
    assert_eq!(
        builtin_math_clamp(&[num(-5.0), num(0.0), num(10.0)]).unwrap(),
        num(0.0)
    );
    assert_eq!(
        builtin_math_clamp(&[num(5.0), num(0.0), num(10.0)]).unwrap(),
        num(5.0)
    );
}

#[test]
fn test_math_lerp() {
    assert_eq!(
        builtin_math_lerp(&[num(0.0), num(10.0), num(0.5)]).unwrap(),
        num(5.0)
    );
    assert_eq!(
        builtin_math_lerp(&[num(0.0), num(10.0), num(0.0)]).unwrap(),
        num(0.0)
    );
    assert_eq!(
        builtin_math_lerp(&[num(0.0), num(10.0), num(1.0)]).unwrap(),
        num(10.0)
    );
}

#[test]
fn test_math_map() {
    assert_eq!(
        builtin_math_map(&[num(5.0), num(0.0), num(10.0), num(0.0), num(100.0)]).unwrap(),
        num(50.0)
    );
}

#[test]
fn test_math_deg_rad() {
    let result = builtin_math_deg2rad(&[num(180.0)]).unwrap();
    if let Value::Number(n) = result {
        assert!((n - std::f64::consts::PI).abs() < 1e-10);
    }
    let result = builtin_math_rad2deg(&[num(std::f64::consts::PI)]).unwrap();
    if let Value::Number(n) = result {
        assert!((n - 180.0).abs() < 1e-10);
    }
}

#[test]
fn test_math_is_checks() {
    assert_eq!(builtin_math_is_finite(&[num(1.0)]).unwrap(), Value::Bool(true));
    assert_eq!(
        builtin_math_is_finite(&[num(f64::INFINITY)]).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        builtin_math_is_infinite(&[num(f64::INFINITY)]).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        builtin_math_is_nan(&[num(f64::NAN)]).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(builtin_math_is_nan(&[num(1.0)]).unwrap(), Value::Bool(false));
}

#[test]
fn test_math_intdiv() {
    assert_eq!(
        builtin_math_intdiv(&[num(7.0), num(2.0)]).unwrap(),
        num(3.0)
    );
    assert!(builtin_math_intdiv(&[num(1.0), num(0.0)]).is_err());
}

#[test]
fn test_math_trunc_fract() {
    assert_eq!(builtin_math_trunc(&[num(4.7)]).unwrap(), num(4.0));
    assert_eq!(builtin_math_trunc(&[num(-4.7)]).unwrap(), num(-4.0));
    let result = builtin_math_fract(&[num(4.75)]).unwrap();
    if let Value::Number(n) = result {
        assert!((n - 0.75).abs() < 1e-10);
    }
}

#[test]
fn test_math_cbrt() {
    assert_eq!(builtin_math_cbrt(&[num(27.0)]).unwrap(), num(3.0));
}

#[test]
fn test_math_hypot() {
    assert_eq!(
        builtin_math_hypot(&[num(3.0), num(4.0)]).unwrap(),
        num(5.0)
    );
}

#[test]
fn test_math_fmod() {
    assert_eq!(
        builtin_math_fmod(&[num(10.0), num(3.0)]).unwrap(),
        num(1.0)
    );
}

#[test]
fn test_math_exp_expm1() {
    assert_eq!(builtin_math_exp(&[num(0.0)]).unwrap(), num(1.0));
    assert_eq!(builtin_math_expm1(&[num(0.0)]).unwrap(), num(0.0));
}

#[test]
fn test_math_atan2() {
    let result = builtin_math_atan2(&[num(1.0), num(1.0)]).unwrap();
    if let Value::Number(n) = result {
        assert!((n - std::f64::consts::FRAC_PI_4).abs() < 1e-10);
    }
}
