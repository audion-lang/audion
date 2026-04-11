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

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use crate::ast::{Param, Stmt};
use crate::environment::Environment;

// ---------------------------------------------------------------------------
// ArrayKey — hashable wrapper for array keys
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArrayKey {
    Int(i64),
    Float(u64), // f64::to_bits() for non-integer numbers
    Str(String),
    Bool(bool),
    Nil,
}

pub fn to_array_key(v: &Value) -> Result<ArrayKey, String> {
    match v {
        Value::Number(n) => {
            if n.is_nan() {
                return Err("cannot use NaN as array key".to_string());
            }
            // Normalize -0.0 to 0.0
            let n = if *n == 0.0 { 0.0 } else { *n };
            let i = n as i64;
            if (i as f64) == n && n.is_finite() {
                Ok(ArrayKey::Int(i))
            } else {
                Ok(ArrayKey::Float(n.to_bits()))
            }
        }
        Value::String(s) => Ok(ArrayKey::Str(s.clone())),
        Value::Bool(b) => Ok(ArrayKey::Bool(*b)),
        Value::Nil => Ok(ArrayKey::Nil),
        other => Err(format!("cannot use {} as array key", other.type_name())),
    }
}

// ---------------------------------------------------------------------------
// AudionArray — ordered hash map with cursor and auto-index
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct AudionArray {
    entries: Vec<(Value, Value)>,
    key_to_pos: HashMap<ArrayKey, usize>,
    cursor: usize,
    next_int_key: i64,
}

impl AudionArray {
    pub fn new() -> Self {
        AudionArray {
            entries: Vec::new(),
            key_to_pos: HashMap::new(),
            cursor: 0,
            next_int_key: 0,
        }
    }

    pub fn get(&self, key: &Value) -> Option<&Value> {
        let ak = to_array_key(key).ok()?;
        let &pos = self.key_to_pos.get(&ak)?;
        Some(&self.entries[pos].1)
    }

    pub fn get_mut(&mut self, key: &Value) -> Option<&mut Value> {
        let ak = to_array_key(key).ok()?;
        let &pos = self.key_to_pos.get(&ak)?;
        Some(&mut self.entries[pos].1)
    }

    pub fn set(&mut self, key: Value, val: Value) {
        let ak = to_array_key(&key).unwrap();
        if let ArrayKey::Int(i) = &ak {
            if *i >= self.next_int_key {
                self.next_int_key = *i + 1;
            }
        }
        if let Some(&pos) = self.key_to_pos.get(&ak) {
            self.entries[pos].1 = val;
        } else {
            let pos = self.entries.len();
            self.key_to_pos.insert(ak, pos);
            self.entries.push((key, val));
        }
    }

    pub fn push_auto(&mut self, val: Value) -> i64 {
        let key_num = self.next_int_key;
        let key = Value::Number(key_num as f64);
        let ak = ArrayKey::Int(key_num);
        let pos = self.entries.len();
        self.key_to_pos.insert(ak, pos);
        self.entries.push((key, val));
        self.next_int_key = key_num + 1;
        self.entries.len() as i64
    }

    pub fn pop(&mut self) -> Option<(Value, Value)> {
        if let Some((k, v)) = self.entries.pop() {
            if let Ok(ak) = to_array_key(&k) {
                self.key_to_pos.remove(&ak);
            }
            if self.cursor >= self.entries.len() && self.cursor > 0 {
                self.cursor = self.entries.len().saturating_sub(1);
            }
            Some((k, v))
        } else {
            None
        }
    }

    pub fn remove(&mut self, key: &Value) -> Option<Value> {
        let ak = to_array_key(key).ok()?;
        let pos = self.key_to_pos.remove(&ak)?;
        let (_, val) = self.entries.remove(pos);
        // Fix positions for entries after the removed one
        for (_, p) in self.key_to_pos.iter_mut() {
            if *p > pos {
                *p -= 1;
            }
        }
        if self.cursor >= self.entries.len() && self.cursor > 0 {
            self.cursor = self.entries.len().saturating_sub(1);
        }
        Some(val)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[(Value, Value)] {
        &self.entries
    }

    // Cursor operations

    pub fn cursor_current(&self) -> Option<(&Value, &Value)> {
        self.entries.get(self.cursor).map(|(k, v)| (k, v))
    }

    pub fn cursor_next(&mut self, wrap: bool) -> Option<(&Value, &Value)> {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
            self.cursor_current()
        } else if wrap && !self.entries.is_empty() {
            self.cursor = 0;
            self.cursor_current()
        } else {
            None
        }
    }

    pub fn cursor_prev(&mut self, wrap: bool) -> Option<(&Value, &Value)> {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.cursor_current()
        } else if wrap && !self.entries.is_empty() {
            self.cursor = self.entries.len() - 1;
            self.cursor_current()
        } else {
            None
        }
    }

    pub fn cursor_beginning(&mut self) -> Option<(&Value, &Value)> {
        self.cursor = 0;
        self.cursor_current()
    }

    pub fn cursor_end(&mut self) -> Option<(&Value, &Value)> {
        if !self.entries.is_empty() {
            self.cursor = self.entries.len() - 1;
        }
        self.cursor_current()
    }

    pub fn cursor_key(&self) -> Option<&Value> {
        self.entries.get(self.cursor).map(|(k, _)| k)
    }

    pub fn deep_clone(&self) -> AudionArray {
        let entries: Vec<(Value, Value)> = self
            .entries
            .iter()
            .map(|(k, v)| (k.deep_clone(), v.deep_clone()))
            .collect();
        let key_to_pos = self.key_to_pos.clone();
        AudionArray {
            entries,
            key_to_pos,
            cursor: self.cursor,
            next_int_key: self.next_int_key,
        }
    }
}

// ---------------------------------------------------------------------------
// Value
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Value {
    Number(f64),
    String(String),
    Bool(bool),
    Nil,
    Function {
        name: String,
        params: Vec<Param>,
        body: Stmt,
        closure: Arc<Mutex<Environment>>,
    },
    BuiltinFn(String),
    Bytes(Vec<u8>),
    Array(Arc<Mutex<AudionArray>>),
    Object(Arc<Mutex<Environment>>),
    Namespace(Arc<Mutex<Environment>>),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Bytes(b) => !b.is_empty(),
            Value::Array(arr) => !arr.lock().unwrap().is_empty(),
            Value::Object(env) => !env.lock().unwrap().values().is_empty(),
            _ => true,
        }
    }

    pub fn type_name(&self) -> &str {
        match self {
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Nil => "nil",
            Value::Function { .. } => "function",
            Value::BuiltinFn(_) => "builtin",
            Value::Bytes(_) => "bytes",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Namespace(_) => "namespace",
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Deep clone — creates a fully independent copy of arrays and objects (recursively).
    /// Non-array values are cheaply cloned as usual.
    pub fn deep_clone(&self) -> Value {
        match self {
            Value::Array(arr) => {
                let guard = arr.lock().unwrap();
                Value::Array(Arc::new(Mutex::new(guard.deep_clone())))
            }
            Value::Object(env_arc) => {
                let env = env_arc.lock().unwrap();
                let parent = env.parent();
                let new_env = if let Some(p) = parent {
                    Arc::new(Mutex::new(Environment::new_child(p)))
                } else {
                    Arc::new(Mutex::new(Environment::new()))
                };
                // First pass: deep clone all non-function values
                for (name, value) in env.values() {
                    if !matches!(value, Value::Function { .. }) {
                        new_env.lock().unwrap().define(name.clone(), value.deep_clone());
                    }
                }
                // Second pass: clone functions, remapping closures that point to our env
                for (name, value) in env.values() {
                    if let Value::Function {
                        name: fn_name,
                        params,
                        body,
                        closure,
                    } = value
                    {
                        let new_closure = if Arc::ptr_eq(closure, env_arc) {
                            new_env.clone()
                        } else {
                            closure.clone()
                        };
                        new_env.lock().unwrap().define(
                            name.clone(),
                            Value::Function {
                                name: fn_name.clone(),
                                params: params.clone(),
                                body: body.clone(),
                                closure: new_closure,
                            },
                        );
                    }
                }
                Value::Object(new_env)
            }
            other => other.clone(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => {
                if *n == (*n as i64) as f64 && n.is_finite() {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Nil => write!(f, "nil"),
            Value::Function { name, params, .. } => {
                let param_strs: Vec<String> = params.iter().map(|p| {
                    if p.default.is_some() {
                        format!("{}=?", p.name)
                    } else {
                        p.name.clone()
                    }
                }).collect();
                write!(f, "<fn {}({})>", name, param_strs.join(", "))
            }
            Value::BuiltinFn(name) => write!(f, "<builtin {}>", name),
            Value::Bytes(b) => write!(f, "<bytes: {}>", b.len()),
            Value::Array(arr) => {
                let guard = arr.lock().unwrap();
                write!(f, "[")?;
                for (i, (key, val)) in guard.entries().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    match key {
                        Value::String(s) => write!(f, "\"{}\"", s)?,
                        other => write!(f, "{}", other)?,
                    }
                    write!(f, " => ")?;
                    match val {
                        Value::String(s) => write!(f, "\"{}\"", s)?,
                        other => write!(f, "{}", other)?,
                    }
                }
                write!(f, "]")
            }
            Value::Object(env) => {
                let e = env.lock().unwrap();
                let keys: Vec<&String> = e.values().keys().collect();
                write!(f, "<object {{{}}}>", keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", "))
            }
            Value::Namespace(env) => {
                let e = env.lock().unwrap();
                let keys: Vec<&String> = e.values().keys().collect();
                write!(f, "<namespace {{{}}}>", keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", "))
            }
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => {
                let a = a.lock().unwrap();
                let b = b.lock().unwrap();
                let ae = a.entries();
                let be = b.entries();
                ae.len() == be.len()
                    && ae.iter()
                        .zip(be.iter())
                        .all(|((k1, v1), (k2, v2))| k1 == k2 && v1 == v2)
            }
            (Value::Object(a), Value::Object(b)) => Arc::ptr_eq(a, b),
            (Value::Namespace(a), Value::Namespace(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
