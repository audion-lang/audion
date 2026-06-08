use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::{AudionError, Result};
use crate::value::{AudionArray, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_number(fn_name: &str, val: &Value) -> Result<f64> {
    match val {
        Value::Number(n) => Ok(*n),
        _ => Err(AudionError::RuntimeError {
            msg: format!("{} requires a number", fn_name),
        }),
    }
}

fn require_array(fn_name: &str, val: &Value) -> Result<Arc<Mutex<AudionArray>>> {
    match val {
        Value::Array(arr) => Ok(arr.clone()),
        _ => Err(AudionError::RuntimeError {
            msg: format!("{} requires an array", fn_name),
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

/// Extract a flat Vec<f64> from an AudionArray.
fn array_to_f64_vec(fn_name: &str, arr: &Arc<Mutex<AudionArray>>) -> Result<Vec<f64>> {
    let locked = arr.lock().unwrap();
    let mut out = Vec::with_capacity(locked.len());
    for (_key, val) in locked.entries() {
        match val {
            Value::Number(n) => out.push(*n),
            _ => {
                return Err(AudionError::RuntimeError {
                    msg: format!("{}() array must contain only numbers", fn_name),
                })
            }
        }
    }
    Ok(out)
}

/// Helper to get array element by integer index.
fn array_get_num(arr: &AudionArray, idx: i64) -> Option<f64> {
    let key = Value::Number(idx as f64);
    match arr.get(&key) {
        Some(Value::Number(n)) => Some(*n),
        _ => None,
    }
}

/// Helper to get array element by integer index, returning an Arc<Mutex<AudionArray>>.
fn array_get_arr(arr: &AudionArray, idx: i64) -> Option<Arc<Mutex<AudionArray>>> {
    let key = Value::Number(idx as f64);
    match arr.get(&key) {
        Some(Value::Array(a)) => Some(a.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Internal: build transition table for a single order
// ---------------------------------------------------------------------------

fn build_transitions_for_order(
    notes: &[f64],
    order: usize,
) -> HashMap<String, HashMap<u64, (f64, u64)>> {
    let mut transitions: HashMap<String, HashMap<u64, (f64, u64)>> = HashMap::new();

    for i in 0..notes.len().saturating_sub(order) {
        let context = &notes[i..i + order];
        let next = notes[i + order];
        let key = context_key(context);

        let entry = transitions.entry(key).or_default();
        let bits = next.to_bits();
        entry
            .entry(bits)
            .and_modify(|(_, c)| *c += 1)
            .or_insert((next, 1));
    }

    transitions
}

/// Convert a HashMap transition table into an AudionArray (sorted for determinism).
fn transitions_to_audion_array(
    transitions: &HashMap<String, HashMap<u64, (f64, u64)>>,
) -> AudionArray {
    let mut sorted_keys: Vec<&String> = transitions.keys().collect();
    sorted_keys.sort();

    let mut trans_table = AudionArray::new();
    for ctx_key in &sorted_keys {
        let next_map = &transitions[*ctx_key];
        let mut pairs = AudionArray::new();
        let mut sorted_pairs: Vec<_> = next_map.values().collect();
        sorted_pairs
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for (val, count) in sorted_pairs {
            let mut pair = AudionArray::new();
            pair.push_auto(Value::Number(*val));
            pair.push_auto(Value::Number(*count as f64));
            pairs.push_auto(Value::Array(Arc::new(Mutex::new(pair))));
        }
        trans_table.set(
            Value::String(ctx_key.to_string()),
            Value::Array(Arc::new(Mutex::new(pairs))),
        );
    }
    trans_table
}

// ---------------------------------------------------------------------------
// ml_markov_train(notes, order)
//
// Train a Markov model with backoff — builds transition tables for ALL orders
// from 1 up to `order`. This allows the generator to fall back to shorter
// contexts when a high-order context has no match (Katz backoff).
//
// Model format:
//   [0] = max order (number)
//   [1] = transitions for order N (highest)
//   [2] = transitions for order N-1
//   ...
//   [N] = transitions for order 1 (always has matches)
//
// Each transition table is keyed by context string, values are [[note, count], ...].
// ---------------------------------------------------------------------------

pub fn builtin_ml_markov_train(args: &[Value]) -> Result<Value> {
    require_at_least("ml_markov_train", args, 2)?;
    let notes_arr = require_array("ml_markov_train", &args[0])?;
    let order = require_number("ml_markov_train", &args[1])? as usize;

    if order == 0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_markov_train() order must be >= 1".to_string(),
        });
    }

    let notes = array_to_f64_vec("ml_markov_train", &notes_arr)?;

    if notes.len() <= order {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "ml_markov_train() needs more than {} notes for order {}",
                order, order
            ),
        });
    }

    let mut model = AudionArray::new();
    model.push_auto(Value::Number(order as f64));

    // Build transition tables for all orders from `order` down to 1
    for o in (1..=order).rev() {
        let transitions = build_transitions_for_order(&notes, o);
        let table = transitions_to_audion_array(&transitions);
        model.push_auto(Value::Array(Arc::new(Mutex::new(table))));
    }

    Ok(Value::Array(Arc::new(Mutex::new(model))))
}

fn context_key(context: &[f64]) -> String {
    context
        .iter()
        .map(|n| {
            if *n == (*n as i64) as f64 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

// ---------------------------------------------------------------------------
// ml_markov_generate(model, length, [start])
//
// Generate a sequence from a trained Markov model using Katz backoff.
// On each step, tries the highest order first. If no match, backs off to
// order-1, order-2, etc. down to order 1. Order 1 always has a match
// (every note in the corpus appears as a context).
//
// If start is omitted, picks a random starting context.
// Returns an array of values (the actual notes, not state indices).
// ---------------------------------------------------------------------------

pub fn builtin_ml_markov_generate(args: &[Value]) -> Result<Value> {
    require_at_least("ml_markov_generate", args, 2)?;
    let model_arr = require_array("ml_markov_generate", &args[0])?;
    let length = require_number("ml_markov_generate", &args[1])? as usize;

    // Parse model
    let model_locked = model_arr.lock().unwrap();
    let max_order = match array_get_num(&model_locked, 0) {
        Some(n) => n as usize,
        None => {
            return Err(AudionError::RuntimeError {
                msg: "ml_markov_generate() invalid model (missing order)".to_string(),
            })
        }
    };

    // Collect all transition tables (index 1 = highest order, index max_order = order 1)
    let mut trans_tables: Vec<Arc<Mutex<AudionArray>>> = Vec::new();
    for i in 1..=(max_order as i64) {
        match array_get_arr(&model_locked, i) {
            Some(a) => trans_tables.push(a),
            None => {
                return Err(AudionError::RuntimeError {
                    msg: format!(
                        "ml_markov_generate() invalid model (missing transition table {})",
                        i
                    ),
                })
            }
        }
    }
    drop(model_locked);

    // Collect all context keys from the highest-order table for random start
    let top_table = trans_tables[0].lock().unwrap();
    let all_keys: Vec<String> = top_table
        .entries()
        .iter()
        .filter_map(|(k, _)| match k {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    drop(top_table);

    if all_keys.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "ml_markov_generate() model has no transitions".to_string(),
        });
    }

    // Determine starting context
    let start_context: Vec<f64> = if args.len() >= 3 {
        let start_arr = require_array("ml_markov_generate", &args[2])?;
        let ctx = array_to_f64_vec("ml_markov_generate", &start_arr)?;
        if ctx.len() != max_order {
            return Err(AudionError::RuntimeError {
                msg: format!(
                    "ml_markov_generate() start context must have {} elements (model order)",
                    max_order
                ),
            });
        }
        ctx
    } else {
        let idx = (crate::builtins::random_f64() * all_keys.len() as f64) as usize;
        let idx = idx.min(all_keys.len() - 1);
        parse_context_key(&all_keys[idx])
    };

    let mut sequence: Vec<f64> = start_context.clone();

    for _ in 0..length {
        let seq_len = sequence.len();
        let mut found = false;

        // Katz backoff: try order max_order, then max_order-1, ..., then 1
        for (table_idx, table_arc) in trans_tables.iter().enumerate() {
            let current_order = max_order - table_idx; // table_idx 0 = max_order, 1 = max_order-1, ...
            if seq_len < current_order {
                continue; // not enough history for this order
            }

            let ctx = &sequence[seq_len - current_order..];
            let key = context_key(ctx);
            let lookup = Value::String(key);

            let table_locked = table_arc.lock().unwrap();
            if let Some(Value::Array(pairs_arr)) = table_locked.get(&lookup) {
                let next = weighted_pick_from_pairs(&pairs_arr.lock().unwrap())?;
                sequence.push(next);
                found = true;
                break;
            }
        }

        if !found {
            // Shouldn't happen if corpus has > 1 note (order-1 always matches),
            // but just in case: pick a random note from the order-1 table
            let table1 = trans_tables.last().unwrap().lock().unwrap();
            let entries = table1.entries();
            let idx = (crate::builtins::random_f64() * entries.len() as f64) as usize;
            let idx = idx.min(entries.len().saturating_sub(1));
            if let (Value::String(k), _) = &entries[idx] {
                let note = parse_context_key(k);
                sequence.push(*note.last().unwrap_or(&0.0));
            }
        }
    }

    // Return only the generated part (after the seed context)
    let mut result = AudionArray::new();
    for &val in &sequence[max_order..] {
        result.push_auto(Value::Number(val));
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

/// Pick a value from [[value, count], ...] pairs weighted by count.
fn weighted_pick_from_pairs(pairs: &AudionArray) -> Result<f64> {
    let mut total: f64 = 0.0;
    let mut items: Vec<(f64, f64)> = Vec::new();

    for (_k, v) in pairs.entries() {
        if let Value::Array(pair) = v {
            let pair_locked = pair.lock().unwrap();
            let val = match array_get_num(&pair_locked, 0) {
                Some(n) => n,
                None => continue,
            };
            let count = match array_get_num(&pair_locked, 1) {
                Some(n) => n,
                None => continue,
            };
            total += count;
            items.push((val, count));
        }
    }

    if total == 0.0 || items.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "ml_markov_generate() empty transition".to_string(),
        });
    }

    // Sort by value for deterministic ordering regardless of HashMap iteration order
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let u = crate::builtins::random_f64() * total;
    let mut acc = 0.0;
    for (val, count) in &items {
        acc += count;
        if u < acc {
            return Ok(*val);
        }
    }

    Ok(items.last().unwrap().0)
}

fn parse_context_key(key: &str) -> Vec<f64> {
    key.split(',')
        .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
        .collect()
}

// ---------------------------------------------------------------------------
// ml_markov_next(model, context)
//
// Given a trained model and the current context (last N notes),
// return the probability distribution for the next note.
// Uses backoff: tries full context first, then shorter.
// Returns array of [value, probability] pairs, sorted by probability desc.
// ---------------------------------------------------------------------------

pub fn builtin_ml_markov_next(args: &[Value]) -> Result<Value> {
    require_at_least("ml_markov_next", args, 2)?;
    let model_arr = require_array("ml_markov_next", &args[0])?;
    let context_arr = require_array("ml_markov_next", &args[1])?;

    let model_locked = model_arr.lock().unwrap();
    let max_order = match array_get_num(&model_locked, 0) {
        Some(n) => n as usize,
        None => {
            return Err(AudionError::RuntimeError {
                msg: "ml_markov_next() invalid model".to_string(),
            })
        }
    };

    let context = array_to_f64_vec("ml_markov_next", &context_arr)?;
    if context.len() != max_order {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "ml_markov_next() context must have {} elements (model order)",
                max_order
            ),
        });
    }

    // Try each order from highest to lowest
    for table_idx in 0..max_order {
        let current_order = max_order - table_idx;
        let trans_arr = match array_get_arr(&model_locked, (table_idx + 1) as i64) {
            Some(a) => a,
            None => continue,
        };

        let ctx = &context[context.len() - current_order..];
        let key = context_key(ctx);
        let lookup = Value::String(key);
        let trans_locked = trans_arr.lock().unwrap();

        if let Some(Value::Array(pairs_arr)) = trans_locked.get(&lookup) {
            let pairs_locked = pairs_arr.lock().unwrap();
            let mut total: f64 = 0.0;
            let mut items: Vec<(f64, f64)> = Vec::new();

            for (_k, v) in pairs_locked.entries() {
                if let Value::Array(pair) = v {
                    let pl = pair.lock().unwrap();
                    let val = match array_get_num(&pl, 0) {
                        Some(n) => n,
                        None => continue,
                    };
                    let count = match array_get_num(&pl, 1) {
                        Some(n) => n,
                        None => continue,
                    };
                    total += count;
                    items.push((val, count));
                }
            }

            // Sort by probability descending
            items
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let mut result = AudionArray::new();
            for (val, count) in &items {
                let mut pair = AudionArray::new();
                pair.push_auto(Value::Number(*val));
                pair.push_auto(Value::Number(if total > 0.0 {
                    count / total
                } else {
                    0.0
                }));
                result.push_auto(Value::Array(Arc::new(Mutex::new(pair))));
            }

            drop(model_locked);
            return Ok(Value::Array(Arc::new(Mutex::new(result))));
        }
    }

    drop(model_locked);
    // No match at any order — return empty
    Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))))
}

// ---------------------------------------------------------------------------
// ml_weighted_choice(values, weights)
//
// Pick one item from values using weights as probabilities.
// ---------------------------------------------------------------------------

pub fn builtin_ml_weighted_choice(args: &[Value]) -> Result<Value> {
    require_at_least("ml_weighted_choice", args, 2)?;
    let values_arr = require_array("ml_weighted_choice", &args[0])?;
    let weights_arr = require_array("ml_weighted_choice", &args[1])?;

    let values_locked = values_arr.lock().unwrap();
    let weights = array_to_f64_vec("ml_weighted_choice", &weights_arr)?;

    let items: Vec<Value> = values_locked
        .entries()
        .iter()
        .map(|(_, v)| v.clone())
        .collect();

    if items.len() != weights.len() {
        return Err(AudionError::RuntimeError {
            msg: format!(
                "ml_weighted_choice() values ({}) and weights ({}) must have same length",
                items.len(),
                weights.len()
            ),
        });
    }

    if items.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "ml_weighted_choice() arrays cannot be empty".to_string(),
        });
    }

    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_weighted_choice() weights must sum to > 0".to_string(),
        });
    }

    let u = crate::builtins::random_f64() * total;
    let mut acc = 0.0;
    for (i, w) in weights.iter().enumerate() {
        acc += w;
        if u < acc {
            return Ok(items[i].clone());
        }
    }

    Ok(items.last().unwrap().clone())
}

// ---------------------------------------------------------------------------
// ml_softmax(values, [temperature])
//
// Turn an array of numbers into probabilities that sum to 1.
// Temperature: low = peaky, high = flat. Default = 1.0.
// ---------------------------------------------------------------------------

pub fn builtin_ml_softmax(args: &[Value]) -> Result<Value> {
    require_at_least("ml_softmax", args, 1)?;
    let arr = require_array("ml_softmax", &args[0])?;
    let temp = if args.len() >= 2 {
        require_number("ml_softmax", &args[1])?
    } else {
        1.0
    };

    if temp <= 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_softmax() temperature must be > 0".to_string(),
        });
    }

    let values = array_to_f64_vec("ml_softmax", &arr)?;
    if values.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "ml_softmax() array cannot be empty".to_string(),
        });
    }

    // Subtract max for numerical stability
    let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = values.iter().map(|v| ((v - max_val) / temp).exp()).collect();
    let sum: f64 = exps.iter().sum();

    let mut result = AudionArray::new();
    for e in &exps {
        result.push_auto(Value::Number(e / sum));
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

// ---------------------------------------------------------------------------
// ml_entropy(probabilities)
//
// Shannon entropy of a probability distribution (in bits).
// ---------------------------------------------------------------------------

pub fn builtin_ml_entropy(args: &[Value]) -> Result<Value> {
    require_at_least("ml_entropy", args, 1)?;
    let arr = require_array("ml_entropy", &args[0])?;
    let probs = array_to_f64_vec("ml_entropy", &arr)?;

    if probs.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "ml_entropy() array cannot be empty".to_string(),
        });
    }

    let mut h = 0.0;
    for &p in &probs {
        if p > 0.0 {
            h -= p * p.log2();
        }
    }

    Ok(Value::Number(h))
}

// ---------------------------------------------------------------------------
// Kalman utilities
// ---------------------------------------------------------------------------

/// Box-Muller transform: two uniform [0,1] samples → one standard normal N(0,1).
///
/// Why Box-Muller? We only have `random_f64()` (uniform). To get Gaussian noise
/// for the Brownian bridge we need N(0,1). Box-Muller is the classic two-liner:
///
///   if U1, U2 ~ Uniform(0,1) then
///   Z = sqrt(-2 ln U1) * cos(2π U2)  ~  N(0,1)
///
/// We clamp U1 away from 0 to avoid ln(0) = -∞.
fn randn() -> f64 {
    let u1 = crate::builtins::random_f64().max(1e-15);
    let u2 = crate::builtins::random_f64();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

// ---------------------------------------------------------------------------
// ml_kalman_filter(array, Q, R)
//
// 1-D Kalman filter (constant model) for smoothing a noisy sequence.
//
// At each step we track:
//   x  — best estimate of the true value
//   P  — our uncertainty about x (error covariance)
//
// Each step is two phases:
//   PREDICT: uncertainty grows by Q (the signal can drift a little)
//     x_pred = x
//     P_pred = P + Q
//
//   UPDATE: blend prediction with the new noisy measurement z
//     K = P_pred / (P_pred + R)   ← Kalman gain, 0..1
//     x = x_pred + K * (z - x_pred)
//     P = (1 - K) * P_pred
//
// K is the key: high Q/R → trust measurements, low Q/R → trust model.
//
// Q (process noise) — how much the true signal can change per step.
//   Small Q → assume slow-moving signal, heavy smoothing.
//   Large Q → assume fast-moving signal, follows measurements more closely.
//
// R (measurement noise) — how noisy your input data is.
//   Small R → trust the measurements.
//   Large R → distrust measurements, rely on prediction.
//
// Musical use: smooth a jittery LFO, sensor stream, or random parameter array.
//   let smooth = ml_kalman_filter(raw, 0.1, 1.0);
// ---------------------------------------------------------------------------

pub fn builtin_ml_kalman_filter(args: &[Value]) -> Result<Value> {
    require_at_least("ml_kalman_filter", args, 3)?;
    let arr = require_array("ml_kalman_filter", &args[0])?;
    let q = require_number("ml_kalman_filter", &args[1])?;
    let r = require_number("ml_kalman_filter", &args[2])?;

    if q < 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_kalman_filter() Q (process noise) must be >= 0".to_string(),
        });
    }
    if r <= 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_kalman_filter() R (measurement noise) must be > 0".to_string(),
        });
    }

    let measurements = array_to_f64_vec("ml_kalman_filter", &arr)?;
    if measurements.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "ml_kalman_filter() array cannot be empty".to_string(),
        });
    }

    // Initialise: first measurement is our best guess, uncertainty = R.
    let mut x = measurements[0];
    let mut p = r;

    let mut result = AudionArray::new();
    result.push_auto(Value::Number(x));

    for &z in &measurements[1..] {
        // --- PREDICT ---
        // x doesn't change (constant model), but uncertainty grows.
        let p_pred = p + q;

        // --- UPDATE ---
        // K: how much to trust the new measurement vs our prediction.
        // If p_pred is large (we're uncertain) → K → 1 → lean on measurement.
        // If r is large (measurement is noisy) → K → 0 → lean on prediction.
        let k = p_pred / (p_pred + r);
        x += k * (z - x);          // nudge estimate toward measurement
        p = (1.0 - k) * p_pred;    // our uncertainty shrinks after each update

        result.push_auto(Value::Number(x));
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

// ---------------------------------------------------------------------------
// ml_kalman_smooth(start, end, steps, [volatility])
//
// Generate an organic trajectory from `start` to `end` over `steps` points.
//
// This is a BROWNIAN BRIDGE — mathematically equivalent to sampling from the
// RTS (Rauch-Tung-Striebel) Kalman smoother posterior with two anchor points.
//
// The idea:
//   1. Generate a random walk W where W[0] = 0, each step += randn() * volatility
//   2. Subtract the linear drift so the walk starts AND ends at 0:
//        bridge[t] = W[t] - (t/N) * W[N]
//   3. Add it on top of the linear interpolation between start and end:
//        x[t] = lerp(start, end, t/N) + bridge[t]
//
// Step 2 is the key insight: we "de-trend" the random walk so its endpoints
// are pinned, while everything in between wanders organically.
//
// volatility = 0  → perfect straight line (lerp)
// volatility small → slight organic curve
// volatility large → wild excursions, still hits start and end exactly
//
// Musical use: morph a filter cutoff, generate a pitch contour, evolve
// dynamics over a phrase.
//   let arc = ml_kalman_smooth(200, 8000, 16, 300);
// ---------------------------------------------------------------------------

pub fn builtin_ml_kalman_smooth(args: &[Value]) -> Result<Value> {
    require_at_least("ml_kalman_smooth", args, 3)?;
    let start      = require_number("ml_kalman_smooth", &args[0])?;
    let end        = require_number("ml_kalman_smooth", &args[1])?;
    let steps      = require_number("ml_kalman_smooth", &args[2])? as usize;
    let volatility = if args.len() >= 4 {
        require_number("ml_kalman_smooth", &args[3])?
    } else {
        0.0
    };

    if steps < 2 {
        return Err(AudionError::RuntimeError {
            msg: "ml_kalman_smooth() steps must be >= 2".to_string(),
        });
    }
    if volatility < 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_kalman_smooth() volatility must be >= 0".to_string(),
        });
    }

    let n = steps - 1; // number of intervals (steps - 1 gaps between steps points)

    // Build the random walk W: cumulative sum of Gaussian steps.
    // W[0] = 0 by construction.
    let mut w = vec![0.0f64; steps];
    for i in 1..steps {
        w[i] = w[i - 1] + randn() * volatility;
    }

    // w[n] is where the walk ended up. We need to subtract a linear ramp
    // so that w[0] stays 0 and w[n] becomes 0 — that's the bridge correction.
    let w_end = w[n];

    let mut result = AudionArray::new();
    for t in 0..steps {
        let t_frac = t as f64 / n as f64;
        // Linear interpolation between start and end:
        let lerp = start + (end - start) * t_frac;
        // Brownian bridge: remove drift so both endpoints are pinned:
        let bridge = w[t] - t_frac * w_end;
        result.push_auto(Value::Number(lerp + bridge));
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

// ---------------------------------------------------------------------------
// ml_kalman_state / ml_kalman_update / ml_kalman_predict
//
// Step-by-step Kalman filter for live/real-time use in sequencer loops.
//
// State is an Audion array of 5 numbers: [x, P, Q, R, x_prev]
//   x      — current best estimate of the true value
//   P      — current uncertainty (error covariance)
//   Q      — process noise (baked in so you don't repeat it each call)
//   R      — measurement noise
//   x_prev — estimate from previous step (used by ml_kalman_predict)
//
// Usage:
//   let state = ml_kalman_state(400, 0.05, 1.0);
//   loop {
//       state = ml_kalman_update(state, noisy_reading);
//       let x      = state[0];            // current filtered value
//       let future = ml_kalman_predict(state, 4); // next 4 predicted values
//   }
// ---------------------------------------------------------------------------

/// State array indices — named constants for clarity.
const KS_X:     i64 = 0;  // current estimate
const KS_P:     i64 = 1;  // uncertainty
const KS_Q:     i64 = 2;  // process noise (stored in state)
const KS_R:     i64 = 3;  // measurement noise (stored in state)
const KS_XPREV: i64 = 4;  // previous estimate (for velocity calculation)

fn kalman_state_get(fn_name: &str, arr: &AudionArray, idx: i64) -> Result<f64> {
    array_get_num(arr, idx).ok_or_else(|| AudionError::RuntimeError {
        msg: format!("{}() invalid state array (missing index {})", fn_name, idx),
    })
}

// ---------------------------------------------------------------------------
// ml_kalman_state(initial_x, Q, R)
//
// Create a fresh Kalman state. Initial uncertainty P is set to R (measurement
// noise) — meaning "we're about as uncertain as one noisy reading".
// ---------------------------------------------------------------------------

pub fn builtin_ml_kalman_state(args: &[Value]) -> Result<Value> {
    require_at_least("ml_kalman_state", args, 3)?;
    let x = require_number("ml_kalman_state", &args[0])?;
    let q = require_number("ml_kalman_state", &args[1])?;
    let r = require_number("ml_kalman_state", &args[2])?;

    if q < 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_kalman_state() Q (process noise) must be >= 0".to_string(),
        });
    }
    if r <= 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_kalman_state() R (measurement noise) must be > 0".to_string(),
        });
    }

    // [x, P=R, Q, R, x_prev=x]
    // P starts at R: "our initial uncertainty equals the noise of one measurement"
    // x_prev starts at x: velocity = 0 until we have a second reading
    let mut state = AudionArray::new();
    state.push_auto(Value::Number(x));  // KS_X
    state.push_auto(Value::Number(r));  // KS_P  (start uncertain = 1 measurement's worth)
    state.push_auto(Value::Number(q));  // KS_Q
    state.push_auto(Value::Number(r));  // KS_R
    state.push_auto(Value::Number(x));  // KS_XPREV

    Ok(Value::Array(Arc::new(Mutex::new(state))))
}

// ---------------------------------------------------------------------------
// ml_kalman_update(state, measurement)
//
// Feed one noisy measurement into the filter. Returns a new state array.
//
// Rust note: we take the state by value (the Arc clone is cheap — it's just
// a reference count bump), extract the numbers, compute, and return a fresh
// array. Audion state is immutable-by-replacement, matching the language's
// assignment semantics.
// ---------------------------------------------------------------------------

pub fn builtin_ml_kalman_update(args: &[Value]) -> Result<Value> {
    require_at_least("ml_kalman_update", args, 2)?;
    let state_arc = require_array("ml_kalman_update", &args[0])?;
    let z         = require_number("ml_kalman_update", &args[1])?;

    let state_locked = state_arc.lock().unwrap();
    let x = kalman_state_get("ml_kalman_update", &state_locked, KS_X)?;
    let p = kalman_state_get("ml_kalman_update", &state_locked, KS_P)?;
    let q = kalman_state_get("ml_kalman_update", &state_locked, KS_Q)?;
    let r = kalman_state_get("ml_kalman_update", &state_locked, KS_R)?;
    drop(state_locked);

    // PREDICT: uncertainty grows by Q (signal may have drifted since last step)
    let p_pred = p + q;

    // UPDATE: Kalman gain — how much to trust the new measurement
    //   K close to 1 → lean on measurement (we were uncertain, or measurement is clean)
    //   K close to 0 → lean on prediction  (we were confident, or measurement is noisy)
    let k     = p_pred / (p_pred + r);
    let x_new = x + k * (z - x);       // nudge estimate toward measurement
    let p_new = (1.0 - k) * p_pred;    // uncertainty shrinks after each update

    // Build new state; old x becomes x_prev for velocity tracking
    let mut new_state = AudionArray::new();
    new_state.push_auto(Value::Number(x_new)); // KS_X
    new_state.push_auto(Value::Number(p_new)); // KS_P
    new_state.push_auto(Value::Number(q));     // KS_Q (unchanged)
    new_state.push_auto(Value::Number(r));     // KS_R (unchanged)
    new_state.push_auto(Value::Number(x));     // KS_XPREV = old x

    Ok(Value::Array(Arc::new(Mutex::new(new_state))))
}

// ---------------------------------------------------------------------------
// ml_kalman_predict(state, steps)
//
// Extrapolate the current trajectory forward `steps` steps.
// Returns an array of predicted values (not including the current estimate).
//
// Velocity is estimated as (x - x_prev) — the trend of the last step.
// This is a first-order extrapolation: simple, stable, and good for short
// horizons (4–16 steps). For longer horizons the uncertainty grows fast anyway.
//
// Rust note: `t as f64` casts the usize loop index to f64 for multiplication.
// Rust requires explicit casts between numeric types — no silent widening.
// ---------------------------------------------------------------------------

pub fn builtin_ml_kalman_predict(args: &[Value]) -> Result<Value> {
    require_at_least("ml_kalman_predict", args, 2)?;
    let state_arc = require_array("ml_kalman_predict", &args[0])?;
    let steps     = require_number("ml_kalman_predict", &args[1])? as usize;

    if steps == 0 {
        return Ok(Value::Array(Arc::new(Mutex::new(AudionArray::new()))));
    }

    let state_locked = state_arc.lock().unwrap();
    let x      = kalman_state_get("ml_kalman_predict", &state_locked, KS_X)?;
    let x_prev = kalman_state_get("ml_kalman_predict", &state_locked, KS_XPREV)?;
    drop(state_locked);

    // Estimated velocity: how much did x change last step?
    // If this is the first update x == x_prev, so velocity = 0 (flat prediction).
    let velocity = x - x_prev;

    let mut result = AudionArray::new();
    for t in 1..=steps {
        result.push_auto(Value::Number(x + velocity * t as f64));
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}

// ---------------------------------------------------------------------------
// ml_normalize(values)
//
// Normalize an array so it sums to 1.
// ---------------------------------------------------------------------------

pub fn builtin_ml_normalize(args: &[Value]) -> Result<Value> {
    require_at_least("ml_normalize", args, 1)?;
    let arr = require_array("ml_normalize", &args[0])?;
    let values = array_to_f64_vec("ml_normalize", &arr)?;

    if values.is_empty() {
        return Err(AudionError::RuntimeError {
            msg: "ml_normalize() array cannot be empty".to_string(),
        });
    }

    let sum: f64 = values.iter().sum();
    if sum == 0.0 {
        return Err(AudionError::RuntimeError {
            msg: "ml_normalize() array sums to 0, cannot normalize".to_string(),
        });
    }

    let mut result = AudionArray::new();
    for v in &values {
        result.push_auto(Value::Number(v / sum));
    }

    Ok(Value::Array(Arc::new(Mutex::new(result))))
}
