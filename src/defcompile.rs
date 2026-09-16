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
//! SynthDef compilation used to shell out to sclang; `synthdef.rs` now
//! builds and encodes SynthDef binaries directly (see `src/dsp/`). This
//! module just keeps the shared output directory that `osc.rs::load_synthdef`
//! writes `.scsyndef` files into for `/d_load`.

/// Per-process SynthDef output directory. Keyed by PID so two audion
/// processes writing/loading the SAME SynthDef name at the same time never
/// clobber each other's `.scsyndef` file.
pub fn synthdef_output_dir() -> String {
    let dir = std::env::temp_dir().join(format!("audion_synthdefs_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().to_string()
}
