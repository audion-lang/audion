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

use std::process::Command;

use crate::error::{AudionError, Result};

/// Compile a SynthDef by shelling out to sclang.
/// Returns the bytes of the compiled .scsyndef binary.
pub fn compile_synthdef(name: &str, sclang_code: &str) -> Result<Vec<u8>> {
    let sclang = find_sclang().ok_or_else(|| AudionError::RuntimeError {
        msg: "sclang not found. Install SuperCollider and ensure sclang is in your PATH \
              (or at /Applications/SuperCollider.app/Contents/MacOS/sclang on macOS)"
            .to_string(),
    })?;

    let mut tmp = tempfile(name)?;
    tmp.write_all(sclang_code.as_bytes())
        .map_err(|e| AudionError::RuntimeError {
            msg: format!("failed to write temp .scd file: {}", e),
        })?;
    let scd_path = tmp.path().to_string();
    drop(tmp);

    let output = Command::new(&sclang)
        .arg(&scd_path)
        .output()
        .map_err(|e| AudionError::RuntimeError {
            msg: format!("failed to run sclang: {}", e),
        })?;

    // Clean up .scd source file
    let _ = std::fs::remove_file(&scd_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.is_empty() {
            stderr.to_string()
        } else {
            stdout.to_string()
        };
        return Err(AudionError::RuntimeError {
            msg: format!("sclang compilation failed:\n{}", detail.trim()),
        });
    }

    // Read the .scsyndef binary that sclang wrote
    let def_dir = std::env::temp_dir().join("audion_synthdefs");
    let def_path = def_dir.join(format!("{}.scsyndef", name));
    let bytes = std::fs::read(&def_path).map_err(|e| AudionError::RuntimeError {
        msg: format!(
            "failed to read compiled SynthDef at '{}': {}",
            def_path.display(),
            e
        ),
    })?;

    // Clean up the .scsyndef file
    let _ = std::fs::remove_file(&def_path);
    let _ = std::fs::remove_dir(&def_dir);

    Ok(bytes)
}

fn find_sclang() -> Option<String> {
    // Check PATH first
    if Command::new("sclang").arg("--version").output().is_ok() {
        return Some("sclang".to_string());
    }

    // macOS default location
    let mac_path = "/Applications/SuperCollider.app/Contents/MacOS/sclang";
    if std::path::Path::new(mac_path).exists() {
        return Some(mac_path.to_string());
    }

    // Linux common locations
    for path in &["/usr/bin/sclang", "/usr/local/bin/sclang"] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    None
}

struct TempFile {
    path: String,
}

impl TempFile {
    fn path(&self) -> &str {
        &self.path
    }

    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        std::fs::write(&self.path, data)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Intentionally not removing here — caller handles cleanup
    }
}

pub fn synthdef_output_dir() -> String {
    let dir = std::env::temp_dir().join("audion_synthdefs");
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().to_string()
}

fn tempfile(name: &str) -> Result<TempFile> {
    let path = format!(
        "{}/audion_{}.scd",
        std::env::temp_dir().display(),
        name
    );
    Ok(TempFile { path })
}
