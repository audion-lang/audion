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
//! Standalone SynthDef binary encoder — no sclang, no scsynth. `graph.rs`
//! and `encoder.rs` are adapted from vibelang-dsp
//! (https://github.com/trusch/vibelang, MIT OR Apache-2.0); `ctx.rs` is
//! audion-original glue that lets `synthdef.rs` walk a `define` body and
//! build the graph directly instead of emitting sclang source text.

pub mod ctx;
pub mod encoder;
pub mod errors;
pub mod graph;
