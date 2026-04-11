// Based on algorithms from Exstrom Laboratories LLC (Copyright (C) 2013-2018)
// Original C implementation by Exstrom Laboratories LLC
// http://www.exstrom.com
//
// Copyright (C) 2013-2018 Exstrom Laboratories LLC
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// A copy of the GNU General Public License is available on the internet at:
// http://www.gnu.org/copyleft/gpl.html
//
// or you can write to:
//
// The Free Software Foundation, Inc.
// 675 Mass Ave
// Cambridge, MA 02139, USA
//
// Exstrom Laboratories LLC contact:
// stefan(AT)exstrom.com
//
// Exstrom Laboratories LLC
// Longmont, CO 80503, USA
//
//
//

use crate::error::{AudionError, Result};
use crate::value::{AudionArray, Value};
use std::sync::{Arc, Mutex};

//
// Utility Functions
//

/// Helper: require a number argument
fn require_number(fn_name: &str, val: &Value) -> Result<f64> {
    match val {
        Value::Number(n) => Ok(*n),
        _ => Err(AudionError::RuntimeError {
            msg: format!("{} requires a number", fn_name),
        }),
    }
}

/// Helper: require a string argument
fn require_string(fn_name: &str, val: &Value) -> Result<String> {
    match val {
        Value::String(s) => Ok(s.clone()),
        _ => Err(AudionError::RuntimeError {
            msg: format!("{} requires a string", fn_name),
        }),
    }
}

/// Helper: require an array argument
fn require_array(fn_name: &str, val: &Value) -> Result<Arc<Mutex<AudionArray>>> {
    match val {
        Value::Array(arr) => Ok(arr.clone()),
        _ => Err(AudionError::RuntimeError {
            msg: format!("{} requires an array", fn_name),
        }),
    }
}

//
// K-ary De Bruijn Sequences
//

/// Generates a k-ary de Bruijn sequence of order n
/// A k-ary de Bruijn sequence is a cyclic sequence where every possible
/// n-length string of k symbols occurs exactly once as a substring
///
/// Arguments:
///   k - number of symbols (2, 3, 4, ...)
///   n - sequence order (2, 3, 4, ...)
///   v - variant selector (0 to k^(n-1)-1, generates different sequences)
///
/// Returns a string of digits (0 to k-1) representing the sequence
///
/// Reference:
/// - H. Fredricksen and J. Maiorana, "Necklaces of beads in k colors and k-ary de Bruijn sequences"
///   Discrete Mathematics, 23:207–210, 1978
/// - https://www.combinatorics.org/ojs/index.php/eljc/article/view/v23i1p24
pub fn array_mel_debruijn_k(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_debruijn_k(k, n, v) requires 3 arguments".to_string(),
        });
    }

    let k = require_number("array_mel_debruijn_k", &args[0])? as usize;
    let n = require_number("array_mel_debruijn_k", &args[1])? as usize;
    let v0 = require_number("array_mel_debruijn_k", &args[2])? as usize;

    if k < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_debruijn_k() k must be >= 2".to_string(),
        });
    }

    if n < 1 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_debruijn_k() n must be >= 1".to_string(),
        });
    }

    if n > 8 || k > 10 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_debruijn_k() n too large (max 8) or k too large (max 10)".to_string(),
        });
    }

    // Calculate number of vertices: k^(n-1)
    let mut nv = 1;
    for _ in 1..n {
        nv *= k;
    }

    if v0 >= nv {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_debruijn_k() v must be < {} for k={}, n={}",
                nv, k, n
            ),
        });
    }

    let ls = k * nv; // length of sequence
    let mut vec = vec![0; nv]; // vertex edge count
    let mut cvl = vec![0; ls]; // cycle vertex list
    let mut seq = vec![0u8; ls];

    // Cycle finding function
    fn cycle(i0: usize, k: usize, nv: usize, vec: &mut [usize], cvl: &mut [usize]) -> usize {
        let mut clen = 1;
        cvl[0] = i0;
        let mut i1 = (k * i0) % nv + vec[i0];
        vec[i0] += 1;
        cvl[1] = i1;

        while i1 != i0 {
            clen += 1;
            let i2 = (k * i1) % nv + vec[i1];
            vec[i1] += 1;
            cvl[clen] = i2;
            i1 = i2;
        }
        clen
    }

    let mut is0 = 0;
    let mut is1 = ls;
    let mut i = 0;
    let mut v0_mut = v0;

    while i < ls {
        let clen0_initial = cycle(v0_mut, k, nv, &mut vec, &mut cvl);
        let mut clen0 = clen0_initial;
        let mut clen1 = 0;

        i += clen0;

        // Find next vertex with available edges
        for j in 1..clen0_initial {
            if vec[cvl[j]] < k {
                v0_mut = cvl[j];
                clen1 = clen0_initial - j;
                clen0 = j;
                break;
            }
        }

        // Fill sequence
        for j in 0..clen0 {
            seq[is0 + j] = ((cvl[j + 1] as i32 - ((k * cvl[j]) % nv) as i32) % k as i32) as u8;
        }
        is0 += clen0;
        is1 -= clen1;

        for j in 0..clen1 {
            seq[is1 + j] =
                ((cvl[clen0 + j + 1] as i32 - ((k * cvl[clen0 + j]) % nv) as i32) % k as i32)
                    as u8;
        }
    }

    // Convert to string
    let result = seq
        .iter()
        .map(|&d| (b'0' + d) as char)
        .collect::<String>();

    Ok(Value::String(result))
}

//
// Lattice Walks
//

/// Generates all walks on a square lattice from (x,y) to (a,b) with given path length
///
/// Arguments:
///   nc - number of columns
///   nr - number of rows
///   x, y - starting position
///   a, b - ending position
///   n - path length
///
/// Returns an array of strings, where each string is a path:
///   'r' = right, 'l' = left, 'u' = up, 'd' = down
///
/// Reference:
/// - Christian Krattenthaler, "Lattice Path Enumeration"
///   https://arxiv.org/pdf/1503.05930
pub fn array_mel_lattice_walk_square(args: &[Value]) -> Result<Value> {
    if args.len() < 7 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_square(nc, nr, x, y, a, b, n) requires 7 arguments"
                .to_string(),
        });
    }

    let nc = (require_number("array_mel_lattice_walk_square", &args[0])? as i32) - 1;
    let nr = (require_number("array_mel_lattice_walk_square", &args[1])? as i32) - 1;
    let x = require_number("array_mel_lattice_walk_square", &args[2])? as i32;
    let y = require_number("array_mel_lattice_walk_square", &args[3])? as i32;
    let a = require_number("array_mel_lattice_walk_square", &args[4])? as i32;
    let b = require_number("array_mel_lattice_walk_square", &args[5])? as i32;
    let walklen = require_number("array_mel_lattice_walk_square", &args[6])? as usize;

    if x > nc || x < 0 || y > nr || y < 0 {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_walk_square() invalid start position ({}, {})",
                x, y
            ),
        });
    }

    if a > nc || a < 0 || b > nr || b < 0 {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_walk_square() invalid end position ({}, {})",
                a, b
            ),
        });
    }

    if walklen > 128 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_square() path length too long (max 128)".to_string(),
        });
    }

    let mut result = AudionArray::new();
    let mut walkstr = vec![' '; walklen];

    fn walk(
        i: i32,
        j: i32,
        nstep: usize,
        nc: i32,
        nr: i32,
        a: i32,
        b: i32,
        walklen: usize,
        walkstr: &mut [char],
        result: &mut AudionArray,
    ) {
        if nstep == walklen && i == a && j == b {
            let path: String = walkstr.iter().collect();
            result.push_auto(Value::String(path));
            return;
        }

        if nstep < walklen {
            if i < nc {
                walkstr[nstep] = 'r';
                walk(i + 1, j, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            if i > 0 {
                walkstr[nstep] = 'l';
                walk(i - 1, j, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            if j < nr {
                walkstr[nstep] = 'u';
                walk(i, j + 1, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            if j > 0 {
                walkstr[nstep] = 'd';
                walk(i, j - 1, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
        }
    }

    walk(x, y, 0, nc, nr, a, b, walklen, &mut walkstr, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates all walks on a triangular lattice from (x,y) to (a,b) with given path length
///
/// Like square lattice but also includes diagonal moves:
///   'v' = diagonal down-right, 'e' = diagonal up-left
///
/// Reference:
/// - Christian Krattenthaler, "Lattice Path Enumeration"
///   https://arxiv.org/pdf/1503.05930
pub fn array_mel_lattice_walk_tri(args: &[Value]) -> Result<Value> {
    if args.len() < 7 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_tri(nc, nr, x, y, a, b, n) requires 7 arguments"
                .to_string(),
        });
    }

    let nc = (require_number("array_mel_lattice_walk_tri", &args[0])? as i32) - 1;
    let nr = (require_number("array_mel_lattice_walk_tri", &args[1])? as i32) - 1;
    let x = require_number("array_mel_lattice_walk_tri", &args[2])? as i32;
    let y = require_number("array_mel_lattice_walk_tri", &args[3])? as i32;
    let a = require_number("array_mel_lattice_walk_tri", &args[4])? as i32;
    let b = require_number("array_mel_lattice_walk_tri", &args[5])? as i32;
    let walklen = require_number("array_mel_lattice_walk_tri", &args[6])? as usize;

    if x > nc || x < 0 || y > nr || y < 0 {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_walk_tri() invalid start position ({}, {})",
                x, y
            ),
        });
    }

    if a > nc || a < 0 || b > nr || b < 0 {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_walk_tri() invalid end position ({}, {})",
                a, b
            ),
        });
    }

    if walklen > 128 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_tri() path length too long (max 128)".to_string(),
        });
    }

    let mut result = AudionArray::new();
    let mut walkstr = vec![' '; walklen];

    fn walk(
        i: i32,
        j: i32,
        nstep: usize,
        nc: i32,
        nr: i32,
        a: i32,
        b: i32,
        walklen: usize,
        walkstr: &mut [char],
        result: &mut AudionArray,
    ) {
        if nstep == walklen && i == a && j == b {
            let path: String = walkstr.iter().collect();
            result.push_auto(Value::String(path));
            return;
        }

        if nstep < walklen {
            // Square lattice moves
            if i < nc {
                walkstr[nstep] = 'r';
                walk(i + 1, j, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            if i > 0 {
                walkstr[nstep] = 'l';
                walk(i - 1, j, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            if j < nr {
                walkstr[nstep] = 'u';
                walk(i, j + 1, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            if j > 0 {
                walkstr[nstep] = 'd';
                walk(i, j - 1, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            // Diagonal moves
            if j < nr && i < nc {
                walkstr[nstep] = 'v';
                walk(i + 1, j + 1, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            if j > 0 && i > 0 {
                walkstr[nstep] = 'e';
                walk(i - 1, j - 1, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
        }
    }

    walk(x, y, 0, nc, nr, a, b, walklen, &mut walkstr, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// String to Note Indices
//

/// Maps a string to note indices for melodic generation
///
/// Arguments:
///   input_string - string to convert
///   num_notes - number of notes in your scale/palette
///
/// Returns an array of note indices (0 to num_notes-1)
///
/// Mapping:
///   - digits '0'-'9' → indices 0-9 (mod num_notes)
///   - lowercase 'a'-'z' → indices 10-35 (mod num_notes)
///   - uppercase 'A'-'Z' → indices 36-61 (mod num_notes)
///
/// Reference: Simple character-to-integer mapping (standard algorithm)
pub fn array_mel_string_to_indices(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_string_to_indices(string, num_notes) requires 2 arguments".to_string(),
        });
    }

    let input_str = require_string("array_mel_string_to_indices", &args[0])?;
    let num_notes = require_number("array_mel_string_to_indices", &args[1])? as i32;

    if num_notes < 1 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_string_to_indices() num_notes must be >= 1".to_string(),
        });
    }

    let mut result = AudionArray::new();

    for ch in input_str.chars() {
        let index = if ch.is_ascii_digit() {
            (ch as i32 - '0' as i32) % num_notes
        } else if ch.is_ascii_lowercase() {
            (ch as i32 - 'a' as i32 + 10) % num_notes
        } else if ch.is_ascii_uppercase() {
            (ch as i32 - 'A' as i32 + 36) % num_notes
        } else {
            0 // Default for non-alphanumeric
        };

        result.push_auto(Value::Number(index as f64));
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Random Walk with Boundaries
//

/// Generates a random walk within boundaries
///
/// Arguments:
///   start - starting value
///   min - minimum value (inclusive)
///   max - maximum value (inclusive)
///   step_size - maximum step size per move
///   length - number of steps
///
/// Returns an array of values representing the random walk
///
/// Reference: Standard bounded random walk algorithm
pub fn array_mel_random_walk(args: &[Value]) -> Result<Value> {
    if args.len() < 5 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_random_walk(start, min, max, step_size, length) requires 5 arguments"
                .to_string(),
        });
    }

    let mut current = require_number("array_mel_random_walk", &args[0])?;
    let min = require_number("array_mel_random_walk", &args[1])?;
    let max = require_number("array_mel_random_walk", &args[2])?;
    let step_size = require_number("array_mel_random_walk", &args[3])?;
    let length = require_number("array_mel_random_walk", &args[4])? as usize;

    if min >= max {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_random_walk() min must be < max".to_string(),
        });
    }

    if current < min || current > max {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_random_walk() start must be between min and max".to_string(),
        });
    }

    if step_size <= 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_random_walk() step_size must be > 0".to_string(),
        });
    }

    let mut result = AudionArray::new();

    for _ in 0..length {
        result.push_auto(Value::Number(current));

        // Random step in range [-step_size, step_size]
        let step = (crate::builtins::random_f64() * 2.0 - 1.0) * step_size;
        current += step;

        // Clamp to boundaries
        if current < min {
            current = min;
        }
        if current > max {
            current = max;
        }
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Contour Inversion
//

/// Inverts a melodic contour around a pivot point
///
/// Arguments:
///   melody - array of numbers
///   pivot - pivot point for inversion
///
/// Returns a new array with inverted values
///
/// Reference: Standard melodic inversion operation
pub fn array_mel_invert(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_invert(melody, pivot) requires 2 arguments".to_string(),
        });
    }

    let melody_arr = require_array("array_mel_invert", &args[0])?;
    let pivot = require_number("array_mel_invert", &args[1])?;

    let locked = melody_arr.lock().unwrap();
    let mut result = AudionArray::new();

    for (_key, val) in locked.entries() {
        match val {
            Value::Number(n) => {
                let inverted = pivot - (*n - pivot);
                result.push_auto(Value::Number(inverted));
            }
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_mel_invert() requires array of numbers".to_string(),
                })
            }
        }
    }
    drop(locked);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Reverses a melodic sequence
///
/// Arguments:
///   melody - array to reverse
///
/// Returns a new array with reversed order
///
/// Reference: Standard array reversal
pub fn array_mel_reverse(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_reverse(melody) requires 1 argument".to_string(),
        });
    }

    let melody_arr = require_array("array_mel_reverse", &args[0])?;
    let locked = melody_arr.lock().unwrap();

    let mut values: Vec<Value> = Vec::new();
    for (_key, val) in locked.entries() {
        values.push(val.clone());
    }
    drop(locked);

    values.reverse();

    let mut result = AudionArray::new();
    for val in values {
        result.push_auto(val);
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Square Lattice Walk Without Retracing
//

/// Generates all walks on a square lattice that don't retrace the previous step
///
/// Arguments:
///   nc - number of columns
///   nr - number of rows
///   x, y - starting position
///   a, b - ending position
///   n - path length
///
/// Returns an array of strings, where each string is a path:
///   'r' = right, 'l' = left, 'u' = up, 'd' = down
/// The walk cannot immediately reverse the previous move (no backtracking)
///
/// Reference:
/// - N. Madras, G. Slade, "The Self-Avoiding Walk", Birkhäuser, 1993
/// - https://en.wikipedia.org/wiki/Self-avoiding_walk
pub fn array_mel_lattice_walk_square_no_retrace(args: &[Value]) -> Result<Value> {
    if args.len() < 7 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_square_no_retrace(nc, nr, x, y, a, b, n) requires 7 arguments"
                .to_string(),
        });
    }

    let nc = (require_number("array_mel_lattice_walk_square_no_retrace", &args[0])? as i32) - 1;
    let nr = (require_number("array_mel_lattice_walk_square_no_retrace", &args[1])? as i32) - 1;
    let x = require_number("array_mel_lattice_walk_square_no_retrace", &args[2])? as i32;
    let y = require_number("array_mel_lattice_walk_square_no_retrace", &args[3])? as i32;
    let a = require_number("array_mel_lattice_walk_square_no_retrace", &args[4])? as i32;
    let b = require_number("array_mel_lattice_walk_square_no_retrace", &args[5])? as i32;
    let walklen = require_number("array_mel_lattice_walk_square_no_retrace", &args[6])? as usize;

    if x > nc || x < 0 || y > nr || y < 0 {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_walk_square_no_retrace() invalid start position ({}, {})",
                x, y
            ),
        });
    }

    if a > nc || a < 0 || b > nr || b < 0 {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_walk_square_no_retrace() invalid end position ({}, {})",
                a, b
            ),
        });
    }

    if walklen > 128 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_square_no_retrace() path length too long (max 128)"
                .to_string(),
        });
    }

    let mut result = AudionArray::new();
    let mut walkstr = vec![' '; walklen];

    fn walk(
        i: i32,
        j: i32,
        nstep: usize,
        nc: i32,
        nr: i32,
        a: i32,
        b: i32,
        walklen: usize,
        walkstr: &mut [char],
        result: &mut AudionArray,
    ) {
        if nstep == walklen && i == a && j == b {
            let path: String = walkstr.iter().collect();
            result.push_auto(Value::String(path));
            return;
        }

        if nstep < walklen {
            let prev = if nstep > 0 {
                Some(walkstr[nstep - 1])
            } else {
                None
            };

            // Can move right if not at boundary and previous wasn't left
            if i < nc && prev != Some('l') {
                walkstr[nstep] = 'r';
                walk(i + 1, j, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            // Can move left if not at boundary and previous wasn't right
            if i > 0 && prev != Some('r') {
                walkstr[nstep] = 'l';
                walk(i - 1, j, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            // Can move up if not at boundary and previous wasn't down
            if j < nr && prev != Some('d') {
                walkstr[nstep] = 'u';
                walk(i, j + 1, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
            // Can move down if not at boundary and previous wasn't up
            if j > 0 && prev != Some('u') {
                walkstr[nstep] = 'd';
                walk(i, j - 1, nstep + 1, nc, nr, a, b, walklen, walkstr, result);
            }
        }
    }

    walk(x, y, 0, nc, nr, a, b, walklen, &mut walkstr, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Square Lattice Walk With Stops
//

/// Generates all walks on a square lattice that can stop at a point for multiple steps
///
/// Arguments:
///   nc - number of columns
///   nr - number of rows
///   x, y - starting position
///   a, b - ending position
///   max_stops - maximum number of consecutive stop steps allowed
///   n - walk length
///
/// Returns an array of strings, where each string is a path:
///   'r' = right, 'l' = left, 'u' = up, 'd' = down, 's' = stop (stay in place)
///
/// Reference:
/// - Christian Krattenthaler, "Lattice Path Enumeration"
///   https://arxiv.org/pdf/1503.05930
pub fn array_mel_lattice_walk_square_with_stops(args: &[Value]) -> Result<Value> {
    if args.len() < 8 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_square_with_stops(nc, nr, x, y, a, b, max_stops, n) requires 8 arguments"
                .to_string(),
        });
    }

    let nc = (require_number("array_mel_lattice_walk_square_with_stops", &args[0])? as i32) - 1;
    let nr = (require_number("array_mel_lattice_walk_square_with_stops", &args[1])? as i32) - 1;
    let x = require_number("array_mel_lattice_walk_square_with_stops", &args[2])? as i32;
    let y = require_number("array_mel_lattice_walk_square_with_stops", &args[3])? as i32;
    let a = require_number("array_mel_lattice_walk_square_with_stops", &args[4])? as i32;
    let b = require_number("array_mel_lattice_walk_square_with_stops", &args[5])? as i32;
    let maxstop = require_number("array_mel_lattice_walk_square_with_stops", &args[6])? as i32;
    let walklen = require_number("array_mel_lattice_walk_square_with_stops", &args[7])? as usize;

    if x > nc || x < 0 || y > nr || y < 0 {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_walk_square_with_stops() invalid start position ({}, {})",
                x, y
            ),
        });
    }

    if a > nc || a < 0 || b > nr || b < 0 {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_walk_square_with_stops() invalid end position ({}, {})",
                a, b
            ),
        });
    }

    if maxstop < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_square_with_stops() max_stops must be >= 0".to_string(),
        });
    }

    if walklen > 128 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_walk_square_with_stops() walk length too long (max 128)"
                .to_string(),
        });
    }

    let mut result = AudionArray::new();
    let mut walkstr = vec![' '; walklen];

    fn walk(
        i: i32,
        j: i32,
        nstop: i32,
        nstep: usize,
        nc: i32,
        nr: i32,
        a: i32,
        b: i32,
        maxstop: i32,
        walklen: usize,
        walkstr: &mut [char],
        result: &mut AudionArray,
    ) {
        if nstep == walklen && i == a && j == b {
            let path: String = walkstr.iter().collect();
            result.push_auto(Value::String(path));
            return;
        }

        if nstep < walklen {
            // Can stop if haven't exceeded max stops
            if nstop < maxstop {
                walkstr[nstep] = 's';
                walk(
                    i, j, nstop + 1, nstep + 1, nc, nr, a, b, maxstop, walklen, walkstr, result,
                );
            }
            // Move in any direction (resets stop counter)
            if i < nc {
                walkstr[nstep] = 'r';
                walk(
                    i + 1, j, 0, nstep + 1, nc, nr, a, b, maxstop, walklen, walkstr, result,
                );
            }
            if i > 0 {
                walkstr[nstep] = 'l';
                walk(
                    i - 1, j, 0, nstep + 1, nc, nr, a, b, maxstop, walklen, walkstr, result,
                );
            }
            if j < nr {
                walkstr[nstep] = 'u';
                walk(
                    i, j + 1, 0, nstep + 1, nc, nr, a, b, maxstop, walklen, walkstr, result,
                );
            }
            if j > 0 {
                walkstr[nstep] = 'd';
                walk(
                    i, j - 1, 0, nstep + 1, nc, nr, a, b, maxstop, walklen, walkstr, result,
                );
            }
        }
    }

    walk(
        x, y, 0, 0, nc, nr, a, b, maxstop, walklen, &mut walkstr, &mut result,
    );

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Subset Sampling
//

/// Randomly samples a subset from an array
///
/// Arguments:
///   input_array - array to sample from
///   subset_size - number of elements to sample
///
/// Returns a new array containing a random subset.
/// All subsets of the given size have equal probability.
/// Uses algorithm S (reservoir sampling variant)
///
/// Reference:
/// - Jeffrey Scott Vitter, "Random Sampling with a Reservoir"
///   ACM Transactions on Mathematical Software, 11(1):37-57, March 1985
/// - https://www.cs.umd.edu/~samir/498/vitter.pdf
pub fn array_mel_subset_sample(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_subset_sample(array, subset_size) requires 2 arguments".to_string(),
        });
    }

    let input_arr = require_array("array_mel_subset_sample", &args[0])?;
    let m = require_number("array_mel_subset_sample", &args[1])? as usize;

    let locked = input_arr.lock().unwrap();
    let n = locked.entries().len();

    if m > n {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_subset_sample() subset_size ({}) cannot be larger than array size ({})",
                m, n
            ),
        });
    }

    if m == 0 {
        drop(locked);
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    // Collect all values into a vector for easier indexing
    let values: Vec<Value> = locked.entries().iter().map(|(_k, v)| v.clone()).collect();
    drop(locked);

    // Algorithm S - probability-based selection
    let mut result = AudionArray::new();
    let mut mp = m;
    let mut np = n;

    for i in 0..n {
        // Probability that this element should be selected
        let p = mp as f64 / np as f64;

        if crate::builtins::random_f64() < p {
            result.push_auto(values[i].clone());
            mp -= 1;
            if mp == 0 {
                break;
            }
        }
        np -= 1;
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Lattice Walk to Melody Converter
//

/// Converts a lattice walk path to a melody by mapping positions to notes
///
/// Arguments:
///   note_grid - 2D array (array of arrays) where note_grid[row][col] = note value
///   path_string - walk path ('r', 'l', 'u', 'd', 'v', 'e', 's')
///   start_x - starting column
///   start_y - starting row
///
/// Returns an array of note values following the path through the grid.
/// Grid wraps around at edges (toroidal topology).
///
/// Path characters:
///   'r' = right, 'l' = left, 'u' = up, 'd' = down
///   'v' = diagonal down-right, 'e' = diagonal up-left
///   's' = stay (repeat current note)
///
/// Reference: Lattice path interpretation for algorithmic composition (standard technique)
pub fn array_mel_lattice_to_melody(args: &[Value]) -> Result<Value> {
    if args.len() < 4 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_to_melody(note_grid, path_string, start_x, start_y) requires 4 arguments"
                .to_string(),
        });
    }

    let grid_arr = require_array("array_mel_lattice_to_melody", &args[0])?;
    let path = require_string("array_mel_lattice_to_melody", &args[1])?;
    let mut x = require_number("array_mel_lattice_to_melody", &args[2])? as i32;
    let mut y = require_number("array_mel_lattice_to_melody", &args[3])? as i32;

    // Parse the grid into a 2D structure
    let locked = grid_arr.lock().unwrap();
    let mut grid: Vec<Vec<Value>> = Vec::new();

    for (_key, row_val) in locked.entries().iter() {
        match row_val {
            Value::Array(row_arr) => {
                let row_locked = row_arr.lock().unwrap();
                let row_data: Vec<Value> = row_locked
                    .entries()
                    .iter()
                    .map(|(_k, v)| v.clone())
                    .collect();
                grid.push(row_data);
                drop(row_locked);
            }
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_mel_lattice_to_melody() note_grid must be a 2D array".to_string(),
                })
            }
        }
    }
    drop(locked);

    if grid.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_to_melody() note_grid cannot be empty".to_string(),
        });
    }

    let nr = grid.len() as i32;
    let nc = if grid[0].is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_lattice_to_melody() note_grid rows cannot be empty".to_string(),
        });
    } else {
        grid[0].len() as i32
    };

    if x < 0 || x >= nc || y < 0 || y >= nr {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_lattice_to_melody() invalid start position ({}, {})",
                x, y
            ),
        });
    }

    let mut result = AudionArray::new();
    // Add starting note
    result.push_auto(grid[y as usize][x as usize].clone());

    // Follow the path
    for ch in path.chars() {
        match ch {
            'u' => {
                y += 1;
                if y >= nr {
                    y = 0;
                }
            }
            'd' => {
                y -= 1;
                if y < 0 {
                    y = nr - 1;
                }
            }
            'l' => {
                x -= 1;
                if x < 0 {
                    x = nc - 1;
                }
            }
            'r' => {
                x += 1;
                if x >= nc {
                    x = 0;
                }
            }
            'v' => {
                // diagonal down-right
                x += 1;
                y += 1;
                if x >= nc {
                    x = 0;
                }
                if y >= nr {
                    y = 0;
                }
            }
            'e' => {
                // diagonal up-left
                x -= 1;
                y -= 1;
                if x < 0 {
                    x = nc - 1;
                }
                if y < 0 {
                    y = nr - 1;
                }
            }
            's' => {
                // stay - no position change
            }
            _ => {
                // Ignore unknown characters
            }
        }

        result.push_auto(grid[y as usize][x as usize].clone());
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Finite Automaton Word Generator
//

/// Generates all words of a given length accepted by a finite automaton
///
/// Arguments:
///   automaton - array of states, where each state is:
///               ["state_name", [[next_state, "output"], ...]]
///   start_state - index of starting state
///   end_states - array of ending state indices
///   word_length - length of words to generate
///
/// Returns an array of strings, where each string is a generated word
///
/// Reference:
/// - J. E. Hopcroft, J. D. Ullman, "Introduction to Automata Theory, Languages, and Computation"
///   Addison-Wesley, 1979
/// - https://en.wikipedia.org/wiki/Finite-state_machine
pub fn array_mel_automaton(args: &[Value]) -> Result<Value> {
    if args.len() < 4 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_automaton(automaton, start_state, end_states, word_length) requires 4 arguments"
                .to_string(),
        });
    }

    let aut_arr = require_array("array_mel_automaton", &args[0])?;
    let start = require_number("array_mel_automaton", &args[1])? as usize;
    let end_arr = require_array("array_mel_automaton", &args[2])?;
    let nstep = require_number("array_mel_automaton", &args[3])? as usize;

    // Parse automaton structure
    let locked = aut_arr.lock().unwrap();
    let mut automaton: Vec<Vec<(usize, String)>> = Vec::new();

    for (_key, state_val) in locked.entries().iter() {
        match state_val {
            Value::Array(state_arr) => {
                let state_locked = state_arr.lock().unwrap();
                let mut transitions: Vec<(usize, String)> = Vec::new();

                // Each element should be a transition array [next_state, output]
                for (_k, trans_val) in state_locked.entries().iter() {
                    match trans_val {
                        Value::Array(trans_arr) => {
                            let trans_locked = trans_arr.lock().unwrap();
                            let trans_data: Vec<Value> =
                                trans_locked.entries().iter().map(|(_k, v)| v.clone()).collect();

                            if trans_data.len() >= 2 {
                                let next_state = match &trans_data[0] {
                                    Value::Number(n) => *n as usize,
                                    _ => {
                                        return Err(AudionError::RuntimeError {
                                            msg: "array_mel_automaton() transition next_state must be a number"
                                                .to_string(),
                                        })
                                    }
                                };
                                let output = match &trans_data[1] {
                                    Value::String(s) => s.clone(),
                                    _ => {
                                        return Err(AudionError::RuntimeError {
                                            msg: "array_mel_automaton() transition output must be a string"
                                                .to_string(),
                                        })
                                    }
                                };
                                transitions.push((next_state, output));
                            }
                            drop(trans_locked);
                        }
                        _ => {
                            return Err(AudionError::RuntimeError {
                                msg: "array_mel_automaton() transitions must be arrays".to_string(),
                            })
                        }
                    }
                }

                automaton.push(transitions);
                drop(state_locked);
            }
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_mel_automaton() automaton must be array of arrays".to_string(),
                })
            }
        }
    }
    drop(locked);

    // Parse end states
    let end_locked = end_arr.lock().unwrap();
    let end_states: Vec<usize> = end_locked
        .entries()
        .iter()
        .filter_map(|(_k, v)| match v {
            Value::Number(n) => Some(*n as usize),
            _ => None,
        })
        .collect();
    drop(end_locked);

    if start >= automaton.len() {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_automaton() invalid start_state {} (automaton has {} states)",
                start,
                automaton.len()
            ),
        });
    }

    let mut result = AudionArray::new();
    let mut word = vec![String::new(); nstep];

    fn step(
        istate: usize,
        istep: usize,
        nstep: usize,
        automaton: &[Vec<(usize, String)>],
        end_states: &[usize],
        word: &mut [String],
        result: &mut AudionArray,
    ) {
        if istep == nstep {
            // Check if this is an accepting state
            if end_states.contains(&istate) {
                let output = word.join("");
                result.push_auto(Value::String(output));
            }
            return;
        }

        if istep < nstep && istate < automaton.len() {
            for (next_state, output) in &automaton[istate] {
                word[istep] = output.clone();
                step(*next_state, istep + 1, nstep, automaton, end_states, word, result);
            }
        }
    }

    step(start, 0, nstep, &automaton, &end_states, &mut word, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Probabilistic Automaton Word Generator
//

/// Generates a word from a probabilistic finite automaton
///
/// Arguments:
///   automaton - array of states, where each state is an array of transitions:
///               [[[next_state, "output", probability], ...], ...]
///   start_state - index of starting state
///   word_length - length of word to generate
///
/// Returns a string generated by following probabilistic transitions
///
/// Reference:
/// - M. O. Rabin, "Probabilistic automata"
///   Information and Control, 6(3):230-245, 1963
/// - https://www.sciencedirect.com/science/article/pii/S0019995863902900
pub fn array_mel_probabilistic_automaton(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(AudionError::RuntimeError {
            msg: "array_mel_probabilistic_automaton(automaton, start_state, word_length) requires 3 arguments"
                .to_string(),
        });
    }

    let aut_arr = require_array("array_mel_probabilistic_automaton", &args[0])?;
    let start = require_number("array_mel_probabilistic_automaton", &args[1])? as usize;
    let nstep = require_number("array_mel_probabilistic_automaton", &args[2])? as usize;

    // Parse automaton structure
    let locked = aut_arr.lock().unwrap();
    let mut automaton: Vec<Vec<(usize, String, f64)>> = Vec::new();

    for (_key, state_val) in locked.entries().iter() {
        match state_val {
            Value::Array(state_arr) => {
                let state_locked = state_arr.lock().unwrap();
                let mut transitions: Vec<(usize, String, f64)> = Vec::new();

                // Each element should be a transition array [next_state, output, probability]
                for (_k, trans_val) in state_locked.entries().iter() {
                    match trans_val {
                        Value::Array(trans_arr) => {
                            let trans_locked = trans_arr.lock().unwrap();
                            let trans_data: Vec<Value> =
                                trans_locked.entries().iter().map(|(_k, v)| v.clone()).collect();

                            if trans_data.len() >= 3 {
                                let next_state = match &trans_data[0] {
                                    Value::Number(n) => *n as usize,
                                    _ => {
                                        return Err(AudionError::RuntimeError {
                                            msg: "array_mel_probabilistic_automaton() next_state must be a number"
                                                .to_string(),
                                        })
                                    }
                                };
                                let output = match &trans_data[1] {
                                    Value::String(s) => s.clone(),
                                    _ => {
                                        return Err(AudionError::RuntimeError {
                                            msg: "array_mel_probabilistic_automaton() output must be a string"
                                                .to_string(),
                                        })
                                    }
                                };
                                let prob = match &trans_data[2] {
                                    Value::Number(n) => *n,
                                    _ => {
                                        return Err(AudionError::RuntimeError {
                                            msg: "array_mel_probabilistic_automaton() probability must be a number"
                                                .to_string(),
                                        })
                                    }
                                };
                                transitions.push((next_state, output, prob));
                            }
                            drop(trans_locked);
                        }
                        _ => {
                            return Err(AudionError::RuntimeError {
                                msg: "array_mel_probabilistic_automaton() transitions must be arrays"
                                    .to_string(),
                            })
                        }
                    }
                }

                automaton.push(transitions);
                drop(state_locked);
            }
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_mel_probabilistic_automaton() automaton must be array of arrays"
                        .to_string(),
                })
            }
        }
    }
    drop(locked);

    if start >= automaton.len() {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_mel_probabilistic_automaton() invalid start_state {} (automaton has {} states)",
                start,
                automaton.len()
            ),
        });
    }

    let mut result_str = String::new();
    let mut istate = start;

    for _ in 0..nstep {
        if istate >= automaton.len() || automaton[istate].is_empty() {
            break;
        }

        let u = crate::builtins::random_f64();
        let mut cumulative = 0.0;
        let mut found = false;

        for (next_state, output, prob) in &automaton[istate] {
            cumulative += prob;
            if u < cumulative {
                result_str.push_str(output);
                istate = *next_state;
                found = true;
                break;
            }
        }

        if !found {
            // Shouldn't happen if probabilities sum to 1, but handle gracefully
            break;
        }
    }

    Ok(Value::String(result_str))
}
