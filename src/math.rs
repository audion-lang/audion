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

use crate::error::{AudionError, Result};
use crate::value::Value;

fn require_number(fn_name: &str, val: &Value) -> Result<f64> {
    match val {
        Value::Number(n) => Ok(*n),
        other => Err(AudionError::RuntimeError {
            msg: format!("{}() expected number, got {}", fn_name, other.type_name()),
        }),
    }
}

fn require_at_least(fn_name: &str, args: &[Value], n: usize) -> Result<()> {
    if args.len() < n {
        Err(AudionError::RuntimeError {
            msg: format!(
                "{}() requires at least {} argument{}",
                fn_name,
                n,
                if n == 1 { "" } else { "s" }
            ),
        })
    } else {
        Ok(())
    }
}

// math_abs(n) → absolute value
pub fn builtin_math_abs(args: &[Value]) -> Result<Value> {
    require_at_least("math_abs", args, 1)?;
    let n = require_number("math_abs", &args[0])?;
    Ok(Value::Number(n.abs()))
}

// math_acos(n) → arc cosine in radians
pub fn builtin_math_acos(args: &[Value]) -> Result<Value> {
    require_at_least("math_acos", args, 1)?;
    let n = require_number("math_acos", &args[0])?;
    Ok(Value::Number(n.acos()))
}

// math_acosh(n) → inverse hyperbolic cosine
pub fn builtin_math_acosh(args: &[Value]) -> Result<Value> {
    require_at_least("math_acosh", args, 1)?;
    let n = require_number("math_acosh", &args[0])?;
    Ok(Value::Number(n.acosh()))
}

// math_asin(n) → arc sine in radians
pub fn builtin_math_asin(args: &[Value]) -> Result<Value> {
    require_at_least("math_asin", args, 1)?;
    let n = require_number("math_asin", &args[0])?;
    Ok(Value::Number(n.asin()))
}

// math_asinh(n) → inverse hyperbolic sine
pub fn builtin_math_asinh(args: &[Value]) -> Result<Value> {
    require_at_least("math_asinh", args, 1)?;
    let n = require_number("math_asinh", &args[0])?;
    Ok(Value::Number(n.asinh()))
}

// math_atan(n) → arc tangent in radians
pub fn builtin_math_atan(args: &[Value]) -> Result<Value> {
    require_at_least("math_atan", args, 1)?;
    let n = require_number("math_atan", &args[0])?;
    Ok(Value::Number(n.atan()))
}

// math_atan2(y, x) → arc tangent of y/x, using signs to determine quadrant
pub fn builtin_math_atan2(args: &[Value]) -> Result<Value> {
    require_at_least("math_atan2", args, 2)?;
    let y = require_number("math_atan2", &args[0])?;
    let x = require_number("math_atan2", &args[1])?;
    Ok(Value::Number(y.atan2(x)))
}

// math_atanh(n) → inverse hyperbolic tangent
pub fn builtin_math_atanh(args: &[Value]) -> Result<Value> {
    require_at_least("math_atanh", args, 1)?;
    let n = require_number("math_atanh", &args[0])?;
    Ok(Value::Number(n.atanh()))
}

// math_ceil(n) → round up
pub fn builtin_math_ceil(args: &[Value]) -> Result<Value> {
    require_at_least("math_ceil", args, 1)?;
    let n = require_number("math_ceil", &args[0])?;
    Ok(Value::Number(n.ceil()))
}

// math_cos(n) → cosine
pub fn builtin_math_cos(args: &[Value]) -> Result<Value> {
    require_at_least("math_cos", args, 1)?;
    let n = require_number("math_cos", &args[0])?;
    Ok(Value::Number(n.cos()))
}

// math_cosh(n) → hyperbolic cosine
pub fn builtin_math_cosh(args: &[Value]) -> Result<Value> {
    require_at_least("math_cosh", args, 1)?;
    let n = require_number("math_cosh", &args[0])?;
    Ok(Value::Number(n.cosh()))
}

// math_deg2rad(n) → convert degrees to radians
pub fn builtin_math_deg2rad(args: &[Value]) -> Result<Value> {
    require_at_least("math_deg2rad", args, 1)?;
    let n = require_number("math_deg2rad", &args[0])?;
    Ok(Value::Number(n.to_radians()))
}

// math_exp(n) → e^n
pub fn builtin_math_exp(args: &[Value]) -> Result<Value> {
    require_at_least("math_exp", args, 1)?;
    let n = require_number("math_exp", &args[0])?;
    Ok(Value::Number(n.exp()))
}

// math_expm1(n) → e^n - 1 (accurate near zero)
pub fn builtin_math_expm1(args: &[Value]) -> Result<Value> {
    require_at_least("math_expm1", args, 1)?;
    let n = require_number("math_expm1", &args[0])?;
    Ok(Value::Number(n.exp_m1()))
}

// math_floor(n) → round down
pub fn builtin_math_floor(args: &[Value]) -> Result<Value> {
    require_at_least("math_floor", args, 1)?;
    let n = require_number("math_floor", &args[0])?;
    Ok(Value::Number(n.floor()))
}

// math_fmod(x, y) → floating point remainder of x/y
pub fn builtin_math_fmod(args: &[Value]) -> Result<Value> {
    require_at_least("math_fmod", args, 2)?;
    let x = require_number("math_fmod", &args[0])?;
    let y = require_number("math_fmod", &args[1])?;
    Ok(Value::Number(x % y))
}

// math_hypot(x, y) → sqrt(x^2 + y^2)
pub fn builtin_math_hypot(args: &[Value]) -> Result<Value> {
    require_at_least("math_hypot", args, 2)?;
    let x = require_number("math_hypot", &args[0])?;
    let y = require_number("math_hypot", &args[1])?;
    Ok(Value::Number(x.hypot(y)))
}

// math_is_finite(n) → bool
pub fn builtin_math_is_finite(args: &[Value]) -> Result<Value> {
    require_at_least("math_is_finite", args, 1)?;
    let n = require_number("math_is_finite", &args[0])?;
    Ok(Value::Bool(n.is_finite()))
}

// math_is_infinite(n) → bool
pub fn builtin_math_is_infinite(args: &[Value]) -> Result<Value> {
    require_at_least("math_is_infinite", args, 1)?;
    let n = require_number("math_is_infinite", &args[0])?;
    Ok(Value::Bool(n.is_infinite()))
}

// math_is_nan(n) → bool
pub fn builtin_math_is_nan(args: &[Value]) -> Result<Value> {
    require_at_least("math_is_nan", args, 1)?;
    let n = require_number("math_is_nan", &args[0])?;
    Ok(Value::Bool(n.is_nan()))
}

// math_log(n) → natural logarithm
// math_log(n, base) → logarithm with specified base
pub fn builtin_math_log(args: &[Value]) -> Result<Value> {
    require_at_least("math_log", args, 1)?;
    let n = require_number("math_log", &args[0])?;
    if args.len() >= 2 {
        let base = require_number("math_log", &args[1])?;
        Ok(Value::Number(n.log(base)))
    } else {
        Ok(Value::Number(n.ln()))
    }
}

// math_log10(n) → base-10 logarithm
pub fn builtin_math_log10(args: &[Value]) -> Result<Value> {
    require_at_least("math_log10", args, 1)?;
    let n = require_number("math_log10", &args[0])?;
    Ok(Value::Number(n.log10()))
}

// math_log2(n) → base-2 logarithm
pub fn builtin_math_log2(args: &[Value]) -> Result<Value> {
    require_at_least("math_log2", args, 1)?;
    let n = require_number("math_log2", &args[0])?;
    Ok(Value::Number(n.log2()))
}

// math_log1p(n) → ln(1 + n) (accurate near zero)
pub fn builtin_math_log1p(args: &[Value]) -> Result<Value> {
    require_at_least("math_log1p", args, 1)?;
    let n = require_number("math_log1p", &args[0])?;
    Ok(Value::Number(n.ln_1p()))
}

// math_max(a, b, ...) → highest value (variadic)
pub fn builtin_math_max(args: &[Value]) -> Result<Value> {
    require_at_least("math_max", args, 1)?;
    let mut max = require_number("math_max", &args[0])?;
    for arg in &args[1..] {
        let n = require_number("math_max", arg)?;
        if n > max {
            max = n;
        }
    }
    Ok(Value::Number(max))
}

// math_min(a, b, ...) → lowest value (variadic)
pub fn builtin_math_min(args: &[Value]) -> Result<Value> {
    require_at_least("math_min", args, 1)?;
    let mut min = require_number("math_min", &args[0])?;
    for arg in &args[1..] {
        let n = require_number("math_min", arg)?;
        if n < min {
            min = n;
        }
    }
    Ok(Value::Number(min))
}

// math_pi() → 3.141592653589793
pub fn builtin_math_pi(_args: &[Value]) -> Result<Value> {
    Ok(Value::Number(std::f64::consts::PI))
}

// math_e() → 2.718281828459045
pub fn builtin_math_e(_args: &[Value]) -> Result<Value> {
    Ok(Value::Number(std::f64::consts::E))
}

// math_pow(base, exponent) → base^exponent
pub fn builtin_math_pow(args: &[Value]) -> Result<Value> {
    require_at_least("math_pow", args, 2)?;
    let base = require_number("math_pow", &args[0])?;
    let exp = require_number("math_pow", &args[1])?;
    Ok(Value::Number(base.powf(exp)))
}

// math_rad2deg(n) → convert radians to degrees
pub fn builtin_math_rad2deg(args: &[Value]) -> Result<Value> {
    require_at_least("math_rad2deg", args, 1)?;
    let n = require_number("math_rad2deg", &args[0])?;
    Ok(Value::Number(n.to_degrees()))
}

// math_round(n) → round to nearest integer
// math_round(n, precision) → round to N decimal places
pub fn builtin_math_round(args: &[Value]) -> Result<Value> {
    require_at_least("math_round", args, 1)?;
    let n = require_number("math_round", &args[0])?;
    if args.len() >= 2 {
        let precision = require_number("math_round", &args[1])? as i32;
        let factor = 10.0_f64.powi(precision);
        Ok(Value::Number((n * factor).round() / factor))
    } else {
        Ok(Value::Number(n.round()))
    }
}

// math_sin(n) → sine
pub fn builtin_math_sin(args: &[Value]) -> Result<Value> {
    require_at_least("math_sin", args, 1)?;
    let n = require_number("math_sin", &args[0])?;
    Ok(Value::Number(n.sin()))
}

// math_sinh(n) → hyperbolic sine
pub fn builtin_math_sinh(args: &[Value]) -> Result<Value> {
    require_at_least("math_sinh", args, 1)?;
    let n = require_number("math_sinh", &args[0])?;
    Ok(Value::Number(n.sinh()))
}

// math_sqrt(n) → square root
pub fn builtin_math_sqrt(args: &[Value]) -> Result<Value> {
    require_at_least("math_sqrt", args, 1)?;
    let n = require_number("math_sqrt", &args[0])?;
    Ok(Value::Number(n.sqrt()))
}

// math_tan(n) → tangent
pub fn builtin_math_tan(args: &[Value]) -> Result<Value> {
    require_at_least("math_tan", args, 1)?;
    let n = require_number("math_tan", &args[0])?;
    Ok(Value::Number(n.tan()))
}

// math_tanh(n) → hyperbolic tangent
pub fn builtin_math_tanh(args: &[Value]) -> Result<Value> {
    require_at_least("math_tanh", args, 1)?;
    let n = require_number("math_tanh", &args[0])?;
    Ok(Value::Number(n.tanh()))
}

// math_sign(n) → -1, 0, or 1
pub fn builtin_math_sign(args: &[Value]) -> Result<Value> {
    require_at_least("math_sign", args, 1)?;
    let n = require_number("math_sign", &args[0])?;
    let s = if n > 0.0 {
        1.0
    } else if n < 0.0 {
        -1.0
    } else {
        0.0
    };
    Ok(Value::Number(s))
}

// math_clamp(n, min, max) → clamp value between min and max
pub fn builtin_math_clamp(args: &[Value]) -> Result<Value> {
    require_at_least("math_clamp", args, 3)?;
    let n = require_number("math_clamp", &args[0])?;
    let min = require_number("math_clamp", &args[1])?;
    let max = require_number("math_clamp", &args[2])?;
    Ok(Value::Number(n.max(min).min(max)))
}

// math_intdiv(a, b) → integer division (truncated)
pub fn builtin_math_intdiv(args: &[Value]) -> Result<Value> {
    require_at_least("math_intdiv", args, 2)?;
    let a = require_number("math_intdiv", &args[0])?;
    let b = require_number("math_intdiv", &args[1])?;
    if b == 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "math_intdiv() division by zero".to_string(),
        });
    }
    Ok(Value::Number((a / b).trunc()))
}

// math_lerp(a, b, t) → linear interpolation: a + (b - a) * t
pub fn builtin_math_lerp(args: &[Value]) -> Result<Value> {
    require_at_least("math_lerp", args, 3)?;
    let a = require_number("math_lerp", &args[0])?;
    let b = require_number("math_lerp", &args[1])?;
    let t = require_number("math_lerp", &args[2])?;
    Ok(Value::Number(a + (b - a) * t))
}

// math_map(value, in_min, in_max, out_min, out_max) → remap value from one range to another
pub fn builtin_math_map(args: &[Value]) -> Result<Value> {
    require_at_least("math_map", args, 5)?;
    let value = require_number("math_map", &args[0])?;
    let in_min = require_number("math_map", &args[1])?;
    let in_max = require_number("math_map", &args[2])?;
    let out_min = require_number("math_map", &args[3])?;
    let out_max = require_number("math_map", &args[4])?;
    let t = if in_max == in_min {
        0.0
    } else {
        (value - in_min) / (in_max - in_min)
    };
    Ok(Value::Number(out_min + (out_max - out_min) * t))
}

// math_trunc(n) → truncate toward zero (remove fractional part)
pub fn builtin_math_trunc(args: &[Value]) -> Result<Value> {
    require_at_least("math_trunc", args, 1)?;
    let n = require_number("math_trunc", &args[0])?;
    Ok(Value::Number(n.trunc()))
}

// math_fract(n) → fractional part of a number
pub fn builtin_math_fract(args: &[Value]) -> Result<Value> {
    require_at_least("math_fract", args, 1)?;
    let n = require_number("math_fract", &args[0])?;
    Ok(Value::Number(n.fract()))
}

// math_cbrt(n) → cube root
pub fn builtin_math_cbrt(args: &[Value]) -> Result<Value> {
    require_at_least("math_cbrt", args, 1)?;
    let n = require_number("math_cbrt", &args[0])?;
    Ok(Value::Number(n.cbrt()))
}

// math_wrap(n, lo, hi) → cyclically wrap n into [lo, hi), works with floats
pub fn builtin_math_wrap(args: &[Value]) -> Result<Value> {
    require_at_least("math_wrap", args, 3)?;
    let n = require_number("math_wrap", &args[0])?;
    let lo = require_number("math_wrap", &args[1])?;
    let hi = require_number("math_wrap", &args[2])?;
    if hi == lo {
        return Ok(Value::Number(lo));
    }
    let range = hi - lo;
    let wrapped = lo + ((n - lo) % range + range) % range;
    Ok(Value::Number(wrapped))
}

// math_fold(n, lo, hi) → fold/bounce n back and forth between lo and hi, works with floats
pub fn builtin_math_fold(args: &[Value]) -> Result<Value> {
    require_at_least("math_fold", args, 3)?;
    let n = require_number("math_fold", &args[0])?;
    let lo = require_number("math_fold", &args[1])?;
    let hi = require_number("math_fold", &args[2])?;
    if hi == lo {
        return Ok(Value::Number(lo));
    }
    let range = hi - lo;
    // fold into [0, 2*range), then mirror the upper half
    let t = ((n - lo) % (2.0 * range) + 2.0 * range) % (2.0 * range);
    let folded = if t <= range { lo + t } else { hi - (t - range) };
    Ok(Value::Number(folded))
}

