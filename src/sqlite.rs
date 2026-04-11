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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::Connection;

use crate::error::{AudionError, Result};
use crate::value::{AudionArray, Value};

// ---------------------------------------------------------------------------
// SQLite connection handle store
// ---------------------------------------------------------------------------

static NEXT_SQLITE_HANDLE: AtomicU64 = AtomicU64::new(1);

fn sqlite_handles() -> &'static Mutex<HashMap<u64, Arc<Mutex<Connection>>>> {
    static HANDLES: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Connection>>>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn store_connection(conn: Connection) -> u64 {
    let id = NEXT_SQLITE_HANDLE.fetch_add(1, Ordering::Relaxed);
    sqlite_handles()
        .lock()
        .unwrap()
        .insert(id, Arc::new(Mutex::new(conn)));
    id
}

fn get_connection(id: u64) -> Option<Arc<Mutex<Connection>>> {
    sqlite_handles().lock().unwrap().get(&id).cloned()
}

fn remove_connection(id: u64) {
    sqlite_handles().lock().unwrap().remove(&id);
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn require_string(fn_name: &str, val: &Value) -> Result<String> {
    match val {
        Value::String(s) => Ok(s.clone()),
        other => Err(AudionError::RuntimeError {
            msg: format!("{}() expected string, got {}", fn_name, other.type_name()),
        }),
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

// Convert Audion Value to SQLite parameter
fn value_to_sql(val: &Value) -> rusqlite::types::Value {
    match val {
        Value::Number(n) => {
            // If it looks like an integer, store as INTEGER
            let i = *n as i64;
            if (i as f64) == *n && n.is_finite() {
                rusqlite::types::Value::Integer(i)
            } else {
                rusqlite::types::Value::Real(*n)
            }
        }
        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        Value::Nil => rusqlite::types::Value::Null,
        _ => rusqlite::types::Value::Text(val.to_string()),
    }
}

// Convert SQLite row to Audion array (associative array keyed by column names)
fn row_to_array(row: &rusqlite::Row, columns: &[String]) -> rusqlite::Result<Value> {
    let mut arr = AudionArray::new();
    for (i, col_name) in columns.iter().enumerate() {
        let val = match row.get_ref(i)? {
            rusqlite::types::ValueRef::Null => Value::Nil,
            rusqlite::types::ValueRef::Integer(n) => Value::Number(n as f64),
            rusqlite::types::ValueRef::Real(n) => Value::Number(n),
            rusqlite::types::ValueRef::Text(s) => {
                Value::String(String::from_utf8_lossy(s).to_string())
            }
            rusqlite::types::ValueRef::Blob(b) => {
                // Hex-encode blobs
                Value::String(b.iter().map(|byte| format!("{:02x}", byte)).collect())
            }
        };
        arr.set(Value::String(col_name.clone()), val);
    }
    Ok(Value::Array(Arc::new(Mutex::new(arr))))
}

// ---------------------------------------------------------------------------
// Public builtin functions
// ---------------------------------------------------------------------------

// sqlite_open(path) → handle (number) or false
// Use ":memory:" for in-memory database
pub fn builtin_sqlite_open(args: &[Value]) -> Result<Value> {
    require_at_least("sqlite_open", args, 1)?;
    let path = require_string("sqlite_open", &args[0])?;

    let conn = if path == ":memory:" {
        Connection::open_in_memory()
    } else {
        Connection::open(&path)
    };

    match conn {
        Ok(c) => {
            let id = store_connection(c);
            Ok(Value::Number(id as f64))
        }
        Err(_) => Ok(Value::Bool(false)),
    }
}

// sqlite_close(handle) → bool
pub fn builtin_sqlite_close(args: &[Value]) -> Result<Value> {
    require_at_least("sqlite_close", args, 1)?;
    let id = require_number("sqlite_close", &args[0])? as u64;
    remove_connection(id);
    Ok(Value::Bool(true))
}

// sqlite_exec(handle, sql, ...params) → number (rows affected)
pub fn builtin_sqlite_exec(args: &[Value]) -> Result<Value> {
    require_at_least("sqlite_exec", args, 2)?;
    let id = require_number("sqlite_exec", &args[0])? as u64;
    let sql = require_string("sqlite_exec", &args[1])?;

    let conn = match get_connection(id) {
        Some(c) => c,
        None => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_exec() invalid handle: {}", id),
            })
        }
    };

    let conn = conn.lock().unwrap();

    // Bind parameters from args[2..]
    let params: Vec<rusqlite::types::Value> = args[2..].iter().map(value_to_sql).collect();
    let params_refs: Vec<&dyn rusqlite::ToSql> = params
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect();

    match conn.execute(&sql, params_refs.as_slice()) {
        Ok(rows_affected) => Ok(Value::Number(rows_affected as f64)),
        Err(e) => Err(AudionError::RuntimeError {
            msg: format!("sqlite_exec() error: {}", e),
        }),
    }
}

// sqlite_query(handle, sql, ...params) → array of row arrays
pub fn builtin_sqlite_query(args: &[Value]) -> Result<Value> {
    require_at_least("sqlite_query", args, 2)?;
    let id = require_number("sqlite_query", &args[0])? as u64;
    let sql = require_string("sqlite_query", &args[1])?;

    let conn = match get_connection(id) {
        Some(c) => c,
        None => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_query() invalid handle: {}", id),
            })
        }
    };

    let conn = conn.lock().unwrap();

    // Bind parameters from args[2..]
    let params: Vec<rusqlite::types::Value> = args[2..].iter().map(value_to_sql).collect();
    let params_refs: Vec<&dyn rusqlite::ToSql> = params
        .iter()
        .map(|v| v as &dyn rusqlite::ToSql)
        .collect();

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_query() error: {}", e),
            })
        }
    };

    // Extract column names
    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    // Query rows
    let mut outer_arr = AudionArray::new();
    match stmt.query_map(params_refs.as_slice(), |row| row_to_array(row, &columns)) {
        Ok(rows) => {
            for row_result in rows {
                match row_result {
                    Ok(row_val) => {
                        outer_arr.push_auto(row_val);
                    }
                    Err(e) => {
                        return Err(AudionError::RuntimeError {
                            msg: format!("sqlite_query() row error: {}", e),
                        })
                    }
                }
            }
        }
        Err(e) => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_query() error: {}", e),
            })
        }
    }

    Ok(Value::Array(Arc::new(Mutex::new(outer_arr))))
}

// sqlite_tables(handle) → array of table name strings
pub fn builtin_sqlite_tables(args: &[Value]) -> Result<Value> {
    require_at_least("sqlite_tables", args, 1)?;
    let id = require_number("sqlite_tables", &args[0])? as u64;

    let conn = match get_connection(id) {
        Some(c) => c,
        None => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_tables() invalid handle: {}", id),
            })
        }
    };

    let conn = conn.lock().unwrap();

    let mut stmt = match conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name") {
        Ok(s) => s,
        Err(e) => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_tables() error: {}", e),
            })
        }
    };

    let mut arr = AudionArray::new();
    match stmt.query_map([], |row| row.get::<_, String>(0)) {
        Ok(rows) => {
            for row_result in rows {
                match row_result {
                    Ok(name) => {
                        arr.push_auto(Value::String(name));
                    }
                    Err(e) => {
                        return Err(AudionError::RuntimeError {
                            msg: format!("sqlite_tables() error: {}", e),
                        })
                    }
                }
            }
        }
        Err(e) => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_tables() error: {}", e),
            })
        }
    }

    Ok(Value::Array(Arc::new(Mutex::new(arr))))
}

// sqlite_table_exists(handle, name) → bool
pub fn builtin_sqlite_table_exists(args: &[Value]) -> Result<Value> {
    require_at_least("sqlite_table_exists", args, 2)?;
    let id = require_number("sqlite_table_exists", &args[0])? as u64;
    let table_name = require_string("sqlite_table_exists", &args[1])?;

    let conn = match get_connection(id) {
        Some(c) => c,
        None => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_table_exists() invalid handle: {}", id),
            })
        }
    };

    let conn = conn.lock().unwrap();

    let count: i64 = match conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [&table_name],
        |row| row.get(0),
    ) {
        Ok(c) => c,
        Err(e) => {
            return Err(AudionError::RuntimeError {
                msg: format!("sqlite_table_exists() error: {}", e),
            })
        }
    };

    Ok(Value::Bool(count > 0))
}
