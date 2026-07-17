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
//   x     = 120.0           # absolute position (set by drag-to-arrange edit mode)
//   y     = 40.0

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use super::{WidgetConfig, WidgetStyle};

#[derive(Deserialize, Default, Debug, Clone)]
struct AuiEntry {
    min:             Option<f64>,
    max:             Option<f64>,
    default:         Option<f64>,
    label:           Option<String>,
    color:           Option<[u8; 3]>,
    /// bg_color accepts [r,g,b] or [r,g,b,a] — stored as Vec so both parse.
    bg_color:        Option<Vec<u8>>,
    width:           Option<f32>,
    height:          Option<f32>,
    x:               Option<f32>,
    y:               Option<f32>,
    /// Window-only background image fields (used in [__window] section).
    bg_image:        Option<String>,
    bg_image_mode:   Option<String>,
    bg_image_alpha:  Option<u8>,
}

fn parse_bg_color_rgba(v: &[u8]) -> Option<[u8; 4]> {
    match v {
        [r, g, b]    => Some([*r, *g, *b, 255]),
        [r, g, b, a] => Some([*r, *g, *b, *a]),
        _            => None,
    }
}

type AuiFile = HashMap<String, AuiEntry>;

/// Load widget config overrides from the companion `.aui` file (if present).
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

    if let Some(v) = entry.min    { default.min = v; }
    if let Some(v) = entry.max    { default.max = v; }
    if let Some(s) = &entry.label { default.label = Some(s.clone()); }
    if let Some(v) = entry.x      { default.x = Some(v); }
    if let Some(v) = entry.y      { default.y = Some(v); }

    let widget_bg = entry.bg_color.as_deref().and_then(|v| match v {
        [r, g, b] | [r, g, b, _] => Some([*r, *g, *b]),
        _ => None,
    });
    default.style = WidgetStyle {
        color:    entry.color,
        bg_color: widget_bg,
        width:    entry.width,
        height:   entry.height,
    };

    default
}

/// Write widget defaults back to the `.aui` file for any widget not yet present.
pub fn save_widget_defaults(path: &Path, id: &str, config: &WidgetConfig) {
    let existing_str = std::fs::read_to_string(path).unwrap_or_default();
    let mut file: AuiFile = toml::from_str(&existing_str).unwrap_or_default();

    if file.contains_key(id) {
        return;
    }

    file.insert(id.to_string(), AuiEntry {
        min: Some(config.min), max: Some(config.max),
        label: config.label.clone(),
        color: config.style.color,
        bg_color: config.style.bg_color.map(|[r, g, b]| vec![r, g, b]),
        width: config.style.width, height: config.style.height,
        x: config.x, y: config.y,
        default: None,
        bg_image: None, bg_image_mode: None, bg_image_alpha: None,
    });

    let mut out = existing_str;
    if !out.is_empty() && !out.ends_with('\n') { out.push('\n'); }
    out.push('\n');
    out.push_str(&format!("[{}]\n", id));
    out.push_str(&format!("min = {}\nmax = {}\n", config.min, config.max));
    if let Some(label) = &config.label {
        out.push_str(&format!("label = \"{}\"\n", label.replace('"', "\\\"")));
    }

    let _ = std::fs::write(path, out);
}

/// Layout info collected per widget during edit mode exit.
pub struct WidgetLayout {
    pub x: f32,
    pub y: f32,
    pub width:  Option<f32>,
    pub height: Option<f32>,
}

/// Save all current widget positions + sizes and optional window dimensions to the `.aui` file.
pub fn save_layout(
    path: &Path,
    layouts: &HashMap<String, WidgetLayout>,
    window_size: Option<(f32, f32)>,
) {
    let existing_str = std::fs::read_to_string(path).unwrap_or_default();
    let mut file: AuiFile = toml::from_str(&existing_str).unwrap_or_default();

    for (id, layout) in layouts {
        let entry = file.entry(id.clone()).or_default();
        entry.x = Some(layout.x);
        entry.y = Some(layout.y);
        if layout.width.is_some()  { entry.width  = layout.width; }
        if layout.height.is_some() { entry.height = layout.height; }
    }

    // __window reserved entry for window dimensions
    if let Some((w, h)) = window_size {
        let win = file.entry("__window".to_string()).or_default();
        win.width  = Some(w);
        win.height = Some(h);
    }

    // Hand-roll TOML output (no Serialize derive)
    let mut out = String::new();
    // Write __window first if present
    if let Some(win) = file.get("__window") {
        out.push_str("[__window]\n");
        if let Some(v) = win.width  { out.push_str(&format!("width = {}\n", v)); }
        if let Some(v) = win.height { out.push_str(&format!("height = {}\n", v)); }
        if let Some(c) = &win.bg_color {
            let arr: Vec<String> = c.iter().map(|v| v.to_string()).collect();
            out.push_str(&format!("bg_color = [{}]\n", arr.join(", ")));
        }
        if let Some(img) = &win.bg_image {
            out.push_str(&format!("bg_image = \"{}\"\n", img.replace('"', "\\\"")));
        }
        if let Some(m) = &win.bg_image_mode  { out.push_str(&format!("bg_image_mode = \"{}\"\n", m)); }
        if let Some(a) = win.bg_image_alpha   { out.push_str(&format!("bg_image_alpha = {}\n", a)); }
        out.push('\n');
    }
    for (id, entry) in &file {
        if id == "__window" { continue; }
        out.push_str(&format!("[{}]\n", id));
        if let Some(v) = entry.min      { out.push_str(&format!("min = {}\n", v)); }
        if let Some(v) = entry.max      { out.push_str(&format!("max = {}\n", v)); }
        if let Some(s) = &entry.label   { out.push_str(&format!("label = \"{}\"\n", s.replace('"', "\\\""))); }
        if let Some([r,g,b]) = entry.color    { out.push_str(&format!("color = [{}, {}, {}]\n", r, g, b)); }
        if let Some(c) = &entry.bg_color {
            let s: Vec<String> = c.iter().map(|v| v.to_string()).collect();
            out.push_str(&format!("bg_color = [{}]\n", s.join(", ")));
        }
        if let Some(v) = entry.width    { out.push_str(&format!("width = {}\n", v)); }
        if let Some(v) = entry.height   { out.push_str(&format!("height = {}\n", v)); }
        if let Some(v) = entry.x        { out.push_str(&format!("x = {}\n", v)); }
        if let Some(v) = entry.y        { out.push_str(&format!("y = {}\n", v)); }
        out.push('\n');
    }

    let _ = std::fs::write(path, out);
}

/// Read window size saved by a previous edit session. Returns None if absent.
pub fn load_window_size(path: &Path) -> Option<(f32, f32)> {
    let content = std::fs::read_to_string(path).ok()?;
    let file: AuiFile = toml::from_str(&content).ok()?;
    let win = file.get("__window")?;
    Some((win.width?, win.height?))
}

/// All window-level background settings from the `[__window]` section.
pub struct WindowBackground {
    pub color:       Option<[u8; 4]>,
    pub image:       Option<String>,
    pub image_mode:  super::BgImageMode,
    pub image_alpha: u8,
}

/// Load background settings from the `[__window]` section of the .aui file.
pub fn load_window_background(path: &Path) -> Option<WindowBackground> {
    let content = std::fs::read_to_string(path).ok()?;
    let file: AuiFile = toml::from_str(&content).ok()?;
    let win = file.get("__window")?;

    let has_something = win.bg_color.is_some() || win.bg_image.is_some();
    if !has_something { return None; }

    Some(WindowBackground {
        color:       win.bg_color.as_deref().and_then(parse_bg_color_rgba),
        image:       win.bg_image.clone(),
        image_mode:  super::BgImageMode::from_str(win.bg_image_mode.as_deref().unwrap_or("fill")),
        image_alpha: win.bg_image_alpha.unwrap_or(255),
    })
}

/// Derive the `.aui` path from a `.au` source path.
pub fn aui_path_for(au_path: &Path) -> std::path::PathBuf {
    au_path.with_extension("aui")
}
