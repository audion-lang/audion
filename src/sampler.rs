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

use std::io::Read;
use std::path::Path;

/// Detect the number of audio channels in a sound file.
/// Supports WAV (RIFF/WAVE). Defaults to 2 (stereo) for AIFF, FLAC, or unrecognized formats.
pub fn detect_channels(path: &Path) -> u32 {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 2,
    };
    let mut header = [0u8; 44];
    if file.read_exact(&mut header).is_err() {
        return 2;
    }

    // WAV: bytes 0-3 = "RIFF", bytes 8-11 = "WAVE", bytes 22-23 = numChannels (LE u16)
    if &header[0..4] == b"RIFF" && &header[8..12] == b"WAVE" {
        return u16::from_le_bytes([header[22], header[23]]) as u32;
    }

    // Default to stereo for AIFF, FLAC, or unknown formats
    2
}


/// A 32-bit float WAV (what scsynth's `/b_write ... "wav" "float"` writes),
/// read into interleaved samples.
pub struct FloatWav {
    pub channels: usize,
    pub sample_rate: f64,
    pub samples: Vec<f32>,
}

impl FloatWav {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }

    /// Mean of all channels per frame.
    pub fn mono(&self) -> Vec<f32> {
        self.samples
            .chunks_exact(self.channels)
            .map(|f| f.iter().sum::<f32>() / self.channels as f32)
            .collect()
    }
}

pub fn read_float_wav(path: &Path) -> Result<FloatWav, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".to_string());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);

    let mut channels = 0usize;
    let mut sample_rate = 0f64;
    let mut data: Option<&[u8]> = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(pos + 4) as usize;
        let body = pos + 8;
        let end = (body + size).min(bytes.len());
        if id == b"fmt " && size >= 16 {
            // 3 = IEEE float, 0xFFFE = extensible (float subformat in practice here).
            let format = u16_at(body);
            let bits = u16_at(body + 14);
            if !(format == 3 || format == 0xFFFE) || bits != 32 {
                return Err(format!("expected 32-bit float WAV, got format {} / {} bits", format, bits));
            }
            channels = u16_at(body + 2) as usize;
            sample_rate = u32_at(body + 4) as f64;
        } else if id == b"data" {
            data = Some(&bytes[body..end]);
        }
        pos = body + size + (size & 1); // chunks are word-aligned
    }
    let data = data.ok_or("WAV has no data chunk")?;
    if channels == 0 {
        return Err("WAV has no fmt chunk".to_string());
    }
    let usable = data.len() / (4 * channels) * 4 * channels;
    let samples = data[..usable]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    Ok(FloatWav { channels, sample_rate, samples })
}

/// Reduce a float WAV file to `n` peak values (see `wav_peaks`).
pub fn float_wav_peaks(path: &Path, n: usize) -> Result<Vec<f32>, String> {
    Ok(wav_peaks(&read_float_wav(path)?, n))
}

/// `n` peak values, each the largest absolute sample over all channels in
/// that slice of the file. Used to draw waveforms.
pub fn wav_peaks(wav: &FloatWav, n: usize) -> Vec<f32> {
    let frames = wav.frames();
    let mut peaks = vec![0f32; n];
    if frames == 0 || n == 0 {
        return peaks;
    }
    for (f, frame) in wav.samples.chunks_exact(wav.channels).enumerate() {
        let slot = f * n / frames;
        for v in frame {
            peaks[slot] = peaks[slot].max(v.abs());
        }
    }
    peaks
}

/// Hits and beat grid found in a sound file.
#[derive(Debug, Clone, PartialEq)]
pub struct Beats {
    /// Frame where each hit starts.
    pub onsets: Vec<usize>,
    /// Whole beats the file spans, 0 when it doesn't sound beaty.
    pub beats: u32,
    /// The file's own tempo for that beat count (0 when not beaty).
    pub bpm: f64,
    /// How periodic the hits are, 0..1.
    pub confidence: f64,
}

const HOP: usize = 512;
const WIN: usize = 1024;

/// Find hits (onsets) and, when they repeat regularly, how many beats the
/// file spans. Energy-based: good for drums and other percussive material.
pub fn detect_beats(mono: &[f32], sample_rate: f64) -> Beats {
    let none = |onsets| Beats { onsets, beats: 0, bpm: 0.0, confidence: 0.0 };
    if mono.len() < WIN * 4 || sample_rate <= 0.0 {
        return none(Vec::new());
    }

    // Loudness per hop, and how much it jumps up from the previous hop.
    let n = (mono.len() - WIN) / HOP + 1;
    let rms: Vec<f64> = (0..n)
        .map(|k| {
            let w = &mono[k * HOP..k * HOP + WIN];
            (w.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>() / WIN as f64).sqrt()
        })
        .collect();
    let loudest = rms.iter().cloned().fold(0.0, f64::max);
    if loudest < 1e-4 {
        return none(Vec::new());
    }
    let quiet = loudest * 0.02; // ignore jumps inside near-silence
    // Before the file counts as near-silence, so a hit right at the start counts.
    let mut flux = vec![0f64; n];
    for k in 0..n {
        let prev = if k == 0 { quiet } else { rms[k - 1] };
        if rms[k] > quiet {
            flux[k] = ((rms[k] + 1e-9).ln() - (prev + 1e-9).ln()).max(0.0);
        }
    }

    // Onsets: local peaks of the jump curve above mean + 1 std, ≥ 50 ms apart.
    let mean = flux.iter().sum::<f64>() / n as f64;
    let std = (flux.iter().map(|f| (f - mean) * (f - mean)).sum::<f64>() / n as f64).sqrt();
    let threshold = mean + std;
    let min_gap = (0.05 * sample_rate) as usize;
    let mut onsets: Vec<usize> = Vec::new();
    for k in 0..n {
        let lo = k.saturating_sub(2);
        let hi = (k + 3).min(n);
        let is_peak = flux[k] > threshold && (lo..hi).all(|j| flux[j] <= flux[k]);
        if !is_peak {
            continue;
        }
        // Refine to the first sample that reaches 30% of the hit's peak.
        let from = (k.saturating_sub(1)) * HOP;
        let to = (k * HOP + WIN).min(mono.len());
        let peak = mono[from..to].iter().fold(0f32, |m, x| m.max(x.abs()));
        let rel = mono[from..to].iter().position(|x| x.abs() >= peak * 0.3).unwrap_or(0);
        let frame = (from + rel).saturating_sub(64);
        if onsets.last().map_or(true, |&last| frame >= last + min_gap) {
            onsets.push(frame);
        }
    }
    if onsets.len() < 3 {
        return none(onsets);
    }

    // Beat length: autocorrelation of the jump curve, gently preferring ~120 bpm.
    let centered: Vec<f64> = flux.iter().map(|f| f - mean).collect();
    let ac = |lag: usize| -> f64 {
        (0..n - lag).map(|i| centered[i] * centered[i + lag]).sum::<f64>() / (n - lag) as f64
    };
    let ac0 = ac(0);
    let hops_per_sec = sample_rate / HOP as f64;
    let min_lag = (60.0 / 200.0 * hops_per_sec).floor().max(1.0) as usize;
    let max_lag = ((60.0 / 50.0 * hops_per_sec).ceil() as usize).min(n / 2);
    if ac0 <= 0.0 || min_lag >= max_lag {
        return none(onsets);
    }
    let mut best = (0usize, f64::MIN, 0.0);
    for lag in min_lag..=max_lag {
        let v = ac(lag);
        let bpm = 60.0 * hops_per_sec / lag as f64;
        let w = (-0.5 * (bpm / 120.0).log2().powi(2)).exp();
        if v * w > best.1 {
            best = (lag, v * w, v);
        }
    }
    let (lag, _, peak) = best;
    let confidence = (peak / ac0).clamp(0.0, 1.0);
    // Parabolic refinement of the peak position.
    let (y0, y1, y2) = (ac(lag - 1), peak, ac(lag + 1));
    let denom = y0 - 2.0 * y1 + y2;
    let shift = if denom.abs() > 1e-12 { (0.5 * (y0 - y2) / denom).clamp(-0.5, 0.5) } else { 0.0 };
    let beat_secs = (lag as f64 + shift) / hops_per_sec;

    let dur = mono.len() as f64 / sample_rate;

    // Loops: assume the file spans whole beats and find the beat count whose
    // 16th-note grid the hits land on best.
    if let Some(beats) = loop_beats(&onsets, sample_rate, dur) {
        return Beats { onsets, beats, bpm: 60.0 * beats as f64 / dur, confidence };
    }

    // Otherwise trust a clearly periodic beat from the autocorrelation.
    let raw = dur / beat_secs;
    if confidence < 0.3 || raw < 1.0 {
        return Beats { onsets, beats: 0, bpm: 0.0, confidence };
    }
    // Loops are usually whole bars: snap to a multiple of 4 when close.
    let bars = (raw / 4.0).round() * 4.0;
    let beats = if bars >= 4.0 && (bars - raw).abs() <= raw * 0.08 { bars } else { raw.round().max(1.0) };
    Beats { onsets, beats: beats as u32, bpm: 60.0 * beats / dur, confidence }
}

/// Beat count (70-180 bpm) whose 16th-note grid the onsets fit, if any fits well.
/// Ties go to whole bars and tempos near 120.
fn loop_beats(onsets: &[usize], sample_rate: f64, dur: f64) -> Option<u32> {
    let lo = (dur * 70.0 / 60.0).ceil().max(1.0) as u32;
    let hi = ((dur * 180.0 / 60.0).floor() as u32).min(256);
    let mut best: Option<(u32, f64, f64)> = None; // (beats, grid error, score)
    for n in lo..=hi {
        let grid = dur / (n as f64 * 4.0);
        let err = onsets
            .iter()
            .map(|&o| {
                let pos = o as f64 / sample_rate / grid;
                (pos - pos.round()).abs()
            })
            .sum::<f64>()
            / onsets.len() as f64;
        let bpm = 60.0 * n as f64 / dur;
        let score = err + if n % 4 == 0 { 0.0 } else { 0.05 } + 0.05 * (bpm / 120.0).log2().abs();
        if best.map_or(true, |b| score < b.2) {
            best = Some((n, err, score));
        }
    }
    // 0.1 of a 16th on average: tight enough that random hits rarely pass.
    best.filter(|b| b.1 <= 0.1).map(|b| b.0)
}
