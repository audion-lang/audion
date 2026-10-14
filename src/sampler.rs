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


/// Read a 32-bit float WAV (what scsynth's `/b_write ... "wav" "float"`
/// produces) and reduce it to `n` peak values, each the largest absolute
/// sample over all channels in that slice of the file. Used to draw waveforms.
pub fn float_wav_peaks(path: &Path, n: usize) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".to_string());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);

    let mut channels = 0usize;
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
        } else if id == b"data" {
            data = Some(&bytes[body..end]);
        }
        pos = body + size + (size & 1); // chunks are word-aligned
    }
    let data = data.ok_or("WAV has no data chunk")?;
    if channels == 0 {
        return Err("WAV has no fmt chunk".to_string());
    }

    let frames = data.len() / (4 * channels);
    let mut peaks = vec![0f32; n];
    if frames == 0 || n == 0 {
        return Ok(peaks);
    }
    for f in 0..frames {
        let slot = f * n / frames;
        for c in 0..channels {
            let i = (f * channels + c) * 4;
            let v = f32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]).abs();
            if v > peaks[slot] {
                peaks[slot] = v;
            }
        }
    }
    Ok(peaks)
}
