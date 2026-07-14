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

// Phase 4: .aui TOML config — load/save per-widget settings.
// Stub for Phase 2 MVP; returns defaults.

use super::WidgetConfig;
use std::path::Path;

/// Load widget config from a .aui TOML file, merging over defaults.
/// Returns `default` unchanged until Phase 4 TOML parsing is implemented.
pub fn load_widget_config(_path: &Path, _id: &str, default: WidgetConfig) -> WidgetConfig {
    default
}

/// Write current widget defaults to the .aui file (auto-generate if absent).
/// No-op until Phase 4.
pub fn save_widget_defaults(_path: &Path, _id: &str, _config: &WidgetConfig) {}

/// Derive the .aui path from a .au source path: `my_song.au` → `my_song.aui`.
pub fn aui_path_for(au_path: &Path) -> std::path::PathBuf {
    au_path.with_extension("aui")
}
