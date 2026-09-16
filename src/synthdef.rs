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
use crate::dsp::ctx::{arg_or, binop, konst, unop, BuildCtx};
use crate::dsp::encoder::encode_synthdef;
use crate::dsp::graph::{GraphIR, Input, Rate};
use crate::error::{AudionError, Result};
use std::collections::HashMap;

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
    "env", "env_perc", "line", "xline", "decay", "linen",
    // LFOs
    "lfo_sine", "lfo_saw", "lfo_tri", "lfo_pulse", "lfo_noise", "lfo_step",
    // Effects
    "reverb", "gverb", "delay", "delay_c", "delay_n", "delay_l",
    "allpass_n", "allpass_l", "allpass_c", "comb_n", "comb_c",
    "coin_gate", "pluck", "klank",
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
    "PlayBuf", "buf_rd", "phasor", "buf_wr", "record_buf", "local_buf", "buf_rate_scale",
    // Granular
    "Dust", "Impulse", "TRand", "GrainBuf", "GrainSin", "GrainFM", "grains_t",
    // Signal processing
    "Clip", "Wrap", "LinLin", "LinExp",
    // Helpers
    "array", "array_get", "sample",
    // Analysis feedback
    "send_reply",
];

/// Build and encode a SynthDef directly to `scsyndef` binary bytes — no
/// sclang, no scsynth involved. Walks the parsed `define` body and builds
/// a UGen graph (`crate::dsp`) node by node, mirroring what the old
/// sclang-text codegen used to emit as source.
pub fn build_synthdef(
    name: &str,
    params: &[String],
    body: &UGenExpr,
    buffers: &[BufferInfo],
) -> Result<Vec<u8>> {
    let mut ctx = BuildCtx::new();
    let single_sample = buffers.len() == 1;

    // bufnum param(s) for each sample, single sample uses the clean
    // "bufnum" name (synth("x", bufnum: b)), multiple use bufnum_0, bufnum_1, ...
    let mut extra_params: Vec<(String, f32)> = Vec::new();
    for (i, buf) in buffers.iter().enumerate() {
        let pname = if single_sample {
            "bufnum".to_string()
        } else {
            format!("bufnum_{}", i)
        };
        extra_params.push((pname, buf.buffer_id as f32));
    }
    if has_sample_with_vel_range(body) && !params.iter().any(|p| p == "vel") {
        extra_params.push(("vel".to_string(), 127.0));
    }

    for p in params {
        // Skip user-declared "bufnum" if we auto-generate it for a single sample.
        if single_sample && p == "bufnum" {
            continue;
        }
        let default: f32 = default_for_param(p).parse().unwrap_or(0.0);
        ctx.builder.add_param(p.clone(), vec![default]);
    }
    for (pname, default) in &extra_params {
        ctx.builder.add_param(pname.clone(), vec![*default]);
    }
    ctx.builder.create_control_ugen();

    let mut env: HashMap<String, Vec<Input>> = HashMap::new();
    for pspec in &ctx.builder.params {
        env.insert(
            pspec.name.clone(),
            vec![Input::Node {
                node_id: 0,
                output_index: pspec.index as u32,
            }],
        );
    }

    let mut sample_idx = 0usize;
    emit(&mut ctx, &mut env, body, buffers, &mut sample_idx)?;

    let ir = GraphIR::from_builder(name.to_string(), ctx.builder);
    encode_synthdef(&ir).map_err(|e| AudionError::RuntimeError {
        msg: format!("SynthDef encoding failed for '{}': {}", name, e),
    })
}

/// `SynthDef(\audion_diskout, { |bufnum=0| DiskOut.ar(bufnum, In.ar(0, 2)) })`
/// — the tiny fixed SynthDef `record_start()` needs, built directly instead
/// of round-tripping through a `define` body.
pub fn build_diskout_synthdef() -> Result<Vec<u8>> {
    let mut ctx = BuildCtx::new();
    ctx.builder.add_param("bufnum".to_string(), vec![0.0]);
    ctx.builder.create_control_ugen();
    let bufnum = Input::Node { node_id: 0, output_index: 0 };
    let input_sig = ctx.node("In", Rate::Audio, vec![konst(0.0)], 2, 0);
    let mut inputs = vec![bufnum];
    inputs.extend(input_sig);
    ctx.node("DiskOut", Rate::Audio, inputs, 1, 0);
    let ir = GraphIR::from_builder("audion_diskout".to_string(), ctx.builder);
    encode_synthdef(&ir).map_err(|e| AudionError::RuntimeError {
        msg: format!("SynthDef encoding failed for 'audion_diskout': {}", e),
    })
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
    // fmx's folded-in drum pitch drop: mult=1 is neutral (no pitch change)
    // so any caller that omits these params gets a plain sustained tone,
    // not an unintended pitch dip from the SC-side fallback default of 0.
    ("perc_pitch_mult", "1"),
    ("perc_pitch_time", "0.05"),
    ("perc_pitch_curve", "-4"),
    // fmx's resonant-LPF + output distortion stage: all neutral/off by
    // default (0), so any caller that omits them gets the plain filter/
    // clean-output behavior fmx had before these were added.
    ("lpf_res", "0"),
    ("drive_amt", "0"),
    ("fold_amt", "0"),
    ("crush_amt", "0"),
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

/// Extract a named numeric argument value (literal Number only).
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

// ---------------------------------------------------------------------------
// Graph construction
// ---------------------------------------------------------------------------

/// Walk a `UGenExpr` tree, building nodes in `ctx` as a side effect, and
/// return the resulting signal — represented as one `Input` per channel
/// (almost always a single-element Vec; multi-channel only for things like
/// a stereo `sample()` or `PlayBuf`).
fn emit(
    ctx: &mut BuildCtx,
    env: &mut HashMap<String, Vec<Input>>,
    expr: &UGenExpr,
    buffers: &[BufferInfo],
    sample_idx: &mut usize,
) -> Result<Vec<Input>> {
    match expr {
        UGenExpr::Number(n) => Ok(vec![konst(*n as f32)]),
        UGenExpr::StringLit(_) => Ok(vec![]), // only meaningful inside sample()'s own handling
        UGenExpr::Param(name) => env.get(name).cloned().ok_or_else(|| AudionError::RuntimeError {
            msg: format!("unknown identifier '{}' in define body", name),
        }),
        UGenExpr::BinOp { left, op, right } => {
            let l = emit(ctx, env, left, buffers, sample_idx)?;
            let r = emit(ctx, env, right, buffers, sample_idx)?;
            let idx = match op {
                BinOp::Add => binop::ADD,
                BinOp::Sub => binop::SUB,
                BinOp::Mul => binop::MUL,
                BinOp::Div => binop::DIV,
                BinOp::Mod => binop::MOD,
                BinOp::Gt => binop::GT,
                BinOp::Lt => binop::LT,
                BinOp::GtEq => binop::GE,
                BinOp::LtEq => binop::LE,
                BinOp::Eq => binop::EQ,
                BinOp::NotEq => binop::NE,
                BinOp::BitAnd => binop::BITAND,
                BinOp::BitOr => binop::BITOR,
                BinOp::BitXor => binop::BITXOR,
                BinOp::LeftShift => binop::SHIFT_LEFT,
                BinOp::RightShift => binop::SHIFT_RIGHT,
                BinOp::Pow => binop::POW,
                // No direct audio-rate meaning for boolean and/or; old
                // sclang codegen fell back to "+" for these too.
                BinOp::And | BinOp::Or => binop::ADD,
            };
            Ok(zip_binop(ctx, idx, &l, &r))
        }
        UGenExpr::Index { object, index } => {
            let obj = emit(ctx, env, object, buffers, sample_idx)?;
            match index.as_ref() {
                UGenExpr::Number(n) => {
                    let i = *n as usize;
                    obj.get(i).copied().map(|v| vec![v]).ok_or_else(|| AudionError::RuntimeError {
                        msg: format!("array index {} out of range (len {})", i, obj.len()),
                    })
                }
                _ => Err(AudionError::RuntimeError {
                    msg: "array index must be a constant number".to_string(),
                }),
            }
        }
        UGenExpr::Block { lets, results } => {
            for (name, value) in lets {
                let v = emit(ctx, env, value, buffers, sample_idx)?;
                env.insert(name.clone(), v);
            }
            let mut last = Vec::new();
            for r in results {
                last = emit(ctx, env, r, buffers, sample_idx)?;
            }
            Ok(last)
        }
        UGenExpr::UGenCall { name, args, named_args } => {
            emit_ugen_call(ctx, env, name, args, named_args, buffers, sample_idx)
        }
    }
}

fn zip_binop(ctx: &mut BuildCtx, special_index: i16, a: &[Input], b: &[Input]) -> Vec<Input> {
    let n = a.len().max(b.len()).max(1);
    (0..n)
        .map(|i| {
            let ai = a.get(i % a.len().max(1)).copied().unwrap_or(Input::Constant(0.0));
            let bi = b.get(i % b.len().max(1)).copied().unwrap_or(Input::Constant(0.0));
            ctx.binop(special_index, ai, bi)
        })
        .collect()
}

fn emit_ugen_call(
    ctx: &mut BuildCtx,
    env: &mut HashMap<String, Vec<Input>>,
    name: &str,
    args: &[UGenExpr],
    named_args: &[(String, UGenExpr)],
    buffers: &[BufferInfo],
    sample_idx: &mut usize,
) -> Result<Vec<Input>> {
    match name {
        "sample" => {
            let out = emit_sample_ugen(ctx, env, named_args, buffers, sample_idx)?;
            // Still recurse into positional args (skipping the file path) to
            // advance sample_idx for any nested samples.
            for a in args.iter().skip(1) {
                emit(ctx, env, a, buffers, sample_idx)?;
            }
            Ok(out)
        }
        "stream_disk" | "stream_disk_variable_rate" => {
            let arg_vals = args
                .iter()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .collect::<Result<Vec<_>>>()?;
            Ok(emit_stream_disk_ugen(ctx, name, &arg_vals, named_args))
        }
        "array" => {
            let mut out = Vec::new();
            for a in args {
                out.extend(emit(ctx, env, a, buffers, sample_idx)?);
            }
            Ok(out)
        }
        "array_get" => {
            let arr = args
                .first()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_default();
            match args.get(1) {
                Some(UGenExpr::Number(n)) => {
                    let i = *n as usize;
                    arr.get(i).copied().map(|v| vec![v]).ok_or_else(|| AudionError::RuntimeError {
                        msg: format!("array_get index {} out of range (len {})", i, arr.len()),
                    })
                }
                _ => Err(AudionError::RuntimeError {
                    msg: "array_get index must be a constant number".to_string(),
                }),
            }
        }
        // Routing UGens that take a whole channel array as one argument -
        // must build a single node with all channels inline as consecutive
        // inputs, not go through the per-channel expansion below.
        "out" => {
            let bus = args
                .first()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .and_then(|v| v.first().copied())
                .unwrap_or_else(|| konst(0.0));
            let sig = args
                .get(1)
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_else(|| vec![konst(0.0)]);
            let mut inputs = vec![bus];
            inputs.extend(sig);
            Ok(ctx.node("Out", Rate::Audio, inputs, 0, 0))
        }
        "local_out" => {
            let sig = args
                .first()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_else(|| vec![konst(0.0)]);
            let rate = ctx.rate_of(&sig);
            Ok(ctx.node("LocalOut", rate, sig, 0, 0))
        }
        "buf_wr" => {
            let sig = args
                .first()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_else(|| vec![konst(0.0)]);
            let bufnum = eval_arg_or(ctx, env, args, 1, buffers, sample_idx, 0.0)?;
            let phase = eval_arg_or(ctx, env, args, 2, buffers, sample_idx, 0.0)?;
            let loop_flag = eval_arg_or(ctx, env, args, 3, buffers, sample_idx, 1.0)?;
            let mut inputs = sig;
            inputs.push(bufnum);
            inputs.push(phase);
            inputs.push(loop_flag);
            Ok(ctx.node("BufWr", Rate::Audio, inputs, 0, 0))
        }
        "record_buf" => {
            let sig = args
                .first()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_else(|| vec![konst(0.0)]);
            let bufnum = eval_arg_or(ctx, env, args, 1, buffers, sample_idx, 0.0)?;
            let offset = eval_arg_or(ctx, env, args, 2, buffers, sample_idx, 0.0)?;
            let rec_level = eval_arg_or(ctx, env, args, 3, buffers, sample_idx, 1.0)?;
            let pre_level = eval_arg_or(ctx, env, args, 4, buffers, sample_idx, 0.0)?;
            let run = eval_arg_or(ctx, env, args, 5, buffers, sample_idx, 1.0)?;
            let loop_flag = eval_arg_or(ctx, env, args, 6, buffers, sample_idx, 1.0)?;
            let mut inputs = sig;
            inputs.extend([bufnum, offset, rec_level, pre_level, run, loop_flag]);
            inputs.push(konst(1.0)); // trigger
            inputs.push(konst(0.0)); // doneAction
            Ok(ctx.node("RecordBuf", Rate::Audio, inputs, 1, 0))
        }
        "splay" => {
            let sig = args
                .first()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_else(|| vec![konst(0.0)]);
            let spread = eval_arg_or(ctx, env, args, 1, buffers, sample_idx, 1.0)?;
            let level = eval_arg_or(ctx, env, args, 2, buffers, sample_idx, 1.0)?;
            let center = eval_arg_or(ctx, env, args, 3, buffers, sample_idx, 0.0)?;
            let mut inputs = sig;
            inputs.extend([spread, level, center, konst(1.0)]); // levelComp=1
            Ok(ctx.node("Splay", Rate::Audio, inputs, 2, 0))
        }
        "klank" => {
            // NOTE: less-common UGen, not currently used anywhere in
            // audion_lib. Best-effort port of Klank's server input layout
            // (input, freqscale, freqoffset, decayscale, then interleaved
            // freq/amp/decay triples) — verify against real scsynth output
            // before relying on it.
            let input = args
                .first()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .and_then(|v| v.first().copied())
                .unwrap_or_else(|| konst(0.0));
            let freqs = args
                .get(1)
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_default();
            let amps = args
                .get(2)
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_default();
            let rings = args
                .get(3)
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .transpose()?
                .unwrap_or_default();
            let mut inputs = vec![input, konst(1.0), konst(0.0), konst(1.0)];
            let n = freqs.len().max(amps.len()).max(rings.len());
            for i in 0..n {
                inputs.push(freqs.get(i).copied().unwrap_or_else(|| konst(0.0)));
                inputs.push(amps.get(i).copied().unwrap_or_else(|| konst(1.0)));
                inputs.push(rings.get(i).copied().unwrap_or_else(|| konst(1.0)));
            }
            Ok(vec![ctx.node1("Klank", Rate::Audio, inputs, 0)])
        }
        _ => {
            let arg_vals = args
                .iter()
                .map(|a| emit(ctx, env, a, buffers, sample_idx))
                .collect::<Result<Vec<_>>>()?;
            Ok(ugen_call(ctx, name, &arg_vals))
        }
    }
}

/// Evaluate `args[idx]` if present (returning its first channel), else a constant default.
fn eval_arg_or(
    ctx: &mut BuildCtx,
    env: &mut HashMap<String, Vec<Input>>,
    args: &[UGenExpr],
    idx: usize,
    buffers: &[BufferInfo],
    sample_idx: &mut usize,
    default: f32,
) -> Result<Input> {
    match args.get(idx) {
        Some(a) => {
            let v = emit(ctx, env, a, buffers, sample_idx)?;
            Ok(v.first().copied().unwrap_or_else(|| konst(default)))
        }
        None => Ok(konst(default)),
    }
}

/// Dispatch a UGen call across channels: SC-style multichannel expansion —
/// if any argument has more than one channel, the whole UGen is replicated
/// once per channel (shorter arguments cycle).
fn ugen_call(ctx: &mut BuildCtx, name: &str, args: &[Vec<Input>]) -> Vec<Input> {
    let n = args.iter().map(|a| a.len().max(1)).max().unwrap_or(1);
    if n <= 1 {
        let scalar: Vec<Input> = args
            .iter()
            .map(|a| a.first().copied().unwrap_or(Input::Constant(0.0)))
            .collect();
        return ugen_call_scalar(ctx, name, &scalar);
    }
    let mut out = Vec::new();
    for i in 0..n {
        let scalar: Vec<Input> = args
            .iter()
            .map(|a| {
                if a.is_empty() {
                    Input::Constant(0.0)
                } else {
                    a[i % a.len()]
                }
            })
            .collect();
        out.extend(ugen_call_scalar(ctx, name, &scalar));
    }
    out
}

/// Emit a `sample()` UGen call: root/velocity/key-range gated buffer playback.
fn emit_sample_ugen(
    ctx: &mut BuildCtx,
    env: &mut HashMap<String, Vec<Input>>,
    named_args: &[(String, UGenExpr)],
    buffers: &[BufferInfo],
    sample_idx: &mut usize,
) -> Result<Vec<Input>> {
    let idx = *sample_idx;
    *sample_idx += 1;

    let num_ch = buffers.get(idx).map(|b| b.num_channels).unwrap_or(2);
    let bufnum_name = if buffers.len() == 1 {
        "bufnum".to_string()
    } else {
        format!("bufnum_{}", idx)
    };
    let bufnum = env
        .get(&bufnum_name)
        .and_then(|v| v.first().copied())
        .unwrap_or_else(|| konst(0.0));

    let root = get_named_number(named_args, "root").unwrap_or(60.0);
    let vel_lo = get_named_number(named_args, "vel_lo").unwrap_or(0.0);
    let vel_hi = get_named_number(named_args, "vel_hi").unwrap_or(127.0);
    let key_lo = get_named_number(named_args, "key_lo").unwrap_or(0.0);
    let key_hi = get_named_number(named_args, "key_hi").unwrap_or(127.0);
    let loop_flag = get_named_number(named_args, "loop").unwrap_or(0.0);
    let loop_end_num = get_named_number(named_args, "loop_end").unwrap_or(0.0);
    let detune = get_named_number(named_args, "detune").unwrap_or(0.0);

    let loop_start_in = named_input(ctx, env, named_args, "loop_start", buffers, sample_idx)?
        .unwrap_or_else(|| konst(0.0));
    let loop_end_in = named_input(ctx, env, named_args, "loop_end", buffers, sample_idx)?;
    let start_in = named_input(ctx, env, named_args, "start", buffers, sample_idx)?
        .unwrap_or_else(|| konst(0.0));

    // root frequency from MIDI note: 440 * 2^((root-69)/12)
    let root_hz = 440.0 * (2.0_f64).powf((root - 69.0) / 12.0);
    // detune multiplier: 2^(cents/1200)
    let detune_mult = if detune != 0.0 { (2.0_f64).powf(detune / 1200.0) } else { 1.0 };

    let freq = env.get("freq").and_then(|v| v.first().copied()).unwrap_or_else(|| konst(440.0));
    let root_hz_c = konst(root_hz as f32);
    let mut rate_expr = ctx.binop(binop::DIV, freq, root_hz_c);
    if detune_mult != 1.0 {
        let dm = konst(detune_mult as f32);
        rate_expr = ctx.binop(binop::MUL, rate_expr, dm);
    }
    let buf_rate_scale = ctx.node1("BufRateScale", Rate::Control, vec![bufnum], 0);
    let rate_expr = ctx.binop(binop::MUL, rate_expr, buf_rate_scale);

    let use_phasor = loop_flag != 0.0 && (loop_end_num > 0.0 || loop_end_in.is_some());
    let loop_end_final = loop_end_in.unwrap_or_else(|| konst(0.0));

    let playback: Vec<Input> = if use_phasor {
        // Precise loop points with BufRd + Phasor.
        // Phasor.ar(trig, rate, start, end, resetPos)
        let phasor = ctx.node1(
            "Phasor",
            Rate::Audio,
            vec![konst(0.0), rate_expr, loop_start_in, loop_end_final, konst(0.0)],
            0,
        );
        // BufRd.ar real inputs: [bufnum, phase, loop, interpolation]; numChannels is shape-only.
        ctx.node(
            "BufRd",
            Rate::Audio,
            vec![bufnum, phasor, konst(0.0), konst(4.0)],
            num_ch,
            0,
        )
    } else {
        // PlayBuf real inputs: [bufnum, rate, trigger, startPos, loop, doneAction]; numChannels is shape-only.
        let loop_int = if loop_flag != 0.0 { 1.0 } else { 0.0 };
        ctx.node(
            "PlayBuf",
            Rate::Audio,
            vec![
                bufnum,
                rate_expr,
                konst(1.0),
                start_in,
                konst(loop_int),
                konst(0.0),
            ],
            num_ch,
            0,
        )
    };

    let has_vel_range = vel_lo != 0.0 || vel_hi != 127.0;
    let has_key_range = key_lo != 0.0 || key_hi != 127.0;

    let mut result = playback;

    if has_vel_range {
        let vel = env.get("vel").and_then(|v| v.first().copied()).unwrap_or_else(|| konst(127.0));
        let ge = ctx.binop(binop::GE, vel, konst(vel_lo as f32));
        let le = ctx.binop(binop::LE, vel, konst(vel_hi as f32));
        let gate = ctx.binop(binop::MUL, ge, le);
        result = result.into_iter().map(|c| ctx.binop(binop::MUL, c, gate)).collect();
    }

    if has_key_range {
        // freq.cpsmidi (UnaryOpUGen has no direct cpsmidi selector in the
        // table we verified, so derive it: midi = log2(freq/440)*12 + 69)
        let ratio = ctx.binop(binop::DIV, freq, konst(440.0));
        let log2 = ctx.unop(unop::LOG2, ratio);
        let scaled = ctx.binop(binop::MUL, log2, konst(12.0));
        let midi = ctx.binop(binop::ADD, scaled, konst(69.0));
        let ge = ctx.binop(binop::GE, midi, konst(key_lo as f32));
        let le = ctx.binop(binop::LE, midi, konst(key_hi as f32));
        let gate = ctx.binop(binop::MUL, ge, le);
        result = result.into_iter().map(|c| ctx.binop(binop::MUL, c, gate)).collect();
    }

    Ok(result)
}

/// Evaluate a named arg (if present) through the full expression walker,
/// returning its first channel.
fn named_input(
    ctx: &mut BuildCtx,
    env: &mut HashMap<String, Vec<Input>>,
    named_args: &[(String, UGenExpr)],
    key: &str,
    buffers: &[BufferInfo],
    sample_idx: &mut usize,
) -> Result<Option<Input>> {
    match named_args.iter().find(|(n, _)| n == key) {
        Some((_, expr)) => {
            let v = emit(ctx, env, expr, buffers, sample_idx)?;
            Ok(v.first().copied())
        }
        None => Ok(None),
    }
}

fn emit_stream_disk_ugen(
    ctx: &mut BuildCtx,
    name: &str,
    args: &[Vec<Input>],
    named_args: &[(String, UGenExpr)],
) -> Vec<Input> {
    let channels = get_named_number(named_args, "channels").unwrap_or(2.0) as u32;
    let loop_flag = get_named_number(named_args, "loop").unwrap_or(0.0) as f32;

    if name == "stream_disk_variable_rate" {
        let bufnum = args.first().and_then(|v| v.first().copied()).unwrap_or(Input::Constant(0.0));
        let rate = args.get(1).and_then(|v| v.first().copied()).unwrap_or_else(|| konst(1.0));
        // VDiskIn real inputs: [bufnum, rate, loop, sendID]; numChannels is shape-only.
        ctx.node(
            "VDiskIn",
            Rate::Audio,
            vec![bufnum, rate, konst(loop_flag), konst(0.0)],
            channels,
            0,
        )
    } else {
        let bufnum = args.first().and_then(|v| v.first().copied()).unwrap_or(Input::Constant(0.0));
        // DiskIn real inputs: [bufnum, loop]; numChannels is shape-only.
        ctx.node("DiskIn", Rate::Audio, vec![bufnum, konst(loop_flag)], channels, 0)
    }
}

/// The big table: one UGen call, already channel-expanded to scalar
/// inputs. Mirrors the old sclang-text `emit_ugen_call` 1:1, just building
/// graph nodes instead of formatting SC source.
fn ugen_call_scalar(ctx: &mut BuildCtx, name: &str, args: &[Input]) -> Vec<Input> {
    let a = |i: usize, d: f32| arg_or(args, i, d);
    match name {
        // Oscillators
        "sine" => vec![ctx.node1("SinOsc", Rate::Audio, vec![a(0, 440.0), konst(0.0)], 0)],
        "saw" => vec![ctx.node1("Saw", Rate::Audio, vec![a(0, 440.0)], 0)],
        "square" | "pulse" => {
            let freq = a(0, 440.0);
            let width = a(1, 0.5);
            vec![ctx.node1("Pulse", Rate::Audio, vec![freq, width], 0)]
        }
        "tri" => vec![ctx.node1("LFTri", Rate::Audio, vec![a(0, 440.0), konst(0.0)], 0)],
        "noise" | "white" => vec![ctx.node1("WhiteNoise", Rate::Audio, vec![], 0)],
        "pink" => vec![ctx.node1("PinkNoise", Rate::Audio, vec![], 0)],
        "brown" => vec![ctx.node1("BrownNoise", Rate::Audio, vec![], 0)],
        "gray" => vec![ctx.node1("GrayNoise", Rate::Audio, vec![], 0)],
        "clip_noise" => vec![ctx.node1("ClipNoise", Rate::Audio, vec![], 0)],

        "blip" => {
            let freq = a(0, 440.0);
            let numharm = a(1, 200.0);
            vec![ctx.node1("Blip", Rate::Audio, vec![freq, numharm], 0)]
        }
        "var_saw" => {
            let freq = a(0, 440.0);
            let width = a(1, 0.0);
            vec![ctx.node1("VarSaw", Rate::Audio, vec![freq, konst(0.0), width], 0)]
        }
        "sync_saw" => {
            let sf = a(0, 440.0);
            let saw = a(1, 440.0);
            vec![ctx.node1("SyncSaw", Rate::Audio, vec![sf, saw], 0)]
        }
        "fsin_osc" => vec![ctx.node1("FSinOsc", Rate::Audio, vec![a(0, 440.0), konst(0.0)], 0)],
        "lf_par" => vec![ctx.node1("LFPar", Rate::Audio, vec![a(0, 1.0), konst(0.0)], 0)],
        "lf_cub" => vec![ctx.node1("LFCub", Rate::Audio, vec![a(0, 1.0), konst(0.0)], 0)],
        "pm_osc" => {
            let carfreq = a(0, 440.0);
            let modfreq = a(1, 440.0);
            let pmindex = a(2, 0.0);
            let modphase = a(3, 0.0);
            vec![ctx.node1("PMOsc", Rate::Audio, vec![carfreq, modfreq, pmindex, modphase], 0)]
        }

        // Filters
        "lpf" => {
            let sig = a(0, 0.0);
            let cutoff = a(1, 1000.0);
            vec![ctx.node1("LPF", Rate::Audio, vec![sig, cutoff], 0)]
        }
        "hpf" => {
            let sig = a(0, 0.0);
            let cutoff = a(1, 1000.0);
            vec![ctx.node1("HPF", Rate::Audio, vec![sig, cutoff], 0)]
        }
        "bpf" => {
            let sig = a(0, 0.0);
            let freq = a(1, 1000.0);
            let rq = a(2, 1.0);
            vec![ctx.node1("BPF", Rate::Audio, vec![sig, freq, rq], 0)]
        }
        "rlpf" => {
            let sig = a(0, 0.0);
            let cutoff = a(1, 1000.0);
            let rq = a(2, 1.0);
            vec![ctx.node1("RLPF", Rate::Audio, vec![sig, cutoff, rq], 0)]
        }
        "rhpf" => {
            let sig = a(0, 0.0);
            let cutoff = a(1, 1000.0);
            let rq = a(2, 1.0);
            vec![ctx.node1("RHPF", Rate::Audio, vec![sig, cutoff, rq], 0)]
        }

        // Envelope: env(gate) / env(gate, atk, sus, rel)
        "env" => {
            let gate = a(0, 1.0);
            let atk = a(1, 0.01);
            let sus_is_zero = matches!(args.get(2), Some(Input::Constant(v)) if *v == 0.0);
            let rel = a(3, 0.3);
            if sus_is_zero {
                vec![emit_env_gen(ctx, EnvShape::Perc(atk, rel, konst(-4.0)), gate, 2.0)]
            } else {
                let sus = a(2, 1.0);
                vec![emit_env_gen(ctx, EnvShape::Asr(atk, sus, rel), gate, 2.0)]
            }
        }
        "line" => {
            let start = a(0, 0.0);
            let end = a(1, 1.0);
            let dur = a(2, 1.0);
            vec![ctx.node1("Line", Rate::Audio, vec![start, end, dur, konst(0.0)], 0)]
        }
        "xline" => {
            let start = a(0, 0.01);
            let end = a(1, 1.0);
            let dur = a(2, 1.0);
            vec![ctx.node1("XLine", Rate::Audio, vec![start, end, dur, konst(0.0)], 0)]
        }
        "decay" => {
            let sig = a(0, 0.0);
            let time = a(1, 1.0);
            let mult = a(2, 1.0);
            let add = a(3, 0.0);
            let decayed = ctx.node1("Decay", Rate::Audio, vec![sig, time], 0);
            let scaled = ctx.binop(binop::MUL, decayed, mult);
            vec![ctx.binop(binop::ADD, scaled, add)]
        }
        "decay2" => {
            let sig = a(0, 0.0);
            let atk = a(1, 0.01);
            let dec = a(2, 1.0);
            vec![ctx.node1("Decay2", Rate::Audio, vec![sig, atk, dec], 0)]
        }
        "env_perc" => {
            let gate = a(0, 1.0);
            let atk = a(1, 0.01);
            let rel = a(2, 0.3);
            let curve = a(3, -4.0);
            let done_action = args.get(4).copied().unwrap_or_else(|| konst(2.0));
            vec![emit_env_gen(ctx, EnvShape::Perc(atk, rel, curve), gate, done_action_f(done_action))]
        }
        "linen" => {
            let gate = a(0, 1.0);
            let atk = a(1, 0.01);
            let sus = a(2, 1.0);
            let rel = a(3, 0.3);
            vec![ctx.node1("Linen", Rate::Control, vec![gate, atk, sus, rel, konst(2.0)], 0)]
        }

        // LFOs (control rate)
        "lfo_sine" => vec![ctx.node1("SinOsc", Rate::Control, vec![a(0, 1.0), konst(0.0)], 0)],
        "lfo_saw" => vec![ctx.node1("LFSaw", Rate::Control, vec![a(0, 1.0), konst(0.0)], 0)],
        "lfo_tri" => vec![ctx.node1("LFTri", Rate::Control, vec![a(0, 1.0), konst(0.0)], 0)],
        "lfo_pulse" => {
            let freq = a(0, 1.0);
            let width = a(1, 0.5);
            vec![ctx.node1("LFPulse", Rate::Control, vec![freq, konst(0.0), width], 0)]
        }
        "lfo_noise" => vec![ctx.node1("LFNoise1", Rate::Control, vec![a(0, 1.0)], 0)],
        "lfo_step" => vec![ctx.node1("LFNoise0", Rate::Control, vec![a(0, 1.0)], 0)],
        "lfo_noise2" => vec![ctx.node1("LFNoise2", Rate::Control, vec![a(0, 1.0)], 0)],

        "dust2" => vec![ctx.node1("Dust2", Rate::Audio, vec![a(0, 10.0)], 0)],
        "crackle" => vec![ctx.node1("Crackle", Rate::Audio, vec![a(0, 1.5)], 0)],
        "coin_gate" => {
            let prob = a(0, 0.5);
            let trig = if args.len() > 1 {
                a(1, 0.0)
            } else {
                ctx.node1("Impulse", Rate::Audio, vec![konst(1.0), konst(0.0)], 0)
            };
            vec![ctx.node1("CoinGate", Rate::Audio, vec![prob, trig], 0)]
        }

        // Effects
        "reverb" => {
            let sig = a(0, 0.0);
            let mix = a(1, 0.33);
            let room = a(2, 0.5);
            let damp = a(3, 0.5);
            vec![ctx.node1("FreeVerb", Rate::Audio, vec![sig, mix, room, damp], 0)]
        }
        "freeverb2" => {
            let sig = a(0, 0.0);
            let sig2 = a(1, 0.0);
            let mix = a(2, 0.33);
            let room = a(3, 0.5);
            let damp = a(4, 0.5);
            vec![ctx.node1("FreeVerb2", Rate::Audio, vec![sig, sig2, mix, room, damp], 0)]
        }
        "gverb" => {
            let sig = a(0, 0.0);
            let roomsize = a(1, 10.0);
            let revtime = a(2, 3.0);
            let damping = a(3, 0.5);
            let inputbw = a(4, 0.5);
            let spread = a(5, 15.0);
            let drylevel = a(6, 1.0);
            let earlylevel = a(7, 0.7);
            let taillevel = a(8, 0.5);
            let maxroomsize = konst(300.0);
            vec![ctx.node1(
                "GVerb",
                Rate::Audio,
                vec![sig, roomsize, revtime, damping, inputbw, spread, drylevel, earlylevel, taillevel, maxroomsize],
                0,
            )]
        }
        "delay" => {
            let sig = a(0, 0.0);
            let time = a(1, 0.2);
            let decay = a(2, 1.0);
            vec![ctx.node1("CombL", Rate::Audio, vec![sig, time, time, decay], 0)]
        }
        "delay_c" => {
            let sig = a(0, 0.0);
            let maxtime = a(1, 0.2);
            let time = a(2, 0.2);
            vec![ctx.node1("DelayC", Rate::Audio, vec![sig, maxtime, time], 0)]
        }
        "local_in" => {
            // LocalIn.ar(numChannels, default=0.0): numChannels is client-side
            // only (sets output count); real server inputs are `default`
            // wrap-extended to numChannels values (SCClassLibrary InOut.sc).
            let channels = args.first().and_then(|v| if let Input::Constant(c) = v { Some(*c as u32) } else { None }).unwrap_or(1).max(1);
            let defaults: Vec<Input> = (0..channels).map(|_| konst(0.0)).collect();
            ctx.node("LocalIn", Rate::Audio, defaults, channels, 0)
        }

        // Allpass filters
        "allpass_n" => {
            let sig = a(0, 0.0);
            let maxtime = a(1, 0.2);
            let time = a(2, 0.2);
            let decay = a(3, 1.0);
            vec![ctx.node1("AllpassN", Rate::Audio, vec![sig, maxtime, time, decay], 0)]
        }
        "allpass_l" => {
            let sig = a(0, 0.0);
            let maxtime = a(1, 0.2);
            let time = a(2, 0.2);
            let decay = a(3, 1.0);
            vec![ctx.node1("AllpassL", Rate::Audio, vec![sig, maxtime, time, decay], 0)]
        }
        "allpass_c" => {
            let sig = a(0, 0.0);
            let maxtime = a(1, 0.2);
            let time = a(2, 0.2);
            let decay = a(3, 1.0);
            vec![ctx.node1("AllpassC", Rate::Audio, vec![sig, maxtime, time, decay], 0)]
        }

        "resonz" => {
            let sig = a(0, 0.0);
            let freq = a(1, 440.0);
            let bwr = a(2, 0.1);
            vec![ctx.node1("Resonz", Rate::Audio, vec![sig, freq, bwr], 0)]
        }
        "moog_ff" => {
            let sig = a(0, 0.0);
            let freq = a(1, 1000.0);
            let gain = a(2, 2.0);
            vec![ctx.node1("MoogFF", Rate::Audio, vec![sig, freq, gain, konst(0.0)], 0)]
        }
        "brf" => {
            let sig = a(0, 0.0);
            let freq = a(1, 1000.0);
            let rq = a(2, 1.0);
            vec![ctx.node1("BRF", Rate::Audio, vec![sig, freq, rq], 0)]
        }
        "formlet" => {
            let sig = a(0, 0.0);
            let freq = a(1, 1000.0);
            let attack = a(2, 0.005);
            let decay = a(3, 0.04);
            vec![ctx.node1("Formlet", Rate::Audio, vec![sig, freq, attack, decay], 0)]
        }
        "lag" => {
            let sig = a(0, 0.0);
            let lagtime = a(1, 0.1);
            vec![ctx.node1("Lag", Rate::Audio, vec![sig, lagtime], 0)]
        }
        "lag2" => {
            let sig = a(0, 0.0);
            let lagtime = a(1, 0.1);
            vec![ctx.node1("Lag2", Rate::Audio, vec![sig, lagtime], 0)]
        }
        "leak_dc" => {
            let sig = a(0, 0.0);
            let coef = a(1, 0.995);
            vec![ctx.node1("LeakDC", Rate::Audio, vec![sig, coef], 0)]
        }
        "ringz" => {
            let sig = a(0, 0.0);
            let freq = a(1, 440.0);
            let decay = a(2, 1.0);
            vec![ctx.node1("Ringz", Rate::Audio, vec![sig, freq, decay], 0)]
        }
        "one_pole" => {
            let sig = a(0, 0.0);
            let coef = a(1, 0.5);
            vec![ctx.node1("OnePole", Rate::Audio, vec![sig, coef], 0)]
        }
        "two_pole" => {
            let sig = a(0, 0.0);
            let freq = a(1, 1000.0);
            let radius = a(2, 0.8);
            vec![ctx.node1("TwoPole", Rate::Audio, vec![sig, freq, radius], 0)]
        }
        "ramp" => {
            let sig = a(0, 0.0);
            let lagtime = a(1, 0.1);
            vec![ctx.node1("Ramp", Rate::Audio, vec![sig, lagtime], 0)]
        }
        "hpz1" => vec![ctx.node1("HPZ1", Rate::Audio, vec![a(0, 0.0)], 0)],
        "lpz1" => vec![ctx.node1("LPZ1", Rate::Audio, vec![a(0, 0.0)], 0)],
        "hpz2" => vec![ctx.node1("HPZ2", Rate::Audio, vec![a(0, 0.0)], 0)],
        "lpz2" => vec![ctx.node1("LPZ2", Rate::Audio, vec![a(0, 0.0)], 0)],
        "mid_eq" => {
            let sig = a(0, 0.0);
            let freq = a(1, 1000.0);
            let rq = a(2, 1.0);
            let db = a(3, 0.0);
            vec![ctx.node1("MidEQ", Rate::Audio, vec![sig, freq, rq, db], 0)]
        }
        "slew" => {
            let sig = a(0, 0.0);
            let up = a(1, 1.0);
            let dn = a(2, 1.0);
            vec![ctx.node1("Slew", Rate::Audio, vec![sig, up, dn], 0)]
        }

        "delay_n" => {
            let sig = a(0, 0.0);
            let maxtime = a(1, 0.2);
            let time = a(2, 0.2);
            vec![ctx.node1("DelayN", Rate::Audio, vec![sig, maxtime, time], 0)]
        }
        "delay_l" => {
            let sig = a(0, 0.0);
            let maxtime = a(1, 0.2);
            let time = a(2, 0.2);
            vec![ctx.node1("DelayL", Rate::Audio, vec![sig, maxtime, time], 0)]
        }
        "comb_n" => {
            let sig = a(0, 0.0);
            let maxtime = a(1, 0.2);
            let time = a(2, 0.2);
            let decay = a(3, 1.0);
            vec![ctx.node1("CombN", Rate::Audio, vec![sig, maxtime, time, decay], 0)]
        }
        "comb_c" => {
            let sig = a(0, 0.0);
            let maxtime = a(1, 0.2);
            let time = a(2, 0.2);
            let decay = a(3, 1.0);
            vec![ctx.node1("CombC", Rate::Audio, vec![sig, maxtime, time, decay], 0)]
        }
        "delay1" => vec![ctx.node1("Delay1", Rate::Audio, vec![a(0, 0.0)], 0)],
        "delay2" => vec![ctx.node1("Delay2", Rate::Audio, vec![a(0, 0.0)], 0)],
        "pluck" => {
            let sig = a(0, 0.0);
            let trig = a(1, 1.0);
            let maxtime = a(2, 0.1);
            let time = a(3, 0.1);
            let decay = a(4, 5.0);
            let coef = a(5, 0.5);
            vec![ctx.node1("Pluck", Rate::Audio, vec![sig, trig, maxtime, time, decay, coef], 0)]
        }

        // Distortion / dynamics
        "tanh" => vec![ctx.unop(unop::TANH, a(0, 0.0))],
        "atan" => vec![ctx.unop(unop::ARC_TAN, a(0, 0.0))],
        "wrap" => {
            let sig = a(0, 0.0);
            let lo = a(1, -1.0);
            let hi = a(2, 1.0);
            let rate = ctx.rate_of(&[sig, lo, hi]);
            vec![ctx.node1("Wrap", rate, vec![sig, lo, hi], 0)]
        }
        "fold" => {
            let sig = a(0, 0.0);
            let lo = a(1, -1.0);
            let hi = a(2, 1.0);
            let rate = ctx.rate_of(&[sig, lo, hi]);
            vec![ctx.node1("Fold", rate, vec![sig, lo, hi], 0)]
        }
        "softclip" => vec![ctx.unop(unop::SOFT_CLIP, a(0, 0.0))],
        "dist" => {
            let sig = a(0, 0.0);
            let amount = a(1, 2.0);
            let scaled = ctx.binop(binop::MUL, sig, amount);
            vec![ctx.binop(binop::CLIP2, scaled, konst(1.0))]
        }

        "compander" => {
            let sig = a(0, 0.0);
            let control = args.get(1).copied().unwrap_or(sig);
            let thresh = a(2, 0.5);
            let slope_below = a(3, 1.0);
            let slope_above = a(4, 0.5);
            let clamp = a(5, 0.01);
            let relax = a(6, 0.1);
            vec![ctx.node1("Compander", Rate::Audio, vec![sig, control, thresh, slope_below, slope_above, clamp, relax], 0)]
        }
        "limiter" => {
            let sig = a(0, 0.0);
            let level = a(1, 1.0);
            let dur = a(2, 0.01);
            vec![ctx.node1("Limiter", Rate::Audio, vec![sig, level, dur], 0)]
        }
        "amplitude" => {
            let sig = a(0, 0.0);
            let attack = a(1, 0.01);
            let release = a(2, 0.01);
            vec![ctx.node1("Amplitude", Rate::Audio, vec![sig, attack, release], 0)]
        }
        "normalizer" => {
            let sig = a(0, 0.0);
            let level = a(1, 1.0);
            let dur = a(2, 0.01);
            vec![ctx.node1("Normalizer", Rate::Audio, vec![sig, level, dur], 0)]
        }

        // Pitch/Frequency effects
        "pitch_shift" => {
            let sig = a(0, 0.0);
            let winsize = a(1, 0.2);
            let ratio = a(2, 1.0);
            let disp = a(3, 0.0);
            let time_disp = a(4, 0.0);
            vec![ctx.node1("PitchShift", Rate::Audio, vec![sig, winsize, ratio, disp, time_disp], 0)]
        }
        "freq_shift" => {
            let sig = a(0, 0.0);
            let freq = a(1, 0.0);
            let phase = a(2, 0.0);
            vec![ctx.node1("FreqShift", Rate::Audio, vec![sig, freq, phase], 0)]
        }
        "pitch" => {
            let sig = a(0, 0.0);
            let init = a(1, 440.0);
            let minf = a(2, 60.0);
            let maxf = a(3, 4000.0);
            vec![ctx.node1(
                "Pitch",
                Rate::Control,
                vec![sig, init, minf, maxf, konst(100.0), konst(16.0), konst(1.0), konst(0.01), konst(0.5), konst(1.0)],
                0,
            )]
        }
        "vibrato" => {
            let freq = a(0, 440.0);
            let rate = a(1, 6.0);
            let depth = a(2, 0.02);
            let delay = a(3, 0.0);
            let onset = a(4, 0.0);
            let ratevar = a(5, 0.04);
            let depthvar = a(6, 0.1);
            vec![ctx.node1(
                "Vibrato",
                Rate::Audio,
                vec![freq, rate, depth, delay, onset, ratevar, depthvar, konst(0.0), konst(0.0)],
                0,
            )]
        }

        // Analysis & control
        "running_sum" => {
            let sig = a(0, 0.0);
            let num = a(1, 40.0);
            vec![ctx.node1("RunningSum", Rate::Audio, vec![sig, num], 0)]
        }
        "median" => {
            let length = a(0, 3.0);
            let sig = a(1, 0.0);
            vec![ctx.node1("Median", Rate::Audio, vec![length, sig], 0)]
        }
        "running_max" => {
            let sig = a(0, 0.0);
            let num = a(1, 40.0);
            vec![ctx.node1("RunningMax", Rate::Audio, vec![sig, num], 0)]
        }
        "running_min" => {
            let sig = a(0, 0.0);
            let num = a(1, 40.0);
            vec![ctx.node1("RunningMin", Rate::Audio, vec![sig, num], 0)]
        }
        "peak" => {
            let sig = a(0, 0.0);
            let trig = if args.len() > 1 {
                a(1, 0.0)
            } else {
                ctx.node1("Impulse", Rate::Audio, vec![konst(10.0), konst(0.0)], 0)
            };
            vec![ctx.node1("Peak", Rate::Audio, vec![sig, trig], 0)]
        }
        "zero_crossing" => vec![ctx.node1("ZeroCrossing", Rate::Audio, vec![a(0, 0.0)], 0)],

        "latch" => {
            let sig = a(0, 0.0);
            let trig = if args.len() > 1 {
                a(1, 0.0)
            } else {
                ctx.node1("Impulse", Rate::Audio, vec![konst(1.0), konst(0.0)], 0)
            };
            vec![ctx.node1("Latch", Rate::Audio, vec![sig, trig], 0)]
        }
        "gate" => {
            let sig = a(0, 0.0);
            let trig = a(1, 0.0);
            vec![ctx.node1("Gate", Rate::Audio, vec![sig, trig], 0)]
        }
        "pulse_count" => {
            let trig = if !args.is_empty() {
                a(0, 0.0)
            } else {
                ctx.node1("Impulse", Rate::Audio, vec![konst(1.0), konst(0.0)], 0)
            };
            let reset = a(1, 0.0);
            vec![ctx.node1("PulseCount", Rate::Audio, vec![trig, reset], 0)]
        }
        "t_exprand" => {
            let lo = a(0, 0.01);
            let hi = a(1, 1.0);
            let trig = if args.len() > 2 {
                a(2, 0.0)
            } else {
                ctx.node1("Impulse", Rate::Audio, vec![konst(1.0), konst(0.0)], 0)
            };
            vec![ctx.node1("TExpRand", Rate::Audio, vec![lo, hi, trig], 0)]
        }
        "t_irand" => {
            let lo = a(0, 0.0);
            let hi = a(1, 127.0);
            let trig = if args.len() > 2 {
                a(2, 0.0)
            } else {
                ctx.node1("Impulse", Rate::Audio, vec![konst(1.0), konst(0.0)], 0)
            };
            vec![ctx.node1("TIRand", Rate::Audio, vec![lo, hi, trig], 0)]
        }
        "sweep" => {
            let trig = if !args.is_empty() {
                a(0, 0.0)
            } else {
                ctx.node1("Impulse", Rate::Audio, vec![konst(1.0), konst(0.0)], 0)
            };
            let rate = a(1, 1.0);
            vec![ctx.node1("Sweep", Rate::Audio, vec![trig, rate], 0)]
        }

        // Input/Output
        "in" => {
            let bus = a(0, 0.0);
            let channels = args.get(1).and_then(|v| if let Input::Constant(c) = v { Some(*c as u32) } else { None }).unwrap_or(1);
            ctx.node("In", Rate::Audio, vec![bus], channels, 0)
        }
        "pan" => {
            let pos = a(0, 0.0);
            let sig = a(1, 0.0);
            ctx.node("Pan2", Rate::Audio, vec![sig, pos, konst(1.0)], 2, 0)
        }
        "pan4" => {
            let sig = a(0, 0.0);
            let xpos = a(1, 0.0);
            let ypos = a(2, 0.0);
            let level = a(3, 1.0);
            ctx.node("Pan4", Rate::Audio, vec![sig, xpos, ypos, level], 4, 0)
        }
        "balance2" => {
            let left = a(0, 0.0);
            let right = a(1, 0.0);
            let pos = a(2, 0.0);
            let level = a(3, 1.0);
            ctx.node("Balance2", Rate::Audio, vec![left, right, pos, level], 2, 0)
        }

        // Buffer playback
        "buf_rate_scale" => vec![ctx.node1("BufRateScale", Rate::Control, vec![a(0, 0.0)], 0)],
        "phasor" => {
            let trig = a(0, 0.0);
            let rate = a(1, 1.0);
            let start = a(2, 0.0);
            let end = a(3, 44100.0);
            vec![ctx.node1("Phasor", Rate::Audio, vec![trig, rate, start, end, konst(0.0)], 0)]
        }
        "buf_rd" => {
            let numchans = args.first().and_then(|v| if let Input::Constant(c) = v { Some(*c as u32) } else { None }).unwrap_or(2);
            let bufnum = a(1, 0.0);
            let phase = a(2, 0.0);
            let interp = a(3, 4.0);
            ctx.node("BufRd", Rate::Audio, vec![bufnum, phase, konst(1.0), interp], numchans, 0)
        }
        "PlayBuf" => {
            let numchans = args.first().and_then(|v| if let Input::Constant(c) = v { Some(*c as u32) } else { None }).unwrap_or(2);
            let bufnum = a(1, 0.0);
            let rate = a(2, 1.0);
            let trig = a(3, 1.0);
            let startpos = a(4, 0.0);
            let looping = a(5, 0.0);
            ctx.node("PlayBuf", Rate::Audio, vec![bufnum, rate, trig, startpos, looping, konst(0.0)], numchans, 0)
        }
        "local_buf" => {
            let frames = a(0, 2048.0);
            let channels = a(1, 1.0);
            vec![ctx.node1("LocalBuf", Rate::Scalar, vec![channels, frames], 0)]
        }

        // Granular synthesis
        "Dust" => vec![ctx.node1("Dust", Rate::Audio, vec![a(0, 10.0)], 0)],
        "Impulse" => vec![ctx.node1("Impulse", Rate::Audio, vec![a(0, 1.0), konst(0.0)], 0)],
        "TRand" => {
            let lo = a(0, 0.0);
            let hi = a(1, 1.0);
            let trig = if args.len() > 2 {
                a(2, 0.0)
            } else {
                ctx.node1("Impulse", Rate::Audio, vec![konst(1.0), konst(0.0)], 0)
            };
            vec![ctx.node1("TRand", Rate::Audio, vec![lo, hi, trig], 0)]
        }
        "GrainBuf" => {
            let numchans = args.first().and_then(|v| if let Input::Constant(c) = v { Some(*c as u32) } else { None }).unwrap_or(2);
            let trig = a(1, 10.0);
            let dur = a(2, 0.1);
            let sndbuf = a(3, 0.0);
            let rate = a(4, 1.0);
            let pos = a(5, 0.0);
            let interp = a(6, 2.0);
            let pan = a(7, 0.0);
            let envbuf = a(8, -1.0);
            let maxgrains = a(9, 512.0);
            ctx.node("GrainBuf", Rate::Audio, vec![trig, dur, sndbuf, rate, pos, interp, pan, envbuf, maxgrains], numchans, 0)
        }
        "GrainSin" => {
            let numchans = args.first().and_then(|v| if let Input::Constant(c) = v { Some(*c as u32) } else { None }).unwrap_or(2);
            let trig = a(1, 10.0);
            let dur = a(2, 0.1);
            let freq = a(3, 440.0);
            let pan = a(4, 0.0);
            let envbuf = a(5, -1.0);
            let maxgrains = a(6, 512.0);
            ctx.node("GrainSin", Rate::Audio, vec![trig, dur, freq, pan, envbuf, maxgrains], numchans, 0)
        }
        "GrainFM" => {
            let numchans = args.first().and_then(|v| if let Input::Constant(c) = v { Some(*c as u32) } else { None }).unwrap_or(2);
            let trig = a(1, 10.0);
            let dur = a(2, 0.1);
            let carfreq = a(3, 440.0);
            let modfreq = a(4, 200.0);
            let index = a(5, 1.0);
            let pan = a(6, 0.0);
            let envbuf = a(7, -1.0);
            let maxgrains = a(8, 512.0);
            ctx.node("GrainFM", Rate::Audio, vec![trig, dur, carfreq, modfreq, index, pan, envbuf, maxgrains], numchans, 0)
        }
        "grains_t" => {
            let numchans = args.first().and_then(|v| if let Input::Constant(c) = v { Some(*c as u32) } else { None }).unwrap_or(2);
            let trig = a(1, 10.0);
            let bufnum = a(2, 0.0);
            let rate = a(3, 1.0);
            let centerpos = a(4, 0.0);
            let dur = a(5, 0.1);
            let pan = a(6, 0.0);
            let amp = a(7, 0.1);
            let interp = a(8, 4.0);
            ctx.node("TGrains", Rate::Audio, vec![trig, bufnum, rate, centerpos, dur, pan, amp, interp], numchans, 0)
        }

        // Signal processing methods
        "Clip" => {
            let sig = a(0, 0.0);
            let lo = a(1, 0.0);
            let hi = a(2, 1.0);
            let rate = ctx.rate_of(&[sig, lo, hi]);
            vec![ctx.node1("Clip", rate, vec![sig, lo, hi], 0)]
        }
        "Wrap" => {
            let sig = a(0, 0.0);
            let lo = a(1, 0.0);
            let hi = a(2, 1.0);
            let rate = ctx.rate_of(&[sig, lo, hi]);
            vec![ctx.node1("Wrap", rate, vec![sig, lo, hi], 0)]
        }
        "LinLin" => {
            let sig = a(0, 0.0);
            let in_min = a(1, 0.0);
            let in_max = a(2, 1.0);
            let out_min = a(3, 0.0);
            let out_max = a(4, 1.0);
            vec![emit_lin_lin(ctx, sig, in_min, in_max, out_min, out_max)]
        }
        "LinExp" => {
            let sig = a(0, 0.0);
            let in_min = a(1, 0.0);
            let in_max = a(2, 1.0);
            let out_min = a(3, 1.0);
            let out_max = a(4, 2.0);
            vec![emit_lin_exp(ctx, sig, in_min, in_max, out_min, out_max)]
        }

        // Analysis feedback — send values back to Audion via OSC
        "send_reply" => {
            let rate = a(0, 10.0);
            let addr = args.get(1).copied().unwrap_or_else(|| konst(0.0));
            let values = a(2, 0.0);
            // Rate is in Hz; silently cap at 100 to prevent OSC flooding.
            let capped = ctx.binop(binop::MIN, rate, konst(100.0));
            let trig = ctx.node1("Impulse", Rate::Control, vec![capped, konst(0.0)], 0);
            // A2K in case `values` is audio-rate (e.g. Amplitude.ar).
            let values_k = ctx.node1("A2K", Rate::Control, vec![values], 0);
            ctx.node("SendReply", Rate::Control, vec![trig, addr, values_k, konst(-1.0)], 0, 0)
        }

        // Unknown — pass through as a raw SC UGen name at audio rate.
        other => vec![ctx.node1(other, Rate::Audio, args.to_vec(), 0)],
    }
}

/// Envelope shapes supported by `env()`/`env_perc()`, lowered to a literal
/// Env array fed into `EnvGen`, matching `Env.perc`/`Env.asr`'s expansion
/// (SCClassLibrary/Common/Audio/Env.sc):
///   Env.perc(atk, rel, level=1, curve=-4)  -> levels [0, level, 0], no release node
///   Env.asr(atk, susLevel, rel, curve=-4)  -> levels [0, susLevel, 0], releaseNode=1
/// (`sus` in `env()`/Env.asr is a sustain LEVEL, not a hold duration — the
/// envelope holds indefinitely at that level until `gate` drops.)
enum EnvShape {
    Perc(Input, Input, Input), // atk, rel, curve
    Asr(Input, Input, Input),  // atk, susLevel, rel
}

fn done_action_f(input: Input) -> f32 {
    if let Input::Constant(v) = input {
        v
    } else {
        2.0
    }
}

/// Build an `EnvGen.kr(Env.perc(...)/Env.asr(...), gate, doneAction: n)`
/// equivalent. EnvGen's real server inputs are `[gate, levelScale,
/// levelBias, timeScale, doneAction, initialLevel, numSegments,
/// releaseNode, loopNode, then numSegments * (level, dur, shapeCode,
/// curveValue)]` (levelScale/levelBias/timeScale default to 1/0/1 — audion
/// doesn't expose them). A plain numeric curve (as opposed to a named
/// shape like \sine) always encodes as shapeCode 5 with the number itself
/// as curveValue.
fn emit_env_gen(ctx: &mut BuildCtx, shape: EnvShape, gate: Input, done_action: f32) -> Input {
    let (seg_levels, times, curve, release_node): (Vec<Input>, Vec<Input>, Input, f32) = match shape {
        EnvShape::Perc(atk, rel, curve) => (vec![konst(1.0), konst(0.0)], vec![atk, rel], curve, -99.0),
        EnvShape::Asr(atk, sus_level, rel) => (vec![sus_level, konst(0.0)], vec![atk, rel], konst(-4.0), 1.0),
    };

    let n = times.len();
    let mut inputs = vec![
        gate,
        konst(1.0), // levelScale
        konst(0.0), // levelBias
        konst(1.0), // timeScale
        konst(done_action),
        konst(0.0),           // initialLevel
        konst(n as f32),      // numSegments
        konst(release_node),  // releaseNode
        konst(-99.0),         // loopNode: none
    ];
    for i in 0..n {
        inputs.push(seg_levels[i]);
        inputs.push(times[i]);
        inputs.push(konst(5.0)); // shapeCode 5 = numeric curve
        inputs.push(curve);          // curveValue
    }
    ctx.node1("EnvGen", Rate::Control, inputs, 0)
}

/// `sig.linlin(inMin, inMax, outMin, outMax)` lowered to `MulAdd`:
/// scale = (outMax-outMin)/(inMax-inMin); offset = outMin - inMin*scale.
fn emit_lin_lin(ctx: &mut BuildCtx, sig: Input, in_min: Input, in_max: Input, out_min: Input, out_max: Input) -> Input {
    let dst_span = ctx.binop(binop::SUB, out_max, out_min);
    let src_span = ctx.binop(binop::SUB, in_max, in_min);
    let scale = ctx.binop(binop::DIV, dst_span, src_span);
    let scaled_in_min = ctx.binop(binop::MUL, scale, in_min);
    let offset = ctx.binop(binop::SUB, out_min, scaled_in_min);
    let rate = ctx.rate_of(&[sig, scale, offset]);
    ctx.node1("MulAdd", rate, vec![sig, scale, offset], 0)
}

/// `sig.linexp(inMin, inMax, outMin, outMax)`:
/// outMin * (outMax/outMin) ** ((sig-inMin)/(inMax-inMin))
fn emit_lin_exp(ctx: &mut BuildCtx, sig: Input, in_min: Input, in_max: Input, out_min: Input, out_max: Input) -> Input {
    let numer = ctx.binop(binop::SUB, sig, in_min);
    let denom = ctx.binop(binop::SUB, in_max, in_min);
    let ratio = ctx.binop(binop::DIV, numer, denom);
    let out_ratio = ctx.binop(binop::DIV, out_max, out_min);
    let log_out_ratio = ctx.unop(unop::LOG, out_ratio);
    let scaled = ctx.binop(binop::MUL, ratio, log_out_ratio);
    let expd = ctx.unop(unop::EXP, scaled);
    ctx.binop(binop::MUL, out_min, expd)
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
