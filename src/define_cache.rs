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

/// Persistent disk cache for compiled SynthDef bytecode.
///
/// One `.adc` (audion-define-cache) file is written next to each `.au` source file
/// that contains `define` blocks.  Multiple defines in one source file share a single
/// `.adc`.  The file is re-read at most once per process run (then promoted to the
/// in-memory `SynthDefCache`), so disk I/O is minimal after the first load.
///
/// Binary format:
///   magic      : [u8; 4]  = b"ADC1"
///   entry_count: u32 (little-endian)
///   per entry  :
///     name_len : u16le
///     name     : [u8; name_len]   (UTF-8)
///     hash     : u64le            (AST hash)
///     data_len : u32le
///     data     : [u8; data_len]   (raw .scsyndef bytes)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"ADC1";

pub struct DefineCache {
    /// Loaded .adc files: canonical source path -> { synthdef_name -> (hash, bytes) }
    files: HashMap<PathBuf, HashMap<String, (u64, Vec<u8>)>>,
}

impl DefineCache {
    pub fn new() -> Self {
        DefineCache { files: HashMap::new() }
    }

    /// Return the `.adc` path that corresponds to a given source `.au` file.
    pub fn cache_path(source_path: &Path) -> PathBuf {
        let stem = source_path.file_stem().unwrap_or_default();
        source_path.with_file_name(format!("{}.adc", stem.to_string_lossy()))
    }

    /// Load a source file's `.adc` entries into memory if not already loaded.
    fn ensure_loaded(&mut self, source_path: &Path) {
        if !self.files.contains_key(source_path) {
            let adc_path = Self::cache_path(source_path);
            let entries = read_adc(&adc_path).unwrap_or_default();
            self.files.insert(source_path.to_path_buf(), entries);
        }
    }

    /// Look up a compiled define.  Returns `Some(bytes)` only if the stored hash matches.
    pub fn get(&mut self, source_path: &Path, name: &str, hash: u64) -> Option<Vec<u8>> {
        self.ensure_loaded(source_path);
        let entries = self.files.get(source_path)?;
        entries.get(name).and_then(|(cached_hash, bytes)| {
            if *cached_hash == hash { Some(bytes.clone()) } else { None }
        })
    }

    /// Store compiled bytes for a define and persist the whole entry set to disk.
    pub fn put(&mut self, source_path: &Path, name: &str, hash: u64, bytes: Vec<u8>) {
        self.ensure_loaded(source_path);
        let entries = self.files.entry(source_path.to_path_buf()).or_default();
        entries.insert(name.to_string(), (hash, bytes));
        let adc_path = Self::cache_path(source_path);
        if let Err(e) = write_adc(&adc_path, entries) {
            eprintln!("warning: could not write define cache '{}': {}", adc_path.display(), e);
        }
    }
}

fn read_adc(path: &Path) -> Option<HashMap<String, (u64, Vec<u8>)>> {
    let data = std::fs::read(path).ok()?;
    let mut pos = 0usize;

    if data.get(pos..pos + 4)? != MAGIC {
        return None;
    }
    pos += 4;

    let count = read_u32(&data, &mut pos)? as usize;
    let mut map = HashMap::new();

    for _ in 0..count {
        let name_len = read_u16(&data, &mut pos)? as usize;
        let name = std::str::from_utf8(data.get(pos..pos + name_len)?).ok()?.to_string();
        pos += name_len;

        let hash = read_u64(&data, &mut pos)?;
        let data_len = read_u32(&data, &mut pos)? as usize;
        let bytes = data.get(pos..pos + data_len)?.to_vec();
        pos += data_len;

        map.insert(name, (hash, bytes));
    }

    Some(map)
}

fn write_adc(path: &Path, entries: &HashMap<String, (u64, Vec<u8>)>) -> std::io::Result<()> {
    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());

    for (name, (hash, bytes)) in entries {
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(&hash.to_le_bytes());
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
    }

    std::fs::write(path, &buf)
}

fn read_u16(data: &[u8], pos: &mut usize) -> Option<u16> {
    let arr: [u8; 2] = data.get(*pos..*pos + 2)?.try_into().ok()?;
    *pos += 2;
    Some(u16::from_le_bytes(arr))
}

fn read_u32(data: &[u8], pos: &mut usize) -> Option<u32> {
    let arr: [u8; 4] = data.get(*pos..*pos + 4)?.try_into().ok()?;
    *pos += 4;
    Some(u32::from_le_bytes(arr))
}

fn read_u64(data: &[u8], pos: &mut usize) -> Option<u64> {
    let arr: [u8; 8] = data.get(*pos..*pos + 8)?.try_into().ok()?;
    *pos += 8;
    Some(u64::from_le_bytes(arr))
}
