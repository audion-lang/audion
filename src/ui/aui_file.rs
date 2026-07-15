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

// .aui TOML config — per-widget overrides for any .au script.
//
// Format (by widget id, kind-agnostic):
//
//   [tempo]
//   min   = 60.0
//   max   = 200.0
//   label = "Tempo (BPM)"
//   color = [255, 102, 0]   # r g b 0-255
//   width = 240.0
//
//   [reverb]
//   label = "Reverb Mix"
//   bg_color = [20, 20, 30]

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use super::{WidgetConfig, WidgetStyle};

#[derive(Deserialize, Default, Debug)]
struct AuiEntry {
    min: Option<f64>,
    max: Option<f64>,
    default: Option<f64>,
    label: Option<String>,
    color: Option<[u8; 3]>,
    bg_color: Option<[u8; 3]>,
    width: Option<f32>,
    height: Option<f32>,
}

type AuiFile = HashMap<String, AuiEntry>;

/// Load widget config overrides from the companion `.aui` file (if present),
/// applying them on top of `default`. Missing file → `default` unchanged.
pub fn load_widget_config(path: &Path, id: &str, mut default: WidgetConfig) -> WidgetConfig {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return default,
    };
    let file: AuiFile = match toml::from_str(&content) {
        Ok(f) => f,
        Err(_) => return default,
    };
    let Some(entry) = file.get(id) else { return default; };

    if let Some(v) = entry.min     { default.min = v; }
    if let Some(v) = entry.max     { default.max = v; }
    if let Some(s) = &entry.label  { default.label = Some(s.clone()); }

    // default value is handled by the caller via WidgetState::new after config merge
    let _ = entry.default; // reserved for future: set initial WidgetValue

    default.style = WidgetStyle {
        color:    entry.color,
        bg_color: entry.bg_color,
        width:    entry.width,
        height:   entry.height,
    };

    default
}

/// Write widget defaults back to the `.aui` file (auto-generate if absent).
/// Existing entries are preserved; only missing widget ids are appended.
pub fn save_widget_defaults(path: &Path, id: &str, config: &WidgetConfig) {
    // Read existing or start empty
    let existing_str = std::fs::read_to_string(path).unwrap_or_default();
    let mut file: AuiFile = toml::from_str(&existing_str).unwrap_or_default();

    // Only write if the id is not yet present (don't overwrite user edits)
    if file.contains_key(id) {
        return;
    }

    file.insert(id.to_string(), AuiEntry {
        min:      Some(config.min),
        max:      Some(config.max),
        default:  None,
        label:    config.label.clone(),
        color:    config.style.color,
        bg_color: config.style.bg_color,
        width:    config.style.width,
        height:   config.style.height,
    });

    // Serialise manually — toml crate can't serialise our struct without Serialize derive.
    // Build a minimal hand-rolled TOML string and append to the file.
    let mut out = existing_str;
    if !out.is_empty() && !out.ends_with('\n') { out.push('\n'); }
    out.push('\n');
    out.push_str(&format!("[{}]\n", id));
    out.push_str(&format!("min = {}\n", config.min));
    out.push_str(&format!("max = {}\n", config.max));
    if let Some(label) = &config.label {
        out.push_str(&format!("label = \"{}\"\n", label.replace('"', "\\\"")));
    }

    let _ = std::fs::write(path, out); // best-effort
}

/// Derive the `.aui` path from a `.au` source path: `my_song.au` → `my_song.aui`.
pub fn aui_path_for(au_path: &Path) -> std::path::PathBuf {
    au_path.with_extension("aui")
}
