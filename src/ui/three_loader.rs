// Copyright (C) 2025-2026 Aleksandr Bogdanov — GPL-3.0-or-later
//
// File-format loaders that run on the interpreter thread.
// Output is always a flat Vec<[f32; 8]> triangle list: pos(3) + norm(3) + uv(2).

use super::three::Vert;

// ---------------------------------------------------------------------------
// OBJ loader — inline parser, zero extra dependencies
// ---------------------------------------------------------------------------

pub fn load_obj(path: &std::path::Path) -> Result<Vec<Vert>, String> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("OBJ read error: {e}"))?;

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals:   Vec<[f32; 3]> = Vec::new();
    let mut uvs:       Vec<[f32; 2]> = Vec::new();
    let mut out:       Vec<Vert>     = Vec::new();

    for line in src.lines() {
        let line = line.trim();
        if line.starts_with("vn ") {
            let f = parse_floats(&line[3..]);
            if f.len() >= 3 { normals.push([f[0], f[1], f[2]]); }
        } else if line.starts_with("vt ") {
            let f = parse_floats(&line[3..]);
            if f.len() >= 2 { uvs.push([f[0], 1.0 - f[1]]); } // flip V for OpenGL convention
        } else if line.starts_with("v ") {
            let f = parse_floats(&line[2..]);
            if f.len() >= 3 { positions.push([f[0], f[1], f[2]]); }
        } else if line.starts_with("f ") {
            // Each token can be:  v  |  v/vt  |  v//vn  |  v/vt/vn
            let tokens: Vec<[Option<usize>; 3]> = line[2..]
                .split_whitespace()
                .map(|tok| parse_face_token(tok))
                .collect();
            // Fan triangulation for quads+
            for i in 1..tokens.len().saturating_sub(1) {
                for &idx in &[0usize, i, i + 1] {
                    let t = tokens[idx];
                    let pi = t[0].unwrap_or(1).saturating_sub(1);
                    let ui = t[1].unwrap_or(1).saturating_sub(1);
                    let ni = t[2].unwrap_or(1).saturating_sub(1);
                    let p = positions.get(pi).copied().unwrap_or([0.0; 3]);
                    let uv = uvs.get(ui).copied().unwrap_or([0.0; 2]);
                    let n = normals.get(ni).copied().unwrap_or([0.0, 1.0, 0.0]);
                    out.push([p[0], p[1], p[2], n[0], n[1], n[2], uv[0], uv[1]]);
                }
            }
        }
    }

    // If no normals were in the file, compute flat normals per triangle
    if normals.is_empty() {
        for tri in out.chunks_exact_mut(3) {
            let a = glam::Vec3::from_slice(&tri[0][0..3]);
            let b = glam::Vec3::from_slice(&tri[1][0..3]);
            let c = glam::Vec3::from_slice(&tri[2][0..3]);
            let n = (b - a).cross(c - a).normalize();
            for v in tri.iter_mut() { v[3] = n.x; v[4] = n.y; v[5] = n.z; }
        }
    }

    if out.is_empty() {
        return Err("OBJ file contained no geometry".to_string());
    }
    Ok(out)
}

fn parse_floats(s: &str) -> Vec<f32> {
    s.split_whitespace()
     .filter_map(|t| t.parse::<f32>().ok())
     .collect()
}

fn parse_face_token(tok: &str) -> [Option<usize>; 3] {
    let mut parts = tok.split('/');
    let v  = parts.next().and_then(|s| s.parse::<usize>().ok());
    let vt = parts.next().and_then(|s| s.parse::<usize>().ok());
    let vn = parts.next().and_then(|s| s.parse::<usize>().ok());
    [v, vt, vn]
}

// ---------------------------------------------------------------------------
// GLTF / GLB loader  (gltf crate)
// Only Triangles primitive mode supported; GLB (embedded buffers) preferred.
// ---------------------------------------------------------------------------

pub fn load_gltf(path: &std::path::Path) -> Result<Vec<Vert>, String> {
    let (doc, buffers, _images) = gltf::import(path)
        .map_err(|e| format!("GLTF import error: {e}"))?;

    let mut out: Vec<Vert> = Vec::new();

    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles { continue; }

            let reader = prim.reader(|buf| Some(buffers[buf.index()].0.as_slice()));

            let Some(pos_iter) = reader.read_positions() else { continue; };
            let positions: Vec<[f32; 3]> = pos_iter.collect();

            let normals: Vec<[f32; 3]> = reader.read_normals()
                .map(|it| it.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

            let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0)
                .map(|it| it.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0; 2]; positions.len()]);

            let indices: Vec<u32> = reader.read_indices()
                .map(|it| it.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());

            for &i in &indices {
                let i = i as usize;
                let p  = positions.get(i).copied().unwrap_or([0.0; 3]);
                let n  = normals  .get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                let uv = uvs      .get(i).copied().unwrap_or([0.0; 2]);
                out.push([p[0], p[1], p[2], n[0], n[1], n[2], uv[0], uv[1]]);
            }
        }
    }

    if out.is_empty() {
        return Err("GLTF file contained no triangle geometry".to_string());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Texture loader  (image crate — already in the dep tree via eframe)
// ---------------------------------------------------------------------------

pub fn load_texture(path: &std::path::Path) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::open(path)
        .map_err(|e| format!("texture load error: {e}"))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}
