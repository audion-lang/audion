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
// Binary/Interval Conversions
//

/// Converts a binary pattern string to interval notation
/// Example: "1010010001001000" -> [2, 3, 4, 3, 4]
///
/// Reference: Standard binary-to-interval conversion for rhythmic patterns
pub fn array_seq_binary_to_intervals(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_binary_to_intervals() requires a binary string argument".to_string(),
        });
    }

    let binary_str = require_string("array_seq_binary_to_intervals", &args[0])?;
    let chars: Vec<char> = binary_str.chars().collect();
    let nbit = chars.len();

    if nbit == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut intervals = AudionArray::new();
    let mut j = 0;

    while j < nbit {
        let mut k = 1;
        j += 1;
        while j < nbit && chars[j] != '1' {
            k += 1;
            j += 1;
        }
        intervals.push_auto(Value::Number(k as f64));
    }

    Ok(Value::Array(Arc::new(Mutex::new(intervals))))
}

/// Converts interval notation to a binary pattern string
/// Example: [2, 3, 4, 3, 4] -> "1010010001001000"
///
/// Reference: Standard interval-to-binary conversion for rhythmic patterns
pub fn array_seq_intervals_to_binary(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_intervals_to_binary() requires an array argument".to_string(),
        });
    }

    let arr = require_array("array_seq_intervals_to_binary", &args[0])?;
    let locked = arr.lock().unwrap();
    let mut result = String::new();

    for (_key, val) in locked.entries() {
        let interval = match val {
            Value::Number(n) => *n as i32,
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_intervals_to_binary() requires array of numbers".to_string(),
                })
            }
        };

        if interval < 1 {
            return Err(AudionError::RuntimeError {
                msg: "array_seq_intervals_to_binary() requires positive intervals".to_string(),
            });
        }

        result.push('1');
        for _ in 1..interval {
            result.push('0');
        }
    }

    Ok(Value::String(result))
}

//
// Random Number Generation
//

/// Generates random numbers with specified correlation
/// Arguments: m (range 0 to m), s (starting number), c (correlation degree), n (count)
///   c = 0: total correlation (all numbers = s)
///   c = m: no correlation (each number is independent)
///
/// Reference: Correlated random number generation (standard statistical algorithm)
pub fn array_seq_random_correlated(args: &[Value]) -> Result<Value> {
    if args.len() < 4 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_random_correlated(m, s, c, n) requires 4 arguments".to_string(),
        });
    }

    let m = require_number("array_seq_random_correlated", &args[0])? as i32;
    let mut s = require_number("array_seq_random_correlated", &args[1])? as i32;
    let c = require_number("array_seq_random_correlated", &args[2])? as i32;
    let n = require_number("array_seq_random_correlated", &args[3])? as i32;

    if m < 0 || s < 0 || s > m || c < 0 || c > m || n < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_random_correlated() invalid parameters".to_string(),
        });
    }

    let mut result = AudionArray::new();

    for _ in 0..n {
        result.push_auto(Value::Number(s as f64));

        if c > 0 {
            // Decrease s with probability proportional to s
            for j in (m - c + 1..=m).rev() {
                let threshold = (s as f64) / (j as f64);
                if crate::builtins::random_f64() < threshold {
                    s -= 1;
                }
            }

            // Increase s with probability 0.5
            for _ in 0..c {
                if crate::builtins::random_f64() < 0.5 {
                    s += 1;
                }
            }
        }
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Euclidean Rhythms (Bjorklund's Algorithm)
//

/// Generates Euclidean rhythms using Bjorklund's algorithm
/// Arguments: pulses (number of 1s), steps (total length)
/// Example: array_seq_euclidean(5, 8) -> [1,0,1,0,1,0,1,0]
///
/// Reference:
/// - Godfried Toussaint, "The Euclidean Algorithm Generates Traditional Musical Rhythms"
///   Proceedings of BRIDGES: Mathematical Connections in Art, Music and Science, 2005
/// - https://cgm.cs.mcgill.ca/~godfried/publications/banff.pdf
pub fn array_seq_euclidean(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_euclidean(pulses, steps) requires 2 arguments".to_string(),
        });
    }

    let pulses = require_number("array_seq_euclidean", &args[0])? as usize;
    let steps = require_number("array_seq_euclidean", &args[1])? as usize;

    if pulses > steps {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_euclidean() pulses cannot exceed steps".to_string(),
        });
    }

    if steps == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    // Bjorklund's algorithm - proper implementation with grouping
    if pulses == 0 {
        let mut result = AudionArray::new();
        for _ in 0..steps {
            result.push_auto(Value::Number(0.0));
        }
        return Ok(Value::Array(Arc::new(Mutex::new(result))));
    }

    // Build using group concatenation
    let mut front: Vec<Vec<u8>> = (0..pulses).map(|_| vec![1]).collect();
    let mut back: Vec<Vec<u8>> = (0..(steps - pulses)).map(|_| vec![0]).collect();

    loop {
        if back.is_empty() {
            break;
        }

        let count = front.len().min(back.len());

        for i in 0..count {
            front[i].append(&mut back[i]);
        }

        if count < back.len() {
            back.drain(0..count);
        } else {
            back.clear();
        }

        if back.len() <= 1 {
            break;
        }

        // Swap if back is now larger
        if back.len() > front.len() {
            std::mem::swap(&mut front, &mut back);
        }
    }

    // Concatenate remaining
    front.append(&mut back);

    // Flatten to pattern
    let mut result = AudionArray::new();
    for group in front {
        for val in group {
            result.push_auto(Value::Number(val as f64));
        }
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Permutations
//

/// Generates all permutations of the given array in lexicographic order
/// Returns an array of arrays, where each inner array is one permutation
///
/// Reference: Standard lexicographic permutation generation algorithm (Knuth, TAOCP Vol. 4)
pub fn array_seq_permutations(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_permutations() requires an array argument".to_string(),
        });
    }

    let arr = require_array("array_seq_permutations", &args[0])?;
    let locked = arr.lock().unwrap();

    // Extract numbers from array
    let mut nums: Vec<i32> = Vec::new();
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(n) => nums.push(*n as i32),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_permutations() requires array of numbers".to_string(),
                })
            }
        }
    }
    drop(locked);

    if nums.is_empty() {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    nums.sort_unstable(); // Start with sorted order for lexicographic generation

    let mut result = AudionArray::new();

    loop {
        // Add current permutation to result
        let mut perm = AudionArray::new();
        for &num in &nums {
            perm.push_auto(Value::Number(num as f64));
        }
        result.push_auto(Value::Array(Arc::new(Mutex::new(perm))));

        // Find next permutation
        let n = nums.len();
        let mut i = n - 1;

        // Find rightmost element that is smaller than its successor
        while i > 0 && nums[i - 1] >= nums[i] {
            i -= 1;
        }

        if i == 0 {
            break; // Last permutation
        }

        // Find rightmost element greater than nums[i-1]
        let mut j = n - 1;
        while nums[i - 1] >= nums[j] {
            j -= 1;
        }

        // Swap
        nums.swap(i - 1, j);

        // Reverse suffix
        nums[i..].reverse();
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// De Bruijn Sequences
//

/// Generates a de Bruijn sequence of order n
/// A de Bruijn sequence is a cyclic sequence where every possible
/// n-length binary string occurs exactly once as a substring
///
/// Reference:
/// - H. Fredricksen, I. J. Kessler, "An algorithm for generating necklaces of beads in two colors"
///   Discrete Mathematics, 61:181–188, 1986
/// - https://www.combinatorics.org/ojs/index.php/eljc/article/view/v23i1p24
pub fn array_seq_debruijn(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_debruijn(n) requires 1 argument".to_string(),
        });
    }

    let n = require_number("array_seq_debruijn", &args[0])? as usize;

    if n == 0 {
        return Ok(Value::String(String::new()));
    }

    if n > 20 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_debruijn() n too large (max 20)".to_string(),
        });
    }

    let ndbs = 1 << n; // 2^n
    let mut b = vec![0; n + 2];
    b[0] = 1;
    let mut dbs = String::new();

    fn neckbin(
        k: usize,
        l: usize,
        n: usize,
        b: &mut [i32],
        dbs: &mut String,
    ) {
        if k > n {
            if (n % l) == 0 {
                for i in 0..l {
                    dbs.push(if b[i + 1] == 0 { '0' } else { '1' });
                }
            }
        } else {
            b[k] = b[k - l];
            if b[k] == 1 {
                neckbin(k + 1, l, n, b, dbs);
                b[k] = 0;
                neckbin(k + 1, k, n, b, dbs);
            } else {
                neckbin(k + 1, l, n, b, dbs);
            }
        }
    }

    neckbin(1, 1, n, &mut b, &mut dbs);

    Ok(Value::String(dbs))
}

//
// Integer Compositions
//

/// Generates all integer compositions of n
/// A composition is an ordered way to write n as a sum of positive integers
/// Example: 4 = [1,3], [2,2], [3,1], [1,1,2], [1,2,1], [2,1,1], [1,1,1,1]
///
/// Reference: Standard integer composition generation algorithm
/// - https://en.wikipedia.org/wiki/Composition_(combinatorics)
pub fn array_seq_compositions(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_compositions(n) requires 1 argument".to_string(),
        });
    }

    let n = require_number("array_seq_compositions", &args[0])? as i32;

    if n < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_compositions() requires non-negative n".to_string(),
        });
    }

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut parts = vec![0; n as usize];

    fn compose(
        n: i32,
        p: i32,
        m: usize,
        parts: &mut [i32],
        result: &mut AudionArray,
    ) {
        if n == 0 {
            let mut comp = AudionArray::new();
            for i in 0..m {
                comp.push_auto(Value::Number(parts[i] as f64));
            }
            comp.push_auto(Value::Number(p as f64));
            result.push_auto(Value::Array(Arc::new(Mutex::new(comp))));
            return;
        }

        parts[m] = p;
        compose(n - 1, 1, m + 1, parts, result);
        compose(n - 1, p + 1, m, parts, result);
    }

    compose(n - 1, 1, 0, &mut parts, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Integer Partitions
//

/// Generates all integer partitions of n
/// A partition is an unordered way to write n as a sum of positive integers
/// Example: 4 = [1,1,1,1], [1,1,2], [1,3], [2,2], [4]
///
/// Reference:
/// - Jerome Kelleher, "Generating integer partitions"
///   https://jeromekelleher.net/generating-integer-partitions.html
/// - https://jeromekelleher.net/downloads/k06.pdf
pub fn array_seq_partitions(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_partitions(n) requires 1 argument".to_string(),
        });
    }

    let n = require_number("array_seq_partitions", &args[0])? as i32;

    if n < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_partitions() requires non-negative n".to_string(),
        });
    }

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut parts = vec![0; n as usize];

    fn partition(
        n: i32,
        p: i32,
        m: usize,
        parts: &mut [i32],
        result: &mut AudionArray,
    ) {
        if n == 0 {
            let mut part = AudionArray::new();
            for i in 0..m {
                part.push_auto(Value::Number(parts[i] as f64));
            }
            part.push_auto(Value::Number(p as f64));
            result.push_auto(Value::Array(Arc::new(Mutex::new(part))));
            return;
        }

        if n < 0 {
            return;
        }

        parts[m] = p;
        partition(n - p, p, m + 1, parts, result);
        partition(n - 1, p + 1, m, parts, result);
    }

    partition(n - 1, 1, 0, &mut parts, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates all integer partitions of n with allowed parts only
/// Example: array_seq_partitions_allowed(8, [2,3]) generates partitions using only 2s and 3s
///
/// Reference: Restricted partition generation (Kelleher's algorithm variant)
pub fn array_seq_partitions_allowed(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_partitions_allowed(n, allowed_array) requires 2 arguments".to_string(),
        });
    }

    let n = require_number("array_seq_partitions_allowed", &args[0])? as i32;
    let allowed_arr = require_array("array_seq_partitions_allowed", &args[1])?;

    if n < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_partitions_allowed() requires non-negative n".to_string(),
        });
    }

    // Extract allowed parts
    let locked = allowed_arr.lock().unwrap();
    let mut allowed: Vec<i32> = Vec::new();
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(num) => allowed.push(*num as i32),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_partitions_allowed() requires array of numbers".to_string(),
                })
            }
        }
    }
    drop(locked);

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut parts = vec![0; n as usize];

    fn is_allowed(p: i32, allowed: &[i32]) -> bool {
        allowed.iter().any(|&x| x == p)
    }

    fn partition(
        n: i32,
        p: i32,
        m: usize,
        parts: &mut [i32],
        allowed: &[i32],
        result: &mut AudionArray,
    ) {
        if n == 0 {
            if is_allowed(p, allowed) {
                let mut part = AudionArray::new();
                for i in 0..m {
                    part.push_auto(Value::Number(parts[i] as f64));
                }
                part.push_auto(Value::Number(p as f64));
                result.push_auto(Value::Array(Arc::new(Mutex::new(part))));
            }
            return;
        }

        if n < 0 {
            return;
        }

        if is_allowed(p, allowed) {
            parts[m] = p;
            partition(n - p, p, m + 1, parts, allowed, result);
        }
        partition(n - 1, p + 1, m, parts, allowed, result);
    }

    partition(n - 1, 1, 0, &mut parts, &allowed, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates all integer partitions of n into exactly m parts
/// Example: array_seq_partitions_m_parts(5, 2) → [1,4], [2,3]
///
/// Reference: Fixed-length partition generation (Kelleher's algorithm variant)
pub fn array_seq_partitions_m_parts(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_partitions_m_parts(n, m) requires 2 arguments".to_string(),
        });
    }

    let n = require_number("array_seq_partitions_m_parts", &args[0])? as i32;
    let mp = (require_number("array_seq_partitions_m_parts", &args[1])? as i32) - 1;

    if n < 0 || mp < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_partitions_m_parts() requires non-negative n and m".to_string(),
        });
    }

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut parts = vec![0; n as usize];

    fn partition(
        n: i32,
        p: i32,
        m: usize,
        mp: i32,
        parts: &mut [i32],
        result: &mut AudionArray,
    ) {
        if n == 0 {
            if m as i32 == mp {
                let mut part = AudionArray::new();
                for i in 0..m {
                    part.push_auto(Value::Number(parts[i] as f64));
                }
                part.push_auto(Value::Number(p as f64));
                result.push_auto(Value::Array(Arc::new(Mutex::new(part))));
            }
            return;
        }

        if n < 0 {
            return;
        }

        if (m as i32) < mp {
            parts[m] = p;
            partition(n - p, p, m + 1, mp, parts, result);
        }
        partition(n - 1, p + 1, m, mp, parts, result);
    }

    partition(n - 1, 1, 0, mp, &mut parts, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates all integer partitions of n into exactly m parts with allowed parts only
/// Example: array_seq_partitions_allowed_m_parts(8, 3, [2,3]) → partitions of 8 into 3 parts using only 2s and 3s
///
/// Reference: Constrained partition generation (Kelleher's algorithm variant)
pub fn array_seq_partitions_allowed_m_parts(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_partitions_allowed_m_parts(n, m, allowed_array) requires 3 arguments"
                .to_string(),
        });
    }

    let n = require_number("array_seq_partitions_allowed_m_parts", &args[0])? as i32;
    let mp = (require_number("array_seq_partitions_allowed_m_parts", &args[1])? as i32) - 1;
    let allowed_arr = require_array("array_seq_partitions_allowed_m_parts", &args[2])?;

    if n < 0 || mp < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_partitions_allowed_m_parts() requires non-negative n and m"
                .to_string(),
        });
    }

    // Extract allowed parts
    let locked = allowed_arr.lock().unwrap();
    let mut allowed: Vec<i32> = Vec::new();
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(num) => allowed.push(*num as i32),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_partitions_allowed_m_parts() requires array of numbers"
                        .to_string(),
                })
            }
        }
    }
    drop(locked);

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut parts = vec![0; n as usize];

    fn is_allowed(p: i32, allowed: &[i32]) -> bool {
        allowed.iter().any(|&x| x == p)
    }

    fn partition(
        n: i32,
        p: i32,
        m: usize,
        mp: i32,
        parts: &mut [i32],
        allowed: &[i32],
        result: &mut AudionArray,
    ) {
        if n == 0 {
            if m as i32 == mp && is_allowed(p, allowed) {
                let mut part = AudionArray::new();
                for i in 0..m {
                    part.push_auto(Value::Number(parts[i] as f64));
                }
                part.push_auto(Value::Number(p as f64));
                result.push_auto(Value::Array(Arc::new(Mutex::new(part))));
            }
            return;
        }

        if n < 0 {
            return;
        }

        if (m as i32) < mp && is_allowed(p, allowed) {
            parts[m] = p;
            partition(n - p, p, m + 1, mp, parts, allowed, result);
        }
        partition(n - 1, p + 1, m, mp, parts, allowed, result);
    }

    partition(n - 1, 1, 0, mp, &mut parts, &allowed, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Necklaces
//

/// Generates all binary necklaces of length n
/// A necklace is a rotation-invariant binary sequence (circular pattern)
/// Uses the FKM algorithm (Fredricksen, Kessler, Maiorana)
///
/// Reference:
/// - F. Ruskey, J. Sawada, "Fast algorithms to generate necklaces, unlabeled necklaces, and irreducible polynomials over GF(2)"
///   Journal of Algorithms, 37:267-282, 2000
/// - https://www.sciencedirect.com/science/article/abs/pii/S0196677400911088
pub fn array_seq_necklaces(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_necklaces(n) requires 1 argument".to_string(),
        });
    }

    let n = require_number("array_seq_necklaces", &args[0])? as usize;

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    if n > 30 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_necklaces() n too large (max 30)".to_string(),
        });
    }

    let mut result = AudionArray::new();
    let mut b = vec![0; n + 2];
    b[0] = 1;

    fn neckbin(
        k: usize,
        l: usize,
        n: usize,
        b: &mut [i32],
        result: &mut AudionArray,
    ) {
        if k > n {
            if (n % l) == 0 {
                let mut necklace = String::new();
                for i in 1..=n {
                    necklace.push(if b[i] == 0 { '0' } else { '1' });
                }
                result.push_auto(Value::String(necklace));
            }
        } else {
            b[k] = b[k - l];
            if b[k] == 1 {
                neckbin(k + 1, l, n, b, result);
                b[k] = 0;
                neckbin(k + 1, k, n, b, result);
            } else {
                neckbin(k + 1, l, n, b, result);
            }
        }
    }

    neckbin(1, 1, n, &mut b, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates binary necklaces of length n with allowed run lengths (part sizes)
/// Example: array_seq_necklaces_allowed(8, [2,3]) generates necklaces with runs of 0s of length 2 or 3
///
/// Reference: FKM algorithm variant with constraints
pub fn array_seq_necklaces_allowed(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_necklaces_allowed(n, allowed_array) requires 2 arguments".to_string(),
        });
    }

    let n = require_number("array_seq_necklaces_allowed", &args[0])? as usize;
    let allowed_arr = require_array("array_seq_necklaces_allowed", &args[1])?;

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    if n > 30 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_necklaces_allowed() n too large (max 30)".to_string(),
        });
    }

    // Extract allowed parts
    let locked = allowed_arr.lock().unwrap();
    let mut allowed: Vec<i32> = Vec::new();
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(num) => allowed.push(*num as i32),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_necklaces_allowed() requires array of numbers".to_string(),
                })
            }
        }
    }
    drop(locked);

    let mut result = AudionArray::new();
    let mut b = vec![0; n + 2];
    b[0] = 1;

    fn is_allowed(p: i32, allowed: &[i32]) -> bool {
        allowed.iter().any(|&x| x == p)
    }

    fn neckbin(
        k: usize,
        l: usize,
        p: i32,
        n: usize,
        b: &mut [i32],
        allowed: &[i32],
        result: &mut AudionArray,
    ) {
        if k > n {
            if (n % l) == 0 && is_allowed(p, allowed) && p <= n as i32 {
                let mut necklace = String::new();
                for i in 1..=n {
                    necklace.push(if b[i] == 0 { '0' } else { '1' });
                }
                result.push_auto(Value::String(necklace));
            }
        } else {
            b[k] = b[k - l];
            if b[k] == 1 {
                if is_allowed(p, allowed) || k == 1 {
                    neckbin(k + 1, l, 1, n, b, allowed, result);
                }
                b[k] = 0;
                neckbin(k + 1, k, p + 1, n, b, allowed, result);
            } else {
                neckbin(k + 1, l, p + 1, n, b, allowed, result);
            }
        }
    }

    neckbin(1, 1, 1, n, &mut b, &allowed, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates all binary necklaces of length n with exactly m ones
/// Example: array_seq_necklaces_m_ones(8, 3) generates all 8-bit necklaces with exactly 3 ones
///
/// Reference:
/// - F. Ruskey, J. Sawada, "An efficient algorithm for generating necklaces with fixed density"
///   SIAM Journal on Computing, 29(2):671-684, 1999
/// - https://epubs.siam.org/doi/10.1137/S0097539798344112
pub fn array_seq_necklaces_m_ones(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_necklaces_m_ones(n, m) requires 2 arguments".to_string(),
        });
    }

    let n = require_number("array_seq_necklaces_m_ones", &args[0])? as usize;
    let n1 = require_number("array_seq_necklaces_m_ones", &args[1])? as i32;

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    if n > 30 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_necklaces_m_ones() n too large (max 30)".to_string(),
        });
    }

    let mut result = AudionArray::new();
    let mut b = vec![0; n + 2];
    b[0] = 1;

    fn neckbin(
        k: usize,
        l: usize,
        m: i32,
        n: usize,
        n1: i32,
        b: &mut [i32],
        result: &mut AudionArray,
    ) {
        if k > n {
            if (n % l) == 0 && m == n1 {
                let mut necklace = String::new();
                for i in 1..=n {
                    necklace.push(if b[i] == 0 { '0' } else { '1' });
                }
                result.push_auto(Value::String(necklace));
            }
        } else {
            b[k] = b[k - l];
            if b[k] == 1 {
                neckbin(k + 1, l, m + 1, n, n1, b, result);
                b[k] = 0;
                neckbin(k + 1, k, m, n, n1, b, result);
            } else {
                neckbin(k + 1, l, m, n, n1, b, result);
            }
        }
    }

    neckbin(1, 1, 0, n, n1, &mut b, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates binary necklaces of length n with exactly m ones and allowed run lengths
/// Combines both constraints
///
/// Reference: FKM algorithm with combined constraints (fixed density + allowed run lengths)
pub fn array_seq_necklaces_allowed_m_ones(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_necklaces_allowed_m_ones(n, m, allowed_array) requires 3 arguments"
                .to_string(),
        });
    }

    let n = require_number("array_seq_necklaces_allowed_m_ones", &args[0])? as usize;
    let n1 = require_number("array_seq_necklaces_allowed_m_ones", &args[1])? as i32;
    let allowed_arr = require_array("array_seq_necklaces_allowed_m_ones", &args[2])?;

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    if n > 30 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_necklaces_allowed_m_ones() n too large (max 30)".to_string(),
        });
    }

    // Extract allowed parts
    let locked = allowed_arr.lock().unwrap();
    let mut allowed: Vec<i32> = Vec::new();
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(num) => allowed.push(*num as i32),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_necklaces_allowed_m_ones() requires array of numbers"
                        .to_string(),
                })
            }
        }
    }
    drop(locked);

    let mut result = AudionArray::new();
    let mut b = vec![0; n + 2];
    b[0] = 1;

    fn is_allowed(p: i32, allowed: &[i32]) -> bool {
        allowed.iter().any(|&x| x == p)
    }

    fn neckbin(
        k: usize,
        l: usize,
        m: i32,
        p: i32,
        n: usize,
        n1: i32,
        b: &mut [i32],
        allowed: &[i32],
        result: &mut AudionArray,
    ) {
        if k > n {
            if (n % l) == 0 && is_allowed(p, allowed) && p <= n as i32 && m == n1 {
                let mut necklace = String::new();
                for i in 1..=n {
                    necklace.push(if b[i] == 0 { '0' } else { '1' });
                }
                result.push_auto(Value::String(necklace));
            }
        } else {
            b[k] = b[k - l];
            if b[k] == 1 {
                if is_allowed(p, allowed) || k == 1 {
                    neckbin(k + 1, l, m + 1, 1, n, n1, b, allowed, result);
                }
                b[k] = 0;
                neckbin(k + 1, k, m, p + 1, n, n1, b, allowed, result);
            } else {
                neckbin(k + 1, l, m, p + 1, n, n1, b, allowed, result);
            }
        }
    }

    neckbin(1, 1, 0, 1, n, n1, &mut b, &allowed, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Markov Chains
//

/// Generates a sequence using a Markov chain transition matrix
/// Args: matrix (2D array), start_state (number), count (how many values to generate)
///
/// Matrix format: matrix[i][j] = probability of transitioning from state i to state j
/// Each row should sum to 1.0 (will work with unnormalized but less predictable)
///
/// Returns an array of state numbers
///
/// Reference: Standard Markov chain sequence generation
/// - https://en.wikipedia.org/wiki/Markov_chain
pub fn array_seq_markov(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_markov(matrix, start_state, count) requires 3 arguments".to_string(),
        });
    }

    let matrix_arr = require_array("array_seq_markov", &args[0])?;
    let mut s = require_number("array_seq_markov", &args[1])? as usize;
    let n = require_number("array_seq_markov", &args[2])? as usize;

    // Parse matrix
    let matrix_locked = matrix_arr.lock().unwrap();
    let mut matrix: Vec<Vec<f64>> = Vec::new();

    for (_key, row_val) in matrix_locked.entries() {
        let row_arr = match row_val {
            Value::Array(arr) => arr.clone(),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_markov() matrix must be a 2D array".to_string(),
                })
            }
        };

        let row_locked = row_arr.lock().unwrap();
        let mut row: Vec<f64> = Vec::new();

        for (_k, val) in row_locked.entries() {
            match val {
                Value::Number(num) => row.push(*num),
                _ => {
                    return Err(AudionError::RuntimeError {
                        msg: "array_seq_markov() matrix must contain only numbers".to_string(),
                    })
                }
            }
        }
        drop(row_locked);
        matrix.push(row);
    }
    drop(matrix_locked);

    let ns = matrix.len(); // number of states

    if ns == 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_markov() matrix cannot be empty".to_string(),
        });
    }

    // Validate matrix is square and start state is valid
    for row in &matrix {
        if row.len() != ns {
            return Err(AudionError::RuntimeError {
                msg: "array_seq_markov() matrix must be square".to_string(),
            });
        }
    }

    if s >= ns {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "array_seq_markov() start_state {} out of range (0-{})",
                s,
                ns - 1
            ),
        });
    }

    let mut result = AudionArray::new();

    for _ in 0..n {
        result.push_auto(Value::Number(s as f64));

        // Choose next state based on transition probabilities
        let u = crate::builtins::random_f64();
        let mut x = 0.0;

        for (j, &prob) in matrix[s].iter().enumerate() {
            x += prob;
            if u < x {
                s = j;
                break;
            }
        }
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Compositions with Allowed Parts
//

/// Generates all integer compositions of n with allowed parts only
/// Example: array_seq_compositions_allowed(5, [1,3]) generates compositions of 5 using only 1s and 3s
///
/// Reference: Constrained composition generation (standard combinatorial algorithm)
pub fn array_seq_compositions_allowed(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_compositions_allowed(n, allowed_array) requires 2 arguments".to_string(),
        });
    }

    let n = require_number("array_seq_compositions_allowed", &args[0])? as i32;
    let allowed_arr = require_array("array_seq_compositions_allowed", &args[1])?;

    if n < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_compositions_allowed() requires non-negative n".to_string(),
        });
    }

    // Extract allowed parts
    let locked = allowed_arr.lock().unwrap();
    let mut allowed: Vec<i32> = Vec::new();
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(num) => allowed.push(*num as i32),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_compositions_allowed() requires array of numbers".to_string(),
                })
            }
        }
    }
    drop(locked);

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut parts = vec![0; n as usize];

    fn is_allowed(p: i32, allowed: &[i32]) -> bool {
        allowed.iter().any(|&x| x == p)
    }

    fn compose(
        n: i32,
        p: i32,
        m: usize,
        parts: &mut [i32],
        allowed: &[i32],
        result: &mut AudionArray,
    ) {
        if n == 0 {
            if is_allowed(p, allowed) {
                let mut comp = AudionArray::new();
                for i in 0..m {
                    comp.push_auto(Value::Number(parts[i] as f64));
                }
                comp.push_auto(Value::Number(p as f64));
                result.push_auto(Value::Array(Arc::new(Mutex::new(comp))));
            }
            return;
        }

        if is_allowed(p, allowed) {
            parts[m] = p;
            compose(n - 1, 1, m + 1, parts, allowed, result);
        }
        compose(n - 1, p + 1, m, parts, allowed, result);
    }

    compose(n - 1, 1, 0, &mut parts, &allowed, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates all integer compositions of n into exactly m parts
/// Example: array_seq_compositions_m_parts(5, 2) → [1,4], [2,3], [3,2], [4,1]
///
/// Reference: Fixed-length composition generation (standard combinatorial algorithm)
pub fn array_seq_compositions_m_parts(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_compositions_m_parts(n, m) requires 2 arguments".to_string(),
        });
    }

    let n = require_number("array_seq_compositions_m_parts", &args[0])? as i32;
    let mp = (require_number("array_seq_compositions_m_parts", &args[1])? as i32) - 1;

    if n < 0 || mp < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_compositions_m_parts() requires non-negative n and m".to_string(),
        });
    }

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut parts = vec![0; n as usize];

    fn compose(
        n: i32,
        p: i32,
        m: usize,
        mp: i32,
        parts: &mut [i32],
        result: &mut AudionArray,
    ) {
        if n == 0 {
            if m as i32 == mp {
                let mut comp = AudionArray::new();
                for i in 0..m {
                    comp.push_auto(Value::Number(parts[i] as f64));
                }
                comp.push_auto(Value::Number(p as f64));
                result.push_auto(Value::Array(Arc::new(Mutex::new(comp))));
            }
            return;
        }

        if (m as i32) < mp {
            parts[m] = p;
            compose(n - 1, 1, m + 1, mp, parts, result);
        }
        compose(n - 1, p + 1, m, mp, parts, result);
    }

    compose(n - 1, 1, 0, mp, &mut parts, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates all integer compositions of n into exactly m parts with allowed parts only
/// Example: array_seq_compositions_allowed_m_parts(8, 3, [2,3]) → compositions of 8 into 3 parts using only 2s and 3s
///
/// Reference: Constrained composition generation with combined constraints
pub fn array_seq_compositions_allowed_m_parts(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_compositions_allowed_m_parts(n, m, allowed_array) requires 3 arguments"
                .to_string(),
        });
    }

    let n = require_number("array_seq_compositions_allowed_m_parts", &args[0])? as i32;
    let mp = (require_number("array_seq_compositions_allowed_m_parts", &args[1])? as i32) - 1;
    let allowed_arr = require_array("array_seq_compositions_allowed_m_parts", &args[2])?;

    if n < 0 || mp < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_compositions_allowed_m_parts() requires non-negative n and m"
                .to_string(),
        });
    }

    // Extract allowed parts
    let locked = allowed_arr.lock().unwrap();
    let mut allowed: Vec<i32> = Vec::new();
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(num) => allowed.push(*num as i32),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_compositions_allowed_m_parts() requires array of numbers"
                        .to_string(),
                })
            }
        }
    }
    drop(locked);

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut parts = vec![0; n as usize];

    fn is_allowed(p: i32, allowed: &[i32]) -> bool {
        allowed.iter().any(|&x| x == p)
    }

    fn compose(
        n: i32,
        p: i32,
        m: usize,
        mp: i32,
        parts: &mut [i32],
        allowed: &[i32],
        result: &mut AudionArray,
    ) {
        if n == 0 {
            if m as i32 == mp && is_allowed(p, allowed) {
                let mut comp = AudionArray::new();
                for i in 0..m {
                    comp.push_auto(Value::Number(parts[i] as f64));
                }
                comp.push_auto(Value::Number(p as f64));
                result.push_auto(Value::Array(Arc::new(Mutex::new(comp))));
            }
            return;
        }

        if (m as i32) < mp && is_allowed(p, allowed) {
            parts[m] = p;
            compose(n - 1, 1, m + 1, mp, parts, allowed, result);
        }
        compose(n - 1, p + 1, m, mp, parts, allowed, result);
    }

    compose(n - 1, 1, 0, mp, &mut parts, &allowed, &mut result);

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Random Compositions
//

/// Generates a random composition of n
/// Each decision point has 50% chance to continue or split
/// Returns an array of positive integers that sum to n
///
/// Reference: Random composition generation (unbiased probabilistic algorithm)
pub fn array_seq_composition_random(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_composition_random(n) requires 1 argument".to_string(),
        });
    }

    let n = require_number("array_seq_composition_random", &args[0])? as i32;

    if n < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_composition_random() requires non-negative n".to_string(),
        });
    }

    if n == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut result = AudionArray::new();
    let mut p = 1;

    for i in 1..n {
        if crate::builtins::random_f64() < 0.5 {
            p += 1;
        } else {
            result.push_auto(Value::Number(p as f64));
            p = 1;
        }
    }

    result.push_auto(Value::Number(p as f64));

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Generates a random composition of n into exactly m parts
/// Uses probability-based algorithm to ensure exactly m parts
/// Returns an array of m positive integers that sum to n
///
/// Reference: Random fixed-length composition generation (unbiased probabilistic algorithm)
pub fn array_seq_composition_random_m_parts(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_composition_random_m_parts(n, m) requires 2 arguments".to_string(),
        });
    }

    let n = require_number("array_seq_composition_random_m_parts", &args[0])? as i32;
    let m = require_number("array_seq_composition_random_m_parts", &args[1])? as i32;

    if n < 0 || m < 0 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_composition_random_m_parts() requires non-negative n and m"
                .to_string(),
        });
    }

    if n == 0 || m == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    if m > n {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_composition_random_m_parts() m cannot exceed n".to_string(),
        });
    }

    let mut result = AudionArray::new();
    let mut mp = m - 1; // remaining parts to emit
    let mut np = n - 1; // remaining positions
    let mut j = 1; // current part value

    while mp > 0 {
        let p = (mp as f64) / (np as f64);
        if crate::builtins::random_f64() < p {
            result.push_auto(Value::Number(j as f64));
            mp -= 1;
            j = 1;
        } else {
            j += 1;
        }
        np -= 1;
    }

    result.push_auto(Value::Number((j + np) as f64));

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

//
// Continued Fractions
//

/// Calculates a continued fraction convergent
/// Takes an array of continued fraction terms and returns [numerator, denominator]
/// Example: array_seq_cf_convergent([1,2,2,2,2]) → approximation of sqrt(2)
///
/// Reference:
/// - Standard continued fraction convergent algorithm
/// - https://en.wikipedia.org/wiki/Continued_fraction
/// - https://cp-algorithms.com/algebra/continued-fractions.html
pub fn array_seq_cf_convergent(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_cf_convergent(terms_array) requires 1 argument".to_string(),
        });
    }

    let terms_arr = require_array("array_seq_cf_convergent", &args[0])?;
    let locked = terms_arr.lock().unwrap();

    let mut terms: Vec<u64> = Vec::new();
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(n) => {
                if *n < 0.0 {
                    return Err(AudionError::RuntimeError {
                        msg: "array_seq_cf_convergent() requires non-negative terms".to_string(),
                    });
                }
                terms.push(*n as u64);
            }
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: "array_seq_cf_convergent() requires array of numbers".to_string(),
                })
            }
        }
    }
    drop(locked);

    if terms.is_empty() {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let mut p0: u64 = 0;
    let mut p1: u64 = 1;
    let mut q0: u64 = 1;
    let mut q1: u64 = 0;

    for &a in &terms {
        let p2 = a * p1 + p0;
        let q2 = a * q1 + q0;
        p0 = p1;
        p1 = p2;
        q0 = q1;
        q1 = q2;
    }

    let mut result = AudionArray::new();
    result.push_auto(Value::Number(p1 as f64));
    result.push_auto(Value::Number(q1 as f64));

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Calculates the continued fraction representation of sqrt(n)
/// Returns a string with the format "a0 (a1 a2 ... ak)" where the part in parentheses repeats
/// Example: array_seq_cf_sqrt(7) → "2 (1 1 1 4)"
///
/// Reference:
/// - M. Beceanu, "Period of the Continued Fraction of √n"
///   https://web.math.princeton.edu/mathlab/jr02fall/Periodicity/mariusjp.pdf
/// - https://en.wikipedia.org/wiki/Periodic_continued_fraction
pub fn array_seq_cf_sqrt(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_cf_sqrt(n) requires 1 argument".to_string(),
        });
    }

    let n = require_number("array_seq_cf_sqrt", &args[0])? as u64;

    let a0 = (n as f64).sqrt() as u64;

    // Check if n is a perfect square
    if a0 * a0 == n {
        return Ok(Value::String(format!("{} ( )", a0)));
    }

    let mut result = format!("{} (", a0);
    let mut a_big = 0u64;
    let mut b_big = 1u64;
    let mut a = a0;

    loop {
        a_big = b_big * a - a_big;
        b_big = (n - a_big * a_big) / b_big;
        a = (a0 + a_big) / b_big;
        result.push_str(&format!(" {}", a));

        if a == 2 * a0 {
            break;
        }
    }

    result.push_str(" )");

    Ok(Value::String(result))
}

//
// Christoffel Words
//

/// Generates Christoffel words (binary sequences related to rational approximations)
/// Args: type ("upper" or "lower"), p (numerator), q (denominator), optional n (length, default p+q)
/// Example: array_seq_christoffel("upper", 3, 5, 8) generates upper Christoffel word for 3/5
///
/// Reference:
/// - J. Berstel, A. Lauve, C. Reutenauer, F. Saliola, "Combinatorics on Words: Christoffel Words and Repetition in Words"
///   CRM Monograph Series, 2008
/// - http://www-igm.univ-mlv.fr/~berstel/Articles/2008wordsbookMtlUltimate.pdf
pub fn array_seq_christoffel(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_christoffel(type, p, q, [n]) requires 3-4 arguments".to_string(),
        });
    }

    let type_str = require_string("array_seq_christoffel", &args[0])?;
    let p = require_number("array_seq_christoffel", &args[1])? as u64;
    let q = require_number("array_seq_christoffel", &args[2])? as u64;
    let n = if args.len() >= 4 {
        require_number("array_seq_christoffel", &args[3])? as u64
    } else {
        p + q
    };

    let is_upper = type_str.to_lowercase().starts_with('u');

    let mut result = String::new();
    let mut i = 0;

    loop {
        result.push(if is_upper { '1' } else { '0' });
        i += 1;

        let mut a = p;
        let mut b = q;

        while a != b && i < n {
            if a > b {
                result.push('1');
                b += q;
            } else {
                result.push('0');
                a += p;
            }
            i += 1;
        }

        if a == b && i < n {
            result.push(if is_upper { '0' } else { '1' });
            i += 1;
        }

        if i >= n {
            break;
        }
    }

    Ok(Value::String(result))
}

//
// Paper Folding Sequences
//

/// Generates paper folding sequences
/// Args: n (number of terms, typically 2^k-1), m (number of bits), f (function number 0 to 2^m-1)
/// Returns a binary string representing the folding pattern
///
/// Reference:
/// - Regular paperfolding sequence (dragon curve)
/// - https://en.wikipedia.org/wiki/Regular_paperfolding_sequence
/// - https://oeis.org/A014577
pub fn array_seq_paper_folding(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_paper_folding(n, m, f) requires 3 arguments".to_string(),
        });
    }

    let n = require_number("array_seq_paper_folding", &args[0])? as u32;
    let m = require_number("array_seq_paper_folding", &args[1])? as u32;
    let f = require_number("array_seq_paper_folding", &args[2])? as u32;

    if m > 31 {
        return Err(AudionError::RuntimeError {
            msg: "array_seq_paper_folding() m too large (max 31)".to_string(),
        });
    }

    let mut result = String::new();

    for i in 1..=n {
        let (k, j) = oddeven(i);
        let k_mod = k % m;
        let mut b = if (f & (1 << k_mod)) != 0 { 1 } else { 0 };
        if (2 * j + 1) % 4 > 1 {
            b = 1 - b;
        }
        result.push(if b == 1 { '1' } else { '0' });
    }

    Ok(Value::String(result))
}

/// Helper function: finds a and b such that n = 2^a * (2*b+1)
fn oddeven(n: u32) -> (u32, u32) {
    // two's complement of n = -n
    let l = n & n.wrapping_neg(); // this is 2^a
    let b = (n / l - 1) / 2;
    let mut k = 0;
    let mut temp = l;
    while temp > 1 {
        k += 1;
        temp >>= 1;
    }
    (k, b)
}
