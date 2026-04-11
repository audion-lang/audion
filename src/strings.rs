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

use crate::builtins::extract_regex_pattern;
use crate::error::{AudionError, Result};
use crate::value::Value;

fn require_string(fn_name: &str, val: &Value) -> Result<String> {
    match val {
        Value::String(s) => Ok(s.clone()),
        other => Err(AudionError::RuntimeError {
            msg: format!("{}() expected string, got {}", fn_name, other.type_name()),
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

fn require_number(fn_name: &str, val: &Value) -> Result<f64> {
    match val {
        Value::Number(n) => Ok(*n),
        other => Err(AudionError::RuntimeError {
            msg: format!("{}() expected number, got {}", fn_name, other.type_name()),
        }),
    }
}

// str_replace(needle, replacement, haystack) → string
// Supports regex via /pattern/, {pattern}, %%pattern%% delimiters.
pub fn builtin_str_replace(args: &[Value]) -> Result<Value> {
    require_at_least("str_replace", args, 3)?;
    let needle = require_string("str_replace", &args[0])?;
    let replacement = require_string("str_replace", &args[1])?;
    let haystack = require_string("str_replace", &args[2])?;

    let result = if let Some(pattern) = extract_regex_pattern(&needle) {
        let re = regex::Regex::new(pattern).map_err(|e| AudionError::RuntimeError {
            msg: format!("str_replace() invalid regex: {}", e),
        })?;
        re.replace_all(&haystack, replacement.as_str()).to_string()
    } else {
        haystack.replace(&needle, &replacement)
    };
    Ok(Value::String(result))
}

// str_contains(needle, haystack) → bool
// Supports regex via /pattern/, {pattern}, %%pattern%% delimiters.
pub fn builtin_str_contains(args: &[Value]) -> Result<Value> {
    require_at_least("str_contains", args, 2)?;
    let needle = require_string("str_contains", &args[0])?;
    let haystack = require_string("str_contains", &args[1])?;

    let found = if let Some(pattern) = extract_regex_pattern(&needle) {
        let re = regex::Regex::new(pattern).map_err(|e| AudionError::RuntimeError {
            msg: format!("str_contains() invalid regex: {}", e),
        })?;
        re.is_match(&haystack)
    } else {
        haystack.contains(&*needle)
    };
    Ok(Value::Bool(found))
}

// str_upper(string) → uppercase string
pub fn builtin_str_upper(args: &[Value]) -> Result<Value> {
    require_at_least("str_upper", args, 1)?;
    let s = require_string("str_upper", &args[0])?;
    Ok(Value::String(s.to_uppercase()))
}

// str_lower(string) → lowercase string
pub fn builtin_str_lower(args: &[Value]) -> Result<Value> {
    require_at_least("str_lower", args, 1)?;
    let s = require_string("str_lower", &args[0])?;
    Ok(Value::String(s.to_lowercase()))
}

// str_trim(string) → trimmed string
pub fn builtin_str_trim(args: &[Value]) -> Result<Value> {
    require_at_least("str_trim", args, 1)?;
    let s = require_string("str_trim", &args[0])?;
    Ok(Value::String(s.trim().to_string()))
}

// str_length(string) → character count (Unicode-safe)
pub fn builtin_str_length(args: &[Value]) -> Result<Value> {
    require_at_least("str_length", args, 1)?;
    let s = require_string("str_length", &args[0])?;
    Ok(Value::Number(s.chars().count() as f64))
}

// str_substr(string, start, [length]) → substring
pub fn builtin_str_substr(args: &[Value]) -> Result<Value> {
    require_at_least("str_substr", args, 2)?;
    let s = require_string("str_substr", &args[0])?;
    let start = require_number("str_substr", &args[1])? as usize;

    let result: String = if args.len() >= 3 {
        let length = require_number("str_substr", &args[2])? as usize;
        s.chars().skip(start).take(length).collect()
    } else {
        s.chars().skip(start).collect()
    };
    Ok(Value::String(result))
}

// str_starts_with(prefix, string) → bool
pub fn builtin_str_starts_with(args: &[Value]) -> Result<Value> {
    require_at_least("str_starts_with", args, 2)?;
    let prefix = require_string("str_starts_with", &args[0])?;
    let string = require_string("str_starts_with", &args[1])?;
    Ok(Value::Bool(string.starts_with(&*prefix)))
}

// str_ends_with(suffix, string) → bool
pub fn builtin_str_ends_with(args: &[Value]) -> Result<Value> {
    require_at_least("str_ends_with", args, 2)?;
    let suffix = require_string("str_ends_with", &args[0])?;
    let string = require_string("str_ends_with", &args[1])?;
    Ok(Value::Bool(string.ends_with(&*suffix)))
}
