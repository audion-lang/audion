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

