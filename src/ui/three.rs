// Copyright (C) 2025-2026 Aleksandr Bogdanov — GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Arc;
use glam::{Mat4, Vec3};

// ---------------------------------------------------------------------------
// Vertex layout: [x, y, z, nx, ny, nz, u, v]  (8 × f32 = 32 bytes)
// Triangle list, non-indexed.
// ---------------------------------------------------------------------------

pub type Vert = [f32; 8];

// ---------------------------------------------------------------------------
// Scene data — shared between interpreter thread (writes) and UI thread (reads)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ThreeSceneData {
    pub id:          String,
    pub width:       f32,
    pub height:      f32,
    pub clear_color: [f32; 3],
    pub camera:      Camera3D,
    pub meshes:      Vec<Mesh3D>,
    /// name → WGSL source (ShaderEntry::Fragment wraps user fs in our prelude;
    /// ShaderEntry::Full is used as-is).
    pub shaders:     HashMap<String, ShaderEntry>,
    /// name → decoded RGBA8 pixels ready to be uploaded to GPU.
    pub textures:    HashMap<String, TextureEntry>,
}

impl Default for ThreeSceneData {
    fn default() -> Self {
        Self {
            id:          String::new(),
            width:       640.0,
            height:      480.0,
            clear_color: [0.05, 0.05, 0.1],
            camera:      Camera3D::default(),
            meshes:      Vec::new(),
            shaders:     HashMap::new(),
            textures:    HashMap::new(),
        }
    }
}

impl ThreeSceneData {
    pub fn get_or_create_mesh(&mut self, id: &str, kind: MeshKind) -> &mut Mesh3D {
        if let Some(pos) = self.meshes.iter().position(|m| m.id == id) {
            self.meshes[pos].kind = kind;
            return &mut self.meshes[pos];
        }
        self.meshes.push(Mesh3D::new(id.to_string(), kind));
        self.meshes.last_mut().unwrap()
    }

    pub fn get_mesh_mut(&mut self, id: &str) -> Option<&mut Mesh3D> {
        self.meshes.iter_mut().find(|m| m.id == id)
    }
}

// ---------------------------------------------------------------------------
// Shader entries
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum ShaderEntry {
    /// User supplies only the `@fragment fn fs(in: VOut) -> @location(0) vec4<f32> { ... }`.
    /// The GPU backend prepends the standard Uniforms + vertex shader prelude.
    Fragment(String),
    /// User supplies the complete WGSL module (must define `vs` and `fs`).
    Full(String),
}

// ---------------------------------------------------------------------------
// Texture entries — decoded on the interpreter thread, uploaded lazily to GPU
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct TextureEntry {
    pub pixels: Vec<u8>,  // RGBA8, row-major
    pub width:  u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Camera3D {
    pub eye:     Vec3,
    pub target:  Vec3,
    pub up:      Vec3,
    pub fov_deg: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            eye:     Vec3::new(3.0, 3.0, 5.0),
            target:  Vec3::ZERO,
            up:      Vec3::Y,
            fov_deg: 60.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Mesh
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Mesh3D {
    pub id:         String,
    pub kind:       MeshKind,
    pub color:      [f32; 3],
    pub position:   Vec3,
    pub rotation:   Vec3,       // Euler degrees (X, Y, Z)
    pub scale:      Vec3,
    pub visible:    bool,
    pub shader_id:  Option<String>,
    pub texture_id: Option<String>,
    pub uv_scale:   [f32; 2],
    pub custom0:    [f32; 4],   // u.custom0 in WGSL
    pub custom1:    [f32; 4],   // u.custom1 in WGSL
}

impl Mesh3D {
    pub fn new(id: String, kind: MeshKind) -> Self {
        Self {
            id,
            kind,
            color:      [1.0, 1.0, 1.0],
            position:   Vec3::ZERO,
            rotation:   Vec3::ZERO,
            scale:      Vec3::ONE,
            visible:    true,
            shader_id:  None,
            texture_id: None,
            uv_scale:   [1.0, 1.0],
            custom0:    [0.0; 4],
            custom1:    [0.0; 4],
        }
    }

    pub fn model_matrix(&self) -> Mat4 {
        let t  = Mat4::from_translation(self.position);
        let rx = Mat4::from_rotation_x(self.rotation.x.to_radians());
        let ry = Mat4::from_rotation_y(self.rotation.y.to_radians());
        let rz = Mat4::from_rotation_z(self.rotation.z.to_radians());
        let s  = Mat4::from_scale(self.scale);
        t * ry * rx * rz * s
    }
}

#[derive(Clone, Debug)]
pub enum MeshKind {
    Box,
    Plane,
    Sphere,
    Axes,
    /// Vertices loaded from an OBJ or GLTF file (pos+norm+uv, triangle list).
    Loaded(Arc<Vec<Vert>>),
}

impl PartialEq for MeshKind {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other),
            (MeshKind::Box,    MeshKind::Box)
          | (MeshKind::Plane,  MeshKind::Plane)
          | (MeshKind::Sphere, MeshKind::Sphere)
          | (MeshKind::Axes,   MeshKind::Axes)
          // Loaded meshes compare by Arc pointer identity
        ) || matches!((self, other),
            (MeshKind::Loaded(a), MeshKind::Loaded(b)) if Arc::ptr_eq(a, b)
        )
    }
}

// ---------------------------------------------------------------------------
// Geometry — triangle list, each Vert = [x,y,z,  nx,ny,nz,  u,v]
// ---------------------------------------------------------------------------

fn v(x: f32, y: f32, z: f32, nx: f32, ny: f32, nz: f32, u: f32, vv: f32) -> Vert {
    [x, y, z, nx, ny, nz, u, vv]
}

pub fn box_verts() -> Vec<Vert> {
    macro_rules! face {
        ($n:expr, [($ax:expr,$ay:expr,$az:expr),($bx:expr,$by:expr,$bz:expr),
                   ($cx:expr,$cy:expr,$cz:expr),($dx:expr,$dy:expr,$dz:expr)]) => {{
            let n = $n;
            [
                v($ax,$ay,$az, n[0],n[1],n[2], 0.0,0.0),
                v($bx,$by,$bz, n[0],n[1],n[2], 1.0,0.0),
                v($cx,$cy,$cz, n[0],n[1],n[2], 1.0,1.0),
                v($ax,$ay,$az, n[0],n[1],n[2], 0.0,0.0),
                v($cx,$cy,$cz, n[0],n[1],n[2], 1.0,1.0),
                v($dx,$dy,$dz, n[0],n[1],n[2], 0.0,1.0),
            ]
        }};
    }
    let mut out: Vec<Vert> = Vec::new();
    out.extend_from_slice(&face!([0.0, 0.0, 1.0], [(-0.5,-0.5,0.5),(0.5,-0.5,0.5),(0.5,0.5,0.5),(-0.5,0.5,0.5)]));  // +Z
    out.extend_from_slice(&face!([0.0, 0.0,-1.0], [(0.5,-0.5,-0.5),(-0.5,-0.5,-0.5),(-0.5,0.5,-0.5),(0.5,0.5,-0.5)])); // -Z
    out.extend_from_slice(&face!([1.0, 0.0, 0.0], [(0.5,-0.5,0.5),(0.5,-0.5,-0.5),(0.5,0.5,-0.5),(0.5,0.5,0.5)]));   // +X
    out.extend_from_slice(&face!([-1.0,0.0, 0.0], [(-0.5,-0.5,-0.5),(-0.5,-0.5,0.5),(-0.5,0.5,0.5),(-0.5,0.5,-0.5)])); // -X
    out.extend_from_slice(&face!([0.0, 1.0, 0.0], [(-0.5,0.5,0.5),(0.5,0.5,0.5),(0.5,0.5,-0.5),(-0.5,0.5,-0.5)]));   // +Y
    out.extend_from_slice(&face!([0.0,-1.0, 0.0], [(-0.5,-0.5,-0.5),(0.5,-0.5,-0.5),(0.5,-0.5,0.5),(-0.5,-0.5,0.5)])); // -Y
    out
}

pub fn plane_verts() -> Vec<Vert> {
    let n = [0.0f32, 1.0, 0.0];
    vec![
        v(-0.5, 0.0, -0.5, n[0],n[1],n[2], 0.0,0.0),
        v( 0.5, 0.0, -0.5, n[0],n[1],n[2], 1.0,0.0),
        v( 0.5, 0.0,  0.5, n[0],n[1],n[2], 1.0,1.0),
        v(-0.5, 0.0, -0.5, n[0],n[1],n[2], 0.0,0.0),
        v( 0.5, 0.0,  0.5, n[0],n[1],n[2], 1.0,1.0),
        v(-0.5, 0.0,  0.5, n[0],n[1],n[2], 0.0,1.0),
    ]
}

pub fn sphere_verts(rings: u32, segs: u32) -> Vec<Vert> {
    use std::f32::consts::PI;
    let mut out = Vec::new();
    for r in 0..rings {
        let t0 = PI * r as f32 / rings as f32;
        let t1 = PI * (r + 1) as f32 / rings as f32;
        for s in 0..segs {
            let p0 = 2.0 * PI * s as f32 / segs as f32;
            let p1 = 2.0 * PI * (s + 1) as f32 / segs as f32;
            let vert = |t: f32, p: f32| -> Vert {
                let nx = t.sin() * p.cos();
                let ny = t.cos();
                let nz = t.sin() * p.sin();
                let u  = p / (2.0 * PI);
                let vv = t / PI;
                [nx*0.5, ny*0.5, nz*0.5, nx, ny, nz, u, vv]
            };
            let v00 = vert(t0, p0); let v10 = vert(t1, p0);
            let v11 = vert(t1, p1); let v01 = vert(t0, p1);
            if r != 0       { out.extend_from_slice(&[v00, v10, v11]); }
            if r != rings-1 { out.extend_from_slice(&[v00, v11, v01]); }
        }
    }
    out
}

/// (translation_offset, scale, color) for X/Y/Z axis shaft boxes.
pub fn axes_sub_draws() -> [(Vec3, Vec3, [f32; 3]); 3] {
    [
        (Vec3::new(0.5, 0.0, 0.0), Vec3::new(1.0, 0.04, 0.04), [0.9, 0.2, 0.2]),
        (Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.04, 1.0, 0.04), [0.2, 0.85, 0.2]),
        (Vec3::new(0.0, 0.0, 0.5), Vec3::new(0.04, 0.04, 1.0), [0.3, 0.3,  1.0]),
    ]
}
