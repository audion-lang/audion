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

use crate::ast::{BinOp, UGenExpr};

/// Buffer info for sample-based SynthDefs.
#[derive(Debug, Clone)]
pub struct BufferInfo {
    pub file_path: String,
    pub buffer_id: i32,
    pub num_channels: u32,
}

/// All known UGen names available inside `define` blocks.
pub const UGEN_NAMES: &[&str] = &[
    // Oscillators
    "sine", "saw", "square", "pulse", "tri",
    "blip", "var_saw", "sync_saw", "fsin_osc", "lf_par", "lf_cub", "pm_osc",
    // Noise
    "noise", "white", "pink", "brown", "gray", "clip_noise", "crackle",
    // Filters
    "lpf", "hpf", "bpf", "rlpf", "rhpf", "resonz", "moog_ff", "brf", "formlet",
    "lag", "leak_dc", "ringz", "one_pole", "two_pole", "ramp", "mid_eq", "slew",
    // Envelopes
    "env", "line", "xline", "decay", "linen",
    // LFOs
    "lfo_sine", "lfo_saw", "lfo_tri", "lfo_pulse", "lfo_noise", "lfo_step",
    // Effects
    "reverb", "gverb", "delay", "delay_c", "delay_n", "delay_l",
    "allpass_n", "allpass_l", "allpass_c", "comb_n", "comb_c",
    "coin_gate", "pluck",
    // Distortion / dynamics
    "tanh", "atan", "wrap", "fold", "softclip", "dist",
    "compander", "limiter", "amplitude", "normalizer",
    // Pitch
    "pitch_shift", "freq_shift", "pitch", "vibrato",
    // Analysis / triggers
    "running_sum", "median", "running_max", "running_min",
    "peak", "zero_crossing", "latch", "gate", "pulse_count",
    "t_exprand", "t_irand", "sweep",
    // Routing
    "in", "out", "pan", "pan4", "splay", "balance2",
    "local_in", "local_out",
    // Buffer
    "PlayBuf", "buf_wr", "record_buf", "local_buf",
    // Granular
    "Dust", "Impulse", "TRand", "GrainBuf", "GrainSin", "GrainFM", "grains_t",
    // Signal processing
    "Clip", "Wrap",
    // Helpers
    "array", "array_get", "sample",
    // Analysis feedback
    "send_reply",
];

/// Generate sclang code from an audion `define` block.
/// The `out_path` is where sclang will write the .scsyndef binary.
/// `buffers` maps sample indices (in tree-walk order) to loaded buffer info.
pub fn generate_sclang(
    name: &str,
    params: &[String],
    body: &UGenExpr,
    out_path: &str,
    buffers: &[BufferInfo],
) -> String {
    let mut extra_params = Vec::new();
    let single_sample = buffers.len() == 1;

    // Add bufnum params for each sample
    // Single sample: use "bufnum" for a clean API (synth("x", bufnum: b))
    // Multiple samples: use "bufnum_0", "bufnum_1", etc.
    for (i, buf) in buffers.iter().enumerate() {
        let param_name = if single_sample {
            "bufnum".to_string()
        } else {
            format!("bufnum_{}", i)
        };
        extra_params.push(format!("{}={}", param_name, buf.buffer_id));
    }

    // If any samples have velocity gating, add vel param
    if has_sample_with_vel_range(body) {
        if !params.contains(&"vel".to_string()) {
            extra_params.push("vel=127".to_string());
        }
    }

    let param_str = params
        .iter()
        .filter(|p| {
            // Skip user-declared "bufnum" if we auto-generate it for a single sample
            !(single_sample && p.as_str() == "bufnum")
        })
        .map(|p| {
            let default = default_for_param(p);
            format!("{}={}", p, default)
        })
        .chain(extra_params.into_iter())
        .collect::<Vec<_>>()
        .join(", ");

    let mut sample_idx = 0usize;
    let body_code = emit_ugen(body, buffers, &mut sample_idx, params);

    // writeDefFile writes the .scsyndef binary to the given directory
    format!(
        "SynthDef(\\{}, {{ |{}|\n\t{};\n}}).writeDefFile(\"{}\");\n0.exit;\n",
        name, param_str, body_code, out_path
    )
}

/// All known SynthDef parameter names with their default values.
pub const DEFAULT_PARAMS: &[(&str, &str)] = &[
    ("freq", "440"),
    ("amp", "0.1"),
    ("pan", "0"),
    ("gate", "1"),
    ("out", "0"),
    ("density", "20"),
    ("rate", "1"),
    ("pos", "0.5"),
    ("spray", "0.1"),
    ("gdur", "0.1"),
    ("gdur_rand", "0"),
    ("pitch_rand", "0"),
    ("width", "1"),
    ("atk", "0.01"),
    ("sus", "1"),
    ("rel", "0.3"),
    ("filt", "20000"),
    ("filt_q", "1"),
    ("cutoff", "20000"),
    ("scan_speed", "0.1"),
    ("scan_depth", "0"),
    ("lfo_rate", "1"),
    ("lfo_depth", "0"),
    ("mix", "0.5"),
    ("rmix", "0.3"),
    ("rroom", "0.5"),
    ("rdamp", "0.5"),
    ("del_time", "0.2"),
    ("del_decay", "0.5"),
    ("ratio", "1"),
    ("index", "0"),
    ("fb", "0"),
];

fn default_for_param(name: &str) -> &'static str {
    DEFAULT_PARAMS
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
        .unwrap_or("0")
}

/// Check if any sample() call in the tree has non-default vel_lo or vel_hi.
fn has_sample_with_vel_range(expr: &UGenExpr) -> bool {
    match expr {
        UGenExpr::UGenCall { name, args, named_args } => {
            if name == "sample" {
                let vel_lo = get_named_number(named_args, "vel_lo").unwrap_or(0.0);
                let vel_hi = get_named_number(named_args, "vel_hi").unwrap_or(127.0);
                if vel_lo != 0.0 || vel_hi != 127.0 {
                    return true;
                }
            }
            args.iter().any(|a| has_sample_with_vel_range(a))
        }
        UGenExpr::BinOp { left, right, .. } => {
            has_sample_with_vel_range(left) || has_sample_with_vel_range(right)
        }
        UGenExpr::Block { lets, results } => {
            lets.iter().any(|(_, v)| has_sample_with_vel_range(v))
                || results.iter().any(|r| has_sample_with_vel_range(r))
        }
        _ => false,
    }
}

fn emit_ugen(expr: &UGenExpr, buffers: &[BufferInfo], sample_idx: &mut usize, params: &[String]) -> String {
    match expr {
        UGenExpr::Number(n) => {
            if *n == (*n as i64) as f64 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        UGenExpr::StringLit(s) => {
            // String literals in UGen context are only valid inside sample() calls.
            // If encountered bare, just emit as a quoted string (will be caught by SC compiler).
            format!("\"{}\"", s)
        }
        UGenExpr::Param(name) => name.clone(),
        UGenExpr::BinOp { left, op, right } => {
            let l = emit_ugen(left, buffers, sample_idx, params);
            let r = emit_ugen(right, buffers, sample_idx, params);
            let op_str = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
                _ => "+",
            };
            format!("({} {} {})", l, op_str, r)
        }
        UGenExpr::Index { object, index } => {
            let obj = emit_ugen(object, buffers, sample_idx, params);
            let idx = emit_ugen(index, buffers, sample_idx, params);
            format!("{}[{}]", obj, idx)
        }
        UGenExpr::Block { lets, results } => {
            // Emit SC var declarations + assignments, then all result expressions
            let mut lines = Vec::new();

            // Filter out any let variables that shadow function parameters
            let param_set: std::collections::HashSet<&str> = params.iter().map(|s| s.as_str()).collect();
            let new_vars: Vec<&str> = lets.iter()
                .map(|(n, _)| n.as_str())
                .filter(|n| !param_set.contains(n))
                .collect();

            // Emit var declarations only for new variables (not shadowing params)
            if !new_vars.is_empty() {
                lines.push(format!("var {}", new_vars.join(", ")));
            }

            // Emit let assignments (all of them, whether shadowing or not)
            for (name, value) in lets {
                let val_code = emit_ugen(value, buffers, sample_idx, params);
                lines.push(format!("{} = {}", name, val_code));
            }

            // Emit all result expressions (e.g., multiple out() calls)
            for result in results {
                lines.push(emit_ugen(result, buffers, sample_idx, params));
            }

            lines.join(";\n\t")
        }
        UGenExpr::UGenCall { name, args, named_args } => {
            if name == "sample" {
                let code = emit_sample_ugen(named_args, buffers, sample_idx);
                // Still recurse into positional args to advance sample_idx for any nested samples
                for a in args.iter().skip(1) {
                    // skip the file path (first positional arg)
                    emit_ugen(a, buffers, sample_idx, params);
                }
                return code;
            }
            if name == "stream_disk" || name == "stream_disk_variable_rate" {
                let arg_strs: Vec<String> = args.iter().map(|a| emit_ugen(a, buffers, sample_idx, params)).collect();
                return emit_stream_disk_ugen(name, &arg_strs, named_args);
            }
            let arg_strs: Vec<String> = args.iter().map(|a| emit_ugen(a, buffers, sample_idx, params)).collect();
            emit_ugen_call(name, &arg_strs)
        }
    }
}

/// Emit SC code for a sample() UGen call.
fn emit_sample_ugen(
    named_args: &[(String, UGenExpr)],
    buffers: &[BufferInfo],
    sample_idx: &mut usize,
) -> String {
    let idx = *sample_idx;
    *sample_idx += 1;

    let num_ch = buffers.get(idx).map(|b| b.num_channels).unwrap_or(2);
    let bufnum_param = if buffers.len() == 1 {
        "bufnum".to_string()
    } else {
        format!("bufnum_{}", idx)
    };

    // Extract named properties with defaults
    let root = get_named_number(named_args, "root").unwrap_or(60.0);
    let vel_lo = get_named_number(named_args, "vel_lo").unwrap_or(0.0);
    let vel_hi = get_named_number(named_args, "vel_hi").unwrap_or(127.0);
    let key_lo = get_named_number(named_args, "key_lo").unwrap_or(0.0);
    let key_hi = get_named_number(named_args, "key_hi").unwrap_or(127.0);
    let loop_flag = get_named_number(named_args, "loop").unwrap_or(0.0);
    let loop_start_sc = get_named_expr_sc(named_args, "loop_start").unwrap_or_else(|| "0".to_string());
    let loop_end_sc = get_named_expr_sc(named_args, "loop_end");
    let loop_end = get_named_number(named_args, "loop_end").unwrap_or(0.0);
    let detune = get_named_number(named_args, "detune").unwrap_or(0.0);
    let start_sc = get_named_expr_sc(named_args, "start").unwrap_or_else(|| "0".to_string());

    // Calculate root frequency from MIDI note: 440 * 2^((root-69)/12)
    let root_hz = 440.0 * (2.0_f64).powf((root - 69.0) / 12.0);

    // Detune multiplier: 2^(cents/1200)
    let detune_mult = if detune != 0.0 {
        (2.0_f64).powf(detune / 1200.0)
    } else {
        1.0
    };

    // Rate expression
    let rate_expr = format!(
        "(freq / {:.6}) * {} * BufRateScale.kr({})",
        root_hz, detune_mult, bufnum_param
    );

    // Use Phasor/BufRd path when loop is set and loop_end is either a non-zero literal
    // or a variable expression (synthdef parameter).
    let use_phasor = loop_flag != 0.0 && (loop_end > 0.0 || loop_end_sc.is_some());
    let loop_end_sc = loop_end_sc.unwrap_or_else(|| "0".to_string());

    // Build the PlayBuf/BufRd expression
    let playback = if use_phasor {
        // Precise loop points with BufRd + Phasor
        format!(
            "BufRd.ar({}, {}, Phasor.ar(0, {}, {}, {}))",
            num_ch, bufnum_param, rate_expr, loop_start_sc, loop_end_sc
        )
    } else {
        // Standard PlayBuf
        let loop_int = if loop_flag != 0.0 { 1 } else { 0 };
        format!(
            "PlayBuf.ar({}, {}, {}, 1, {}, {})",
            num_ch, bufnum_param, rate_expr, start_sc, loop_int
        )
    };

    // Velocity gating
    let has_vel_range = vel_lo != 0.0 || vel_hi != 127.0;
    // Key range gating
    let has_key_range = key_lo != 0.0 || key_hi != 127.0;

    let mut result = playback;

    if has_vel_range {
        result = format!("({} * ((vel >= {}) * (vel <= {})))", result, vel_lo, vel_hi);
    }

    if has_key_range {
        result = format!(
            "({} * ((freq.cpsmidi >= {}) * (freq.cpsmidi <= {})))",
            result, key_lo, key_hi
        );
    }

    result
}

/// Extract a named numeric argument value.
fn get_named_number(named_args: &[(String, UGenExpr)], key: &str) -> Option<f64> {
    named_args.iter().find_map(|(name, expr)| {
        if name == key {
            if let UGenExpr::Number(n) = expr {
                Some(*n)
            } else {
                None
            }
        } else {
            None
        }
    })
}

/// Returns the SC code string for a named arg, handling both Number and Param (variable) values.
fn get_named_expr_sc(named_args: &[(String, UGenExpr)], key: &str) -> Option<String> {
    named_args.iter().find_map(|(name, expr)| {
        if name == key {
            match expr {
                UGenExpr::Number(n) => {
                    if *n == (*n as i64) as f64 && n.is_finite() {
                        Some(format!("{}", *n as i64))
                    } else {
                        Some(format!("{}", n))
                    }
                }
                UGenExpr::Param(p) => Some(p.clone()),
                _ => None,
            }
        } else {
            None
        }
    })
}

/// Emit SC code for stream_disk() and stream_disk_variable_rate() UGen calls.
fn emit_stream_disk_ugen(
    name: &str,
    args: &[String],
    named_args: &[(String, UGenExpr)],
) -> String {
    let channels = get_named_number(named_args, "channels").unwrap_or(2.0) as i32;
    let loop_flag = get_named_number(named_args, "loop").unwrap_or(0.0) as i32;

    if name == "stream_disk_variable_rate" {
        let bufnum = args.first().map(|s| s.as_str()).unwrap_or("0");
        let rate = args.get(1).map(|s| s.as_str()).unwrap_or("1");
        format!("VDiskIn.ar({}, {}, {}, {})", channels, bufnum, rate, loop_flag)
    } else {
        // stream_disk
        let bufnum = args.first().map(|s| s.as_str()).unwrap_or("0");
        format!("DiskIn.ar({}, {}, {})", channels, bufnum, loop_flag)
    }
}

fn emit_ugen_call(name: &str, args: &[String]) -> String {
    match name {
        // Array operations (compile to SuperCollider syntax)
        "array_get" => {
            // array_get(arr, index) → arr[index] or arr.at(index)
            let arr = args.first().map(|s| s.as_str()).unwrap_or("0");
            let index = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            format!("{}[{}]", arr, index)
        }
        "array" => {
            // array(a, b, c) → [a, b, c]
            format!("[{}]", args.join(", "))
        }

        // Oscillators
        "sine" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            format!("SinOsc.ar({})", freq)
        }
        "saw" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            format!("Saw.ar({})", freq)
        }
        "square" | "pulse" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            let width = args.get(1).map(|s| s.as_str()).unwrap_or("0.5");
            format!("Pulse.ar({}, {})", freq, width)
        }
        "tri" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            format!("LFTri.ar({})", freq)
        }
        "noise" => "WhiteNoise.ar".to_string(),
        "white" => "WhiteNoise.ar".to_string(), // same just obvious alias
        "pink" => "PinkNoise.ar".to_string(),
        "brown" => "BrownNoise.ar".to_string(),
        "gray" => "GrayNoise.ar".to_string(),
        "clip_noise" => "ClipNoise.ar".to_string(),

        // More oscillators
        "blip" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            let numharm = args.get(1).map(|s| s.as_str()).unwrap_or("200");
            format!("Blip.ar({}, {})", freq, numharm)
        }
        "var_saw" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            let width = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            format!("VarSaw.ar({}, 0, {})", freq, width)
        }
        "sync_saw" => {
            let sync_freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            let saw_freq = args.get(1).map(|s| s.as_str()).unwrap_or("440");
            format!("SyncSaw.ar({}, {})", sync_freq, saw_freq)
        }
        "fsin_osc" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            format!("FSinOsc.ar({})", freq)
        }
        "lf_par" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("LFPar.ar({})", freq)
        }
        "lf_cub" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("LFCub.ar({})", freq)
        }
        "pm_osc" => {
            // PMOsc.ar(carfreq, modfreq, pmindex, modphase)
            let carfreq = args.first().map(|s| s.as_str()).unwrap_or("440");
            let modfreq = args.get(1).map(|s| s.as_str()).unwrap_or("440");
            let pmindex = args.get(2).map(|s| s.as_str()).unwrap_or("0");
            let modphase = args.get(3).map(|s| s.as_str()).unwrap_or("0");
            format!("PMOsc.ar({}, {}, {}, {})", carfreq, modfreq, pmindex, modphase)
        }

        // Filters
        "lpf" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let cutoff = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            format!("LPF.ar({}, {})", sig, cutoff)
        }
        "hpf" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let cutoff = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            format!("HPF.ar({}, {})", sig, cutoff)
        }
        "bpf" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            let rq = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("BPF.ar({}, {}, {})", sig, freq, rq)
        }
        "rlpf" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let cutoff = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            let rq = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("RLPF.ar({}, {}, {})", sig, cutoff, rq)
        }
        "rhpf" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let cutoff = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            let rq = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("RHPF.ar({}, {}, {})", sig, cutoff, rq)
        }

        // Envelope: env(gate) / env(gate, atk, sus, rel)
        // When sus=0, generates Env.perc(atk, rel) for percussive one-shots
        // Otherwise generates Env.asr(atk, sus, rel) for sustained sounds
        "env" => {
            let gate = args.first().map(|s| s.as_str()).unwrap_or("gate");
            let atk = args.get(1).map(|s| s.as_str()).unwrap_or("0.01");
            let sus = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            let rel = args.get(3).map(|s| s.as_str()).unwrap_or("0.3");
            if sus == "0" {
                // Percussive envelope: attack to peak then decay, ignores gate
                format!(
                    "EnvGen.kr(Env.perc({}, {}), {}, doneAction: 2)",
                    atk, rel, gate
                )
            } else {
                format!(
                    "EnvGen.kr(Env.asr({}, {}, {}), {}, doneAction: 2)",
                    atk, sus, rel, gate
                )
            }
        }
        // === Envelope UGens ===
        "line" => {
            // Line.ar(start, end, dur, doneAction)
            let start = args.first().map(|s| s.as_str()).unwrap_or("0");
            let end = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let dur = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("Line.ar({}, {}, {})", start, end, dur)
        }
        "xline" => {
            // XLine.ar(start, end, dur, doneAction)
            let start = args.first().map(|s| s.as_str()).unwrap_or("0.01");
            let end = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let dur = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("XLine.ar({}, {}, {})", start, end, dur)
        }
        "decay" => { // TODO examples of how to use "in" param ?
            // Decay.ar(in, decayTime, mult, add)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let time = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let mult = args.get(2).map(|s| s.as_str()).unwrap_or("1.0");
            let add = args.get(3).map(|s| s.as_str()).unwrap_or("0");
            format!("Decay.ar({}, {}, {}, {})", sig, time, mult, add)
        }
        "decay2" => { // TODO examples of how to use "in" param ?
            // Decay2.ar(in, attackTime, decayTime)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let atk = args.get(1).map(|s| s.as_str()).unwrap_or("0.01");
            let dec = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("Decay2.ar({}, {}, {})", sig, atk, dec)
        }
        "linen" => { // TODO remove?
            // Linen.kr(gate, attackTime, susLevel, releaseTime, doneAction)
            let gate = args.first().map(|s| s.as_str()).unwrap_or("gate");
            let atk = args.get(1).map(|s| s.as_str()).unwrap_or("0.01");
            let sus = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            let rel = args.get(3).map(|s| s.as_str()).unwrap_or("0.3");
            format!("Linen.kr({}, {}, {}, {}, 2)", gate, atk, sus, rel)
        }

        // LFOs (control rate)
        "lfo_sine" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("SinOsc.kr({})", freq)
        }
        "lfo_saw" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("LFSaw.kr({})", freq)
        }
        "lfo_tri" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("LFTri.kr({})", freq)
        }
        "lfo_pulse" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            let width = args.get(1).map(|s| s.as_str()).unwrap_or("0.5");
            format!("LFPulse.kr({}, 0, {})", freq, width)
        }
        "lfo_noise" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("LFNoise1.kr({})", freq)
        }
        "lfo_step" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("LFNoise0.kr({})", freq)
        }
        "lfo_noise2" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("LFNoise2.kr({})", freq)
        }

        // More noise generators
        "dust2" => {
            let density = args.first().map(|s| s.as_str()).unwrap_or("10");
            format!("Dust2.ar({})", density)
        }
        "crackle" => {
            let chaos = args.first().map(|s| s.as_str()).unwrap_or("1.5");
            format!("Crackle.ar({})", chaos)
        }
        "coin_gate" => {
            let prob = args.first().map(|s| s.as_str()).unwrap_or("0.5");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("Impulse.ar(1)");
            format!("CoinGate.ar({}, {})", prob, trig)
        }

        // Effects
        "reverb" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let mix = args.get(1).map(|s| s.as_str()).unwrap_or("0.33");
            let room = args.get(2).map(|s| s.as_str()).unwrap_or("0.5");
            let damp = args.get(3).map(|s| s.as_str()).unwrap_or("0.5");
            format!("FreeVerb.ar({}, {}, {}, {})", sig, mix, room, damp)
        }
        "freeverb2" => {
            // FreeVerb2.ar(in, in2, mix, room, damp)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let sig2 = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let mix = args.get(2).map(|s| s.as_str()).unwrap_or("0.33");
            let room = args.get(3).map(|s| s.as_str()).unwrap_or("0.5");
            let damp = args.get(4).map(|s| s.as_str()).unwrap_or("0.5");
            format!("FreeVerb2.ar({}, {}, {}, {}, {})", sig, sig2, mix, room, damp)
        }
        "gverb" => {
            // GVerb.ar(in, roomsize, revtime, damping, inputbw, spread, drylevel, earlyreflevel, taillevel, maxroomsize)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let roomsize = args.get(1).map(|s| s.as_str()).unwrap_or("10");
            let revtime = args.get(2).map(|s| s.as_str()).unwrap_or("3");
            let damping = args.get(3).map(|s| s.as_str()).unwrap_or("0.5");
            let inputbw = args.get(4).map(|s| s.as_str()).unwrap_or("0.5");
            let spread = args.get(5).map(|s| s.as_str()).unwrap_or("15");
            let drylevel = args.get(6).map(|s| s.as_str()).unwrap_or("1");
            let earlylevel = args.get(7).map(|s| s.as_str()).unwrap_or("0.7");
            let taillevel = args.get(8).map(|s| s.as_str()).unwrap_or("0.5");
            format!("GVerb.ar({}, {}, {}, {}, {}, {}, {}, {}, {})",
                sig, roomsize, revtime, damping, inputbw, spread, drylevel, earlylevel, taillevel)
        }
        "delay" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let time = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let decay = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("CombL.ar({}, {}, {}, {})", sig, time, time, decay)
        }
        "delay_c" => {
            // DelayC.ar(in, maxdelaytime, delaytime)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let maxtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let time = args.get(2).map(|s| s.as_str()).unwrap_or("0.2");
            format!("DelayC.ar({}, {}, {})", sig, maxtime, time)
        }
        "local_in" => {
            // LocalIn.ar(numChannels)
            let channels = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("LocalIn.ar({})", channels)
        }
        "local_out" => {
            // LocalOut.ar(channelsArray)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("LocalOut.ar({})", sig)
        }

        // === Category 1: Allpass Filters (Essential for reverbs, phasers, flangers) ===
        "allpass_n" => {
            // AllpassN.ar(in, maxdelaytime, delaytime, decaytime)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let maxtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let time = args.get(2).map(|s| s.as_str()).unwrap_or("0.2");
            let decay = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            format!("AllpassN.ar({}, {}, {}, {})", sig, maxtime, time, decay)
        }
        "allpass_l" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let maxtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let time = args.get(2).map(|s| s.as_str()).unwrap_or("0.2");
            let decay = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            format!("AllpassL.ar({}, {}, {}, {})", sig, maxtime, time, decay)
        }
        "allpass_c" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let maxtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let time = args.get(2).map(|s| s.as_str()).unwrap_or("0.2");
            let decay = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            format!("AllpassC.ar({}, {}, {}, {})", sig, maxtime, time, decay)
        }

        // === Category 2: More Filter Types ===
        "resonz" => {
            // Resonz.ar(in, freq, bwr) - Resonant bandpass filter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("440");
            let bwr = args.get(2).map(|s| s.as_str()).unwrap_or("0.1");
            format!("Resonz.ar({}, {}, {})", sig, freq, bwr)
        }
        "moog_ff" => {
            // MoogFF.ar(in, freq, gain) - Moog ladder filter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            let gain = args.get(2).map(|s| s.as_str()).unwrap_or("2");
            format!("MoogFF.ar({}, {}, {})", sig, freq, gain)
        }
        "brf" => {
            // BRF.ar(in, freq, rq) - Band-reject (notch) filter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            let rq = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("BRF.ar({}, {}, {})", sig, freq, rq)
        }
        "formlet" => {
            // Formlet.ar(in, freq, attacktime, decaytime) - Formant filter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            let attack = args.get(2).map(|s| s.as_str()).unwrap_or("0.005");
            let decay = args.get(3).map(|s| s.as_str()).unwrap_or("0.04");
            format!("Formlet.ar({}, {}, {}, {})", sig, freq, attack, decay)
        }
        "lag" => {
            // Lag.ar(in, lagtime) - Exponential lag (slew rate limiter)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let lagtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.1");
            format!("Lag.ar({}, {})", sig, lagtime)
        }
        "lag2" => {
            // Lag2.ar(in, lagtime) - Double exponential lag (smoother)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let lagtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.1");
            format!("Lag2.ar({}, {})", sig, lagtime)
        }
        "leak_dc" => {
            // LeakDC.ar(in, coef) - DC blocker
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let coef = args.get(1).map(|s| s.as_str()).unwrap_or("0.995");
            format!("LeakDC.ar({}, {})", sig, coef)
        }
        "ringz" => {
            // Ringz.ar(in, freq, decaytime)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("440");
            let decay = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("Ringz.ar({}, {}, {})", sig, freq, decay)
        }
        "one_pole" => {
            // OnePole.ar(in, coef)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let coef = args.get(1).map(|s| s.as_str()).unwrap_or("0.5");
            format!("OnePole.ar({}, {})", sig, coef)
        }
        "two_pole" => {
            // TwoPole.ar(in, freq, radius)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            let radius = args.get(2).map(|s| s.as_str()).unwrap_or("0.8");
            format!("TwoPole.ar({}, {}, {})", sig, freq, radius)
        }
        "ramp" => {
            // Ramp.ar(in, lagtime)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let lagtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.1");
            format!("Ramp.ar({}, {})", sig, lagtime)
        }
        "hpz1" => {
            // HPZ1.ar(in) - 2 point highpass filter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("HPZ1.ar({})", sig)
        }
        "lpz1" => {
            // LPZ1.ar(in) - 2 point lowpass filter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("LPZ1.ar({})", sig)
        }
        "hpz2" => {
            // HPZ2.ar(in) - 2 point highpass filter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("HPZ2.ar({})", sig)
        }
        "lpz2" => {
            // LPZ2.ar(in) - 2 point lowpass filter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("LPZ2.ar({})", sig)
        }
        "mid_eq" => {
            // MidEQ.ar(in, freq, rq, db)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("1000");
            let rq = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            let db = args.get(3).map(|s| s.as_str()).unwrap_or("0");
            format!("MidEQ.ar({}, {}, {}, {})", sig, freq, rq, db)
        }
        "slew" => {
            // Slew.ar(in, up, dn) - Slew rate limiter
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let up = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let dn = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("Slew.ar({}, {}, {})", sig, up, dn)
        }

        // === Category 3: Variable Delays ===
        "delay_n" => {
            // DelayN.ar(in, maxdelaytime, delaytime) - No interpolation
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let maxtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let time = args.get(2).map(|s| s.as_str()).unwrap_or("0.2");
            format!("DelayN.ar({}, {}, {})", sig, maxtime, time)
        }
        "delay_l" => {
            // DelayL.ar(in, maxdelaytime, delaytime) - Linear interpolation
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let maxtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let time = args.get(2).map(|s| s.as_str()).unwrap_or("0.2");
            format!("DelayL.ar({}, {}, {})", sig, maxtime, time)
        }
        "comb_n" => {
            // CombN.ar(in, maxdelaytime, delaytime, decaytime)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let maxtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let time = args.get(2).map(|s| s.as_str()).unwrap_or("0.2");
            let decay = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            format!("CombN.ar({}, {}, {}, {})", sig, maxtime, time, decay)
        }
        "comb_c" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let maxtime = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let time = args.get(2).map(|s| s.as_str()).unwrap_or("0.2");
            let decay = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            format!("CombC.ar({}, {}, {}, {})", sig, maxtime, time, decay)
        }
        "delay1" => {
            // Delay1.ar(in) - 1 sample delay
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("Delay1.ar({})", sig)
        }
        "delay2" => {
            // Delay2.ar(in) - 2 sample delay
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("Delay2.ar({})", sig)
        }
        "pluck" => {
            // Pluck.ar(in, trig, maxdelaytime, delaytime, decaytime, coef)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("WhiteNoise.ar");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("Impulse.ar(1)");
            let maxtime = args.get(2).map(|s| s.as_str()).unwrap_or("0.1");
            let time = args.get(3).map(|s| s.as_str()).unwrap_or("0.1");
            let decay = args.get(4).map(|s| s.as_str()).unwrap_or("5");
            let coef = args.get(5).map(|s| s.as_str()).unwrap_or("0.5");
            format!("Pluck.ar({}, {}, {}, {}, {}, {})", sig, trig, maxtime, time, decay, coef)
        }

        // === Category 4: Nonlinear Processing (Distortion, Waveshaping) ===
        "tanh" => {
            // tanh() - Hyperbolic tangent (soft clipping)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("{}.tanh", sig)
        }
        "atan" => {
            // atan() - Arctangent (soft clipping)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("{}.atan", sig)
        }
        "wrap" => {
            // wrap(in, lo, hi) - Wrapping
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let lo = args.get(1).map(|s| s.as_str()).unwrap_or("-1");
            let hi = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("{}.wrap({}, {})", sig, lo, hi)
        }
        "fold" => {
            // fold(in, lo, hi) - Folding
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let lo = args.get(1).map(|s| s.as_str()).unwrap_or("-1");
            let hi = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("{}.fold({}, {})", sig, lo, hi)
        }
        "softclip" => {
            // softclip() - Soft clipping
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("{}.softclip", sig)
        }
        "dist" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let amount = args.get(1).map(|s| s.as_str()).unwrap_or("2");
            format!("({} * {}).clip2(1)", sig, amount)
        }

        // === Category 5: Dynamics Processing ===
        "compander" => {
            // Compander.ar(in, control, thresh, slopeBelow, slopeAbove, clampTime, relaxTime)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let control = args.get(1).map(|s| s.as_str()).unwrap_or(&sig);
            let thresh = args.get(2).map(|s| s.as_str()).unwrap_or("0.5");
            let slope_below = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            let slope_above = args.get(4).map(|s| s.as_str()).unwrap_or("0.5");
            let clamp = args.get(5).map(|s| s.as_str()).unwrap_or("0.01");
            let relax = args.get(6).map(|s| s.as_str()).unwrap_or("0.1");
            format!("Compander.ar({}, {}, {}, {}, {}, {}, {})",
                sig, control, thresh, slope_below, slope_above, clamp, relax)
        }
        "limiter" => {
            // Limiter.ar(in, level, dur)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let level = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let dur = args.get(2).map(|s| s.as_str()).unwrap_or("0.01");
            format!("Limiter.ar({}, {}, {})", sig, level, dur)
        }
        "amplitude" => {
            // Amplitude.ar(in, attacktime, releasetime)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let attack = args.get(1).map(|s| s.as_str()).unwrap_or("0.01");
            let release = args.get(2).map(|s| s.as_str()).unwrap_or("0.01");
            format!("Amplitude.ar({}, {}, {})", sig, attack, release)
        }
        "normalizer" => {
            // Normalizer.ar(in, level, dur)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let level = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let dur = args.get(2).map(|s| s.as_str()).unwrap_or("0.01");
            format!("Normalizer.ar({}, {}, {})", sig, level, dur)
        }

        // === Category 6: Pitch/Frequency Effects ===
        "pitch_shift" => {
            // PitchShift.ar(in, windowSize, pitchRatio, pitchDispersion, timeDispersion)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let winsize = args.get(1).map(|s| s.as_str()).unwrap_or("0.2");
            let ratio = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            let disp = args.get(3).map(|s| s.as_str()).unwrap_or("0");
            let time_disp = args.get(4).map(|s| s.as_str()).unwrap_or("0");
            format!("PitchShift.ar({}, {}, {}, {}, {})", sig, winsize, ratio, disp, time_disp)
        }
        "freq_shift" => {
            // FreqShift.ar(in, freq, phase)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let freq = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let phase = args.get(2).map(|s| s.as_str()).unwrap_or("0");
            format!("FreqShift.ar({}, {}, {})", sig, freq, phase)
        }
        "pitch" => {
            // Pitch.kr(in, initFreq, minFreq, maxFreq, execFreq, maxBinsPerOctave, median, ampThreshold, peakThreshold, downSample, clar)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let init = args.get(1).map(|s| s.as_str()).unwrap_or("440");
            let minf = args.get(2).map(|s| s.as_str()).unwrap_or("60");
            let maxf = args.get(3).map(|s| s.as_str()).unwrap_or("4000");
            format!("Pitch.kr({}, {}, {}, {})", sig, init, minf, maxf)
        }
        "vibrato" => {
            // Vibrato.ar(freq, rate, depth, delay, onset, rateVariation, depthVariation, iphase)
            let freq = args.first().map(|s| s.as_str()).unwrap_or("440");
            let rate = args.get(1).map(|s| s.as_str()).unwrap_or("6");
            let depth = args.get(2).map(|s| s.as_str()).unwrap_or("0.02");
            let delay = args.get(3).map(|s| s.as_str()).unwrap_or("0");
            let onset = args.get(4).map(|s| s.as_str()).unwrap_or("0");
            let ratevar = args.get(5).map(|s| s.as_str()).unwrap_or("0.04");
            let depthvar = args.get(6).map(|s| s.as_str()).unwrap_or("0.1");
            format!("Vibrato.ar({}, {}, {}, {}, {}, {}, {})", freq, rate, depth, delay, onset, ratevar, depthvar)
        }

        // === Category 7: Analysis & Control ===
        "running_sum" => {
            // RunningSum.ar(in, numsamps)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let num = args.get(1).map(|s| s.as_str()).unwrap_or("40");
            format!("RunningSum.ar({}, {})", sig, num)
        }
        "median" => {
            // Median.ar(length, in)
            let length = args.first().map(|s| s.as_str()).unwrap_or("3");
            let sig = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            format!("Median.ar({}, {})", length, sig)
        }
        "running_max" => {
            // RunningMax.ar(in, numsamps)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let num = args.get(1).map(|s| s.as_str()).unwrap_or("40");
            format!("RunningMax.ar({}, {})", sig, num)
        }
        "running_min" => {
            // RunningMin.ar(in, numsamps)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let num = args.get(1).map(|s| s.as_str()).unwrap_or("40");
            format!("RunningMin.ar({}, {})", sig, num)
        }
        "peak" => {
            // Peak.ar(in, trig)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("Impulse.ar(10)");
            format!("Peak.ar({}, {})", sig, trig)
        }
        "zero_crossing" => {
            // ZeroCrossing.ar(in)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            format!("ZeroCrossing.ar({})", sig)
        }

        // === Triggers ===
        "latch" => {
            // Latch.ar(in, trig)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("Impulse.ar(1)");
            format!("Latch.ar({}, {})", sig, trig)
        }
        "gate" => {
            // Gate.ar(in, trig)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            format!("Gate.ar({}, {})", sig, trig)
        }
        "pulse_count" => {
            // PulseCount.ar(trig, reset)
            let trig = args.first().map(|s| s.as_str()).unwrap_or("Impulse.ar(1)");
            let reset = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            format!("PulseCount.ar({}, {})", trig, reset)
        }
        "t_exprand" => {
            // TExpRand.ar(lo, hi, trig)
            let lo = args.first().map(|s| s.as_str()).unwrap_or("0.01");
            let hi = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let trig = args.get(2).map(|s| s.as_str()).unwrap_or("Impulse.ar(1)");
            format!("TExpRand.ar({}, {}, {})", lo, hi, trig)
        }
        "t_irand" => {
            // TIRand.ar(lo, hi, trig)
            let lo = args.first().map(|s| s.as_str()).unwrap_or("0");
            let hi = args.get(1).map(|s| s.as_str()).unwrap_or("127");
            let trig = args.get(2).map(|s| s.as_str()).unwrap_or("Impulse.ar(1)");
            format!("TIRand.ar({}, {}, {})", lo, hi, trig)
        }
        "sweep" => {
            // Sweep.ar(trig, rate)
            let trig = args.first().map(|s| s.as_str()).unwrap_or("Impulse.ar(1)");
            let rate = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            format!("Sweep.ar({}, {})", trig, rate)
        }

        // Input/Output
        "in" => {
            let bus = args.first().map(|s| s.as_str()).unwrap_or("0");
            let channels = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            format!("In.ar({}, {})", bus, channels)
        }
        "out" => {
            let bus = args.first().map(|s| s.as_str()).unwrap_or("0");
            let sig = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            format!("Out.ar({}, {})", bus, sig)
        }
        "pan" => {
            let pos = args.first().map(|s| s.as_str()).unwrap_or("0");
            let sig = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            format!("Pan2.ar({}, {})", sig, pos)
        }
        "pan4" => {
            // Pan4.ar(in, xpos, ypos, level)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let xpos = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let ypos = args.get(2).map(|s| s.as_str()).unwrap_or("0");
            let level = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            format!("Pan4.ar({}, {}, {}, {})", sig, xpos, ypos, level)
        }
        "splay" => {
            // Splay.ar(inArray, spread, level, center)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("[0, 0, 0, 0]");
            let spread = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let level = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            let center = args.get(3).map(|s| s.as_str()).unwrap_or("0");
            format!("Splay.ar({}, {}, {}, {})", sig, spread, level, center)
        }
        "balance2" => {
            // Balance2.ar(left, right, pos, level)
            let left = args.first().map(|s| s.as_str()).unwrap_or("0");
            let right = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let pos = args.get(2).map(|s| s.as_str()).unwrap_or("0");
            let level = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            format!("Balance2.ar({}, {}, {}, {})", left, right, pos, level)
        }

        // Buffer playback
        "PlayBuf" => {
            // PlayBuf.ar(numChannels, bufnum, rate, trigger, startPos, loop)
            let numchans = args.first().map(|s| s.as_str()).unwrap_or("2");
            let bufnum = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let rate = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            let trig = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            let startpos = args.get(4).map(|s| s.as_str()).unwrap_or("0");
            let looping = args.get(5).map(|s| s.as_str()).unwrap_or("0");
            format!("PlayBuf.ar({}, {}, {}, {}, {}, {})", numchans, bufnum, rate, trig, startpos, looping)
        }
        "buf_wr" => {
            // BufWr.ar(inputArray, bufnum, phase, loop)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let bufnum = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let phase = args.get(2).map(|s| s.as_str()).unwrap_or("0");
            let loop_flag = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            format!("BufWr.ar({}, {}, {}, {})", sig, bufnum, phase, loop_flag)
        }
        "record_buf" => {
            // RecordBuf.ar(inputArray, bufnum, offset, recLevel, preLevel, run, loop, trigger, doneAction)
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let bufnum = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let offset = args.get(2).map(|s| s.as_str()).unwrap_or("0");
            let reclevel = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            let prelevel = args.get(4).map(|s| s.as_str()).unwrap_or("0");
            let run = args.get(5).map(|s| s.as_str()).unwrap_or("1");
            let loop_flag = args.get(6).map(|s| s.as_str()).unwrap_or("1");
            format!("RecordBuf.ar({}, {}, {}, {}, {}, {}, {})", sig, bufnum, offset, reclevel, prelevel, run, loop_flag)
        }
        "local_buf" => {
            // LocalBuf.new(numFrames, numChannels)
            let frames = args.first().map(|s| s.as_str()).unwrap_or("2048");
            let channels = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            format!("LocalBuf.new({}, {})", frames, channels)
        }

        // Granular synthesis
        "Dust" => {
            let density = args.first().map(|s| s.as_str()).unwrap_or("10");
            format!("Dust.ar({})", density)
        }
        "Impulse" => {
            let freq = args.first().map(|s| s.as_str()).unwrap_or("1");
            format!("Impulse.ar({})", freq)
        }
        "TRand" => {
            let lo = args.first().map(|s| s.as_str()).unwrap_or("0");
            let hi = args.get(1).map(|s| s.as_str()).unwrap_or("1");
            let trig = args.get(2).map(|s| s.as_str()).unwrap_or("Impulse.ar(1)");
            format!("TRand.ar({}, {}, {})", lo, hi, trig)
        }
        "GrainBuf" => {
            // GrainBuf.ar(numChannels, trigger, dur, sndbuf, rate, pos, interp, pan, envbufnum, maxGrains)
            let numchans = args.first().map(|s| s.as_str()).unwrap_or("2");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("Impulse.ar(10)");
            let dur = args.get(2).map(|s| s.as_str()).unwrap_or("0.1");
            let sndbuf = args.get(3).map(|s| s.as_str()).unwrap_or("0");
            let rate = args.get(4).map(|s| s.as_str()).unwrap_or("1");
            let pos = args.get(5).map(|s| s.as_str()).unwrap_or("0");
            let interp = args.get(6).map(|s| s.as_str()).unwrap_or("2");
            let pan = args.get(7).map(|s| s.as_str()).unwrap_or("0");
            let envbuf = args.get(8).map(|s| s.as_str()).unwrap_or("-1");
            let maxgrains = args.get(9).map(|s| s.as_str()).unwrap_or("512");
            format!(
                "GrainBuf.ar({}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
                numchans, trig, dur, sndbuf, rate, pos, interp, pan, envbuf, maxgrains
            )
        }
        "GrainSin" => {
            // GrainSin.ar(numChannels, trigger, dur, freq, pan, envbufnum, maxGrains)
            let numchans = args.first().map(|s| s.as_str()).unwrap_or("2");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("Impulse.ar(10)");
            let dur = args.get(2).map(|s| s.as_str()).unwrap_or("0.1");
            let freq = args.get(3).map(|s| s.as_str()).unwrap_or("440");
            let pan = args.get(4).map(|s| s.as_str()).unwrap_or("0");
            let envbuf = args.get(5).map(|s| s.as_str()).unwrap_or("-1");
            let maxgrains = args.get(6).map(|s| s.as_str()).unwrap_or("512");
            format!(
                "GrainSin.ar({}, {}, {}, {}, {}, {}, {})",
                numchans, trig, dur, freq, pan, envbuf, maxgrains
            )
        }
        "GrainFM" => {
            // GrainFM.ar(numChannels, trigger, dur, carfreq, modfreq, index, pan, envbufnum, maxGrains)
            let numchans = args.first().map(|s| s.as_str()).unwrap_or("2");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("Impulse.ar(10)");
            let dur = args.get(2).map(|s| s.as_str()).unwrap_or("0.1");
            let carfreq = args.get(3).map(|s| s.as_str()).unwrap_or("440");
            let modfreq = args.get(4).map(|s| s.as_str()).unwrap_or("200");
            let index = args.get(5).map(|s| s.as_str()).unwrap_or("1");
            let pan = args.get(6).map(|s| s.as_str()).unwrap_or("0");
            let envbuf = args.get(7).map(|s| s.as_str()).unwrap_or("-1");
            let maxgrains = args.get(8).map(|s| s.as_str()).unwrap_or("512");
            format!(
                "GrainFM.ar({}, {}, {}, {}, {}, {}, {}, {}, {})",
                numchans, trig, dur, carfreq, modfreq, index, pan, envbuf, maxgrains
            )
        }
        "grains_t" => {
            // TGrains.ar(numChannels, trigger, bufnum, rate, centerPos, dur, pan, amp, interp)
            let numchans = args.first().map(|s| s.as_str()).unwrap_or("2");
            let trig = args.get(1).map(|s| s.as_str()).unwrap_or("Impulse.ar(10)");
            let bufnum = args.get(2).map(|s| s.as_str()).unwrap_or("0");
            let rate = args.get(3).map(|s| s.as_str()).unwrap_or("1");
            let centerpos = args.get(4).map(|s| s.as_str()).unwrap_or("0");
            let dur = args.get(5).map(|s| s.as_str()).unwrap_or("0.1");
            let pan = args.get(6).map(|s| s.as_str()).unwrap_or("0");
            let amp = args.get(7).map(|s| s.as_str()).unwrap_or("0.1");
            let interp = args.get(8).map(|s| s.as_str()).unwrap_or("4");
            format!(
                "TGrains.ar({}, {}, {}, {}, {}, {}, {}, {}, {})",
                numchans, trig, bufnum, rate, centerpos, dur, pan, amp, interp
            )
        }

        // Signal processing methods (not UGens!)
        "Clip" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let lo = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let hi = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("({}).clip({}, {})", sig, lo, hi)
        }
        "Wrap" => {
            let sig = args.first().map(|s| s.as_str()).unwrap_or("0");
            let lo = args.get(1).map(|s| s.as_str()).unwrap_or("0");
            let hi = args.get(2).map(|s| s.as_str()).unwrap_or("1");
            format!("({}).wrap({}, {})", sig, lo, hi)
        }

        // Analysis feedback — send values back to Audion via OSC
        "send_reply" => {
            let rate   = args.first().map(|s| s.as_str()).unwrap_or("10");
            let addr   = args.get(1).map(|s| s.as_str()).unwrap_or("\"reply\"");
            let values = args.get(2).map(|s| s.as_str()).unwrap_or("0");
            // Convert SC string "/foo" → SC symbol '/foo' (required by SendReply)
            let sc_addr = if addr.starts_with('"') && addr.ends_with('"') {
                format!("'{}'", &addr[1..addr.len()-1])
            } else {
                addr.to_string()
            };
            // Rate is in Hz; silently cap at 100 to prevent OSC flooding.
            // Wrap values in A2K in case any are audio-rate (e.g. Amplitude.ar).
            format!("SendReply.kr(Impulse.kr(({}).min(100)), {}, A2K.kr({}), -1)", rate, sc_addr, values)
        }

        // Unknown — pass through as raw SC UGen
        other => {
            let args_str = args.join(", ");
            format!("{}.ar({})", other, args_str)
        }
    }
}

/// Collect all sample file paths from a UGenExpr tree, in tree-walk order.
pub fn collect_sample_paths(expr: &UGenExpr) -> Vec<String> {
    let mut paths = Vec::new();
    collect_sample_paths_inner(expr, &mut paths);
    paths
}

fn collect_sample_paths_inner(expr: &UGenExpr, paths: &mut Vec<String>) {
    match expr {
        UGenExpr::UGenCall { name, args, .. } => {
            if name == "sample" {
                // First positional arg should be a StringLit (the file path)
                if let Some(UGenExpr::StringLit(path)) = args.first() {
                    paths.push(path.clone());
                }
            }
            for a in args {
                collect_sample_paths_inner(a, paths);
            }
        }
        UGenExpr::BinOp { left, right, .. } => {
            collect_sample_paths_inner(left, paths);
            collect_sample_paths_inner(right, paths);
        }
        UGenExpr::Block { lets, results } => {
            for (_, value) in lets {
                collect_sample_paths_inner(value, paths);
            }
            for result in results {
                collect_sample_paths_inner(result, paths);
            }
        }
        _ => {}
    }
}

