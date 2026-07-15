// Copyright (C) 2025-2026 Aleksandr Bogdanov — GPL-3.0-or-later
//
// GPU backend for ui.three.* — wgpu/Metal via eframe's egui_wgpu integration.
//
// Pipeline layout (same for ALL pipelines — default, textured, custom):
//   @group(0) @binding(0)  Uniforms       (per-mesh uniform buffer)
//   @group(1) @binding(0)  texture_2d     (mesh texture or 1×1 white fallback)
//   @group(1) @binding(1)  sampler        (linear/repeat)
//
// Custom shaders can access every binding; unused bindings are harmlessly ignored.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::{egui_wgpu, wgpu};
use glam::{Mat4, Vec3, Vec4};

use super::three::{axes_sub_draws, box_verts, plane_verts, sphere_verts, MeshKind, ShaderEntry, ThreeSceneData};

// ---------------------------------------------------------------------------
// Compile-time size assertion — keeps Rust ↔ WGSL layout honest
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    // All vec4 / mat4 fields first — each is 16-byte aligned in both Rust and WGSL,
    // so there is zero implicit padding in the WGSL std140 layout.
    mvp:       [[f32; 4]; 4],  // offset   0 — 64 bytes
    model:     [[f32; 4]; 4],  // offset  64 — 64 bytes
    color:     [f32; 4],       // offset 128 — 16 bytes
    light_dir: [f32; 4],       // offset 144 — 16 bytes  (w unused)
    custom0:   [f32; 4],       // offset 160 — 16 bytes  (u.custom0)
    custom1:   [f32; 4],       // offset 176 — 16 bytes  (u.custom1)
    // Scalars last.  Rust aligns [f32;2] to 4 bytes; WGSL aligns vec2 to 8.
    // Placing them here (offset 192+) keeps both layouts identical.
    time:      f32,            // offset 192 —  4 bytes
    _pad0:     f32,            // offset 196 —  4 bytes  (ensures vec2 lands at 200, align-8 ✓)
    uv_scale:  [f32; 2],       // offset 200 —  8 bytes
}                              // total = 208 bytes

const _: () = assert!(std::mem::size_of::<Uniforms>() == 208);

// ---------------------------------------------------------------------------
// WGSL — injected as a prelude when the user supplies only a fragment function.
// A user writing a fragment shader has access to every symbol defined here.
// ---------------------------------------------------------------------------

const SHADER_PRELUDE: &str = r#"
struct Uniforms {
    mvp:       mat4x4<f32>,   //   0 — 64 bytes
    model:     mat4x4<f32>,   //  64 — 64 bytes
    color:     vec4<f32>,     // 128 — 16 bytes
    light_dir: vec4<f32>,     // 144 — 16 bytes
    custom0:   vec4<f32>,     // 160 — 16 bytes
    custom1:   vec4<f32>,     // 176 — 16 bytes
    time:      f32,           // 192 —  4 bytes
    _pad0:     f32,           // 196 —  4 bytes (aligns uv_scale to 8)
    uv_scale:  vec2<f32>,     // 200 —  8 bytes
}                             // 208 bytes total, no implicit padding

@group(0) @binding(0) var<uniform> u:       Uniforms;
@group(1) @binding(0) var          t_color: texture_2d<f32>;
@group(1) @binding(1) var          s_color: sampler;

struct VIn  {
    @location(0) pos:  vec3<f32>,
    @location(1) norm: vec3<f32>,
    @location(2) uv:   vec2<f32>,
}
struct VOut {
    @builtin(position) clip:  vec4<f32>,
    @location(0)       wnorm: vec3<f32>,
    @location(1)       uv:    vec2<f32>,
}

@vertex fn vs(in: VIn) -> VOut {
    var o: VOut;
    o.clip  = u.mvp * vec4<f32>(in.pos, 1.0);
    o.wnorm = normalize((u.model * vec4<f32>(in.norm, 0.0)).xyz);
    o.uv    = in.uv * u.uv_scale;
    return o;
}
"#;

// Default fragment — Phong, ignores texture
const FS_DEFAULT: &str = r#"
@fragment fn fs(in: VOut) -> @location(0) vec4<f32> {
    let diff       = max(dot(in.wnorm, normalize(u.light_dir.xyz)), 0.0);
    let brightness = 0.15 + 0.85 * diff;
    return vec4<f32>(u.color.rgb * brightness, u.color.a);
}
"#;

// Textured fragment — samples texture, applies Phong
const FS_TEXTURED: &str = r#"
@fragment fn fs(in: VOut) -> @location(0) vec4<f32> {
    let tex        = textureSample(t_color, s_color, in.uv);
    let diff       = max(dot(in.wnorm, normalize(u.light_dir.xyz)), 0.0);
    let brightness = 0.15 + 0.85 * diff;
    return vec4<f32>(tex.rgb * u.color.rgb * brightness, tex.a * u.color.a);
}
"#;

fn build_shader_src(entry: &ShaderEntry) -> String {
    match entry {
        ShaderEntry::Fragment(fs) => format!("{SHADER_PRELUDE}\n{fs}"),
        ShaderEntry::Full(full)   => full.clone(),
    }
}

// ---------------------------------------------------------------------------
// GPU resource bundles
// ---------------------------------------------------------------------------

struct MeshUniform {
    buffer:     wgpu::Buffer,
    bind_group: wgpu::BindGroup,  // BG 0
}

struct GpuTexture {
    _texture:   wgpu::Texture,
    bind_group: wgpu::BindGroup,  // BG 1
}

#[allow(clippy::upper_case_acronyms)]
enum DrawKind { Box, Plane, Sphere, Loaded }

struct DrawCmd {
    mesh_id:    String,
    kind:       DrawKind,
    /// Key into custom_pipelines, or None → default / textured pipeline.
    shader_key: Option<String>,
    /// Key into gpu_textures, or None → white fallback.
    texture_id: Option<String>,
}

// ---------------------------------------------------------------------------
// ThreePipeline — stored in egui_wgpu::CallbackResources
// ---------------------------------------------------------------------------

pub struct ThreePipeline {
    // Bind group layouts
    bgl_uniform:  wgpu::BindGroupLayout,
    bgl_texture:  wgpu::BindGroupLayout,

    // Pipelines
    pipeline_default:  wgpu::RenderPipeline,
    pipeline_textured: wgpu::RenderPipeline,
    custom_pipelines:  HashMap<String, wgpu::RenderPipeline>,

    // Static geometry vertex buffers
    box_vbuf:      wgpu::Buffer, box_vcount:    u32,
    plane_vbuf:    wgpu::Buffer, plane_vcount:  u32,
    sphere_vbuf:   wgpu::Buffer, sphere_vcount: u32,

    // 1×1 white fallback texture (bound when mesh has no texture)
    white_tex_bg: wgpu::BindGroup,
    // Cache: (canvas_id, mesh_id) → GPU uniforms
    mesh_uniforms: HashMap<(String, String), MeshUniform>,
    // Cache: texture_id → uploaded GPU texture
    gpu_textures:  HashMap<String, GpuTexture>,
    // Cache: (canvas_id, mesh_id) → vertex buffer for Loaded meshes
    loaded_vbufs:  HashMap<(String, String), (wgpu::Buffer, u32)>,

    // Per-canvas draw lists: built in prepare(), consumed in paint() same frame
    draw_lists:    HashMap<String, Vec<DrawCmd>>,

    start_time:     Instant,
    /// Surface format from init() — used when compiling custom shader pipelines.
    target_format:  wgpu::TextureFormat,
}

// ---------------------------------------------------------------------------
// init() — called once from AudionUiApp::new()
// ---------------------------------------------------------------------------

pub fn init(
    device: &wgpu::Device,
    queue:  &wgpu::Queue,
    target_format: wgpu::TextureFormat,
    resources: &mut egui_wgpu::CallbackResources,
) {
    // --- Bind group layouts ---
    let bgl_uniform = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:   Some("three_bgl_uniform"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding:    0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size:   wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
            },
            count: None,
        }],
    });

    let bgl_texture = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:   Some("three_bgl_tex"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type:    wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled:   false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding:    1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:                Some("three_pl"),
        bind_group_layouts:   &[&bgl_uniform, &bgl_texture],
        push_constant_ranges: &[],
    });

    // Shared vertex buffer layout (pos + norm + uv = 8 floats)
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: (8 * 4) as wgpu::BufferAddress,
        step_mode:    wgpu::VertexStepMode::Vertex,
        attributes:   &[
            wgpu::VertexAttribute { offset:  0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
        ],
    };

    let make_pipeline = |device: &wgpu::Device, src: &str, label: &str| {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some(label),
            source: wgpu::ShaderSource::Wgsl(src.into()),
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:               &module,
                entry_point:          Some("vs"),
                buffers:              &[vertex_layout.clone()],
                compilation_options:  Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology:  wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module:              &module,
                entry_point:         Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format:     target_format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache:     None,
        })
    };

    let pipeline_default  = make_pipeline(device, &format!("{SHADER_PRELUDE}\n{FS_DEFAULT}"),  "three_default");
    let pipeline_textured = make_pipeline(device, &format!("{SHADER_PRELUDE}\n{FS_TEXTURED}"), "three_textured");

    // --- Static geometry ---
    let upload = |verts: Vec<super::three::Vert>| -> (wgpu::Buffer, u32) {
        let flat: Vec<f32> = verts.iter().flat_map(|v| v.iter().copied()).collect();
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("three_geo"),
            size:               (flat.len() * 4) as u64,
            usage:              wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        buf.slice(..).get_mapped_range_mut().copy_from_slice(bytemuck::cast_slice(&flat));
        buf.unmap();
        (buf, verts.len() as u32)
    };

    let (box_vbuf,    box_vcount)    = upload(box_verts());
    let (plane_vbuf,  plane_vcount)  = upload(plane_verts());
    let (sphere_vbuf, sphere_vcount) = upload(sphere_verts(16, 24));

    // --- 1×1 white fallback texture ---
    let white_tex_bg = create_gpu_texture(device, queue, &[255u8, 255, 255, 255], 1, 1, &bgl_texture).bind_group;

    resources.insert(ThreePipeline {
        bgl_uniform, bgl_texture,
        pipeline_default, pipeline_textured,
        custom_pipelines: HashMap::new(),
        box_vbuf, box_vcount,
        plane_vbuf, plane_vcount,
        sphere_vbuf, sphere_vcount,
        white_tex_bg,
        mesh_uniforms: HashMap::new(),
        gpu_textures:  HashMap::new(),
        loaded_vbufs:  HashMap::new(),
        draw_lists:    HashMap::new(),
        start_time:    Instant::now(),
        target_format,
    });
}

// ---------------------------------------------------------------------------
// Per-frame callback
// ---------------------------------------------------------------------------

pub struct ThreeCallback {
    pub canvas_id:    String,
    pub scene:        Arc<Mutex<ThreeSceneData>>,
    pub viewport_size: [f32; 2],
    pub egui_rect:    eframe::egui::Rect,
}

impl egui_wgpu::CallbackTrait for ThreeCallback {
    fn prepare(
        &self,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
        screen:  &egui_wgpu::ScreenDescriptor,
        _enc:    &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(pipe) = resources.get_mut::<ThreePipeline>() else { return Vec::new(); };
        let scene = self.scene.lock().unwrap();

        // Reset draw list for this canvas
        pipe.draw_lists.entry(self.canvas_id.clone()).or_default().clear();

        // ── Lazy-upload new textures ───────────────────────────────────────
        for (name, entry) in &scene.textures {
            if !pipe.gpu_textures.contains_key(name) {
                let gt = create_gpu_texture(device, queue, &entry.pixels, entry.width, entry.height, &pipe.bgl_texture);
                pipe.gpu_textures.insert(name.clone(), gt);
            }
        }

        // ── Lazy-compile new custom shaders ───────────────────────────────
        let ppp = screen.pixels_per_point;
        let sw  = screen.size_in_pixels[0] as f32;
        let sh  = screen.size_in_pixels[1] as f32;
        let target_format = pipe.target_format;
        for (name, entry) in &scene.shaders {
            if !pipe.custom_pipelines.contains_key(name) {
                let src = build_shader_src(entry);
                if let Some(pl) = compile_pipeline(device, &src, &pipe.bgl_uniform, &pipe.bgl_texture, target_format) {
                    pipe.custom_pipelines.insert(name.clone(), pl);
                }
            }
        }

        if scene.meshes.is_empty() { return Vec::new(); }

        // ── VP matrix with viewport transform baked in ────────────────────
        let vp_x = self.egui_rect.min.x * ppp;
        let vp_y = self.egui_rect.min.y * ppp;
        let vp_w = (self.egui_rect.width()  * ppp).max(1.0);
        let vp_h = (self.egui_rect.height() * ppp).max(1.0);
        let scale_x = vp_w / sw;
        let scale_y = vp_h / sh;
        let bias_x  = (2.0 * vp_x + vp_w - sw) / sw;
        let bias_y  = (sh - 2.0 * vp_y - vp_h) / sh;
        let vp_transform = Mat4::from_cols(
            Vec4::new(scale_x, 0.0, 0.0, 0.0),
            Vec4::new(0.0, scale_y, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(bias_x, bias_y, 0.0, 1.0),
        );

        let aspect = vp_w / vp_h;
        #[allow(deprecated)]
        let proj = Mat4::perspective_rh(scene.camera.fov_deg.to_radians(), aspect, 0.01, 10_000.0);
        #[allow(deprecated)]
        let view = Mat4::look_at_rh(scene.camera.eye, scene.camera.target, scene.camera.up);
        let vp   = vp_transform * proj * view;

        let light_dir = Vec3::new(0.5, 1.0, 0.7).normalize();
        let elapsed   = pipe.start_time.elapsed().as_secs_f32();

        // ── Sort back-to-front ────────────────────────────────────────────
        let mut order: Vec<(usize, f32)> = scene.meshes.iter().enumerate()
            .filter(|(_, m)| m.visible)
            .map(|(i, m)| {
                let z = (view * Vec4::new(m.position.x, m.position.y, m.position.z, 1.0)).z;
                (i, z)
            })
            .collect();
        order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        for (idx, _) in order {
            let mesh = &scene.meshes[idx];

            // ── Axes: expand to 3 coloured box sub-draws ─────────────────
            if mesh.kind == MeshKind::Axes {
                let base = mesh.model_matrix();
                for (offset, scale, color) in axes_sub_draws() {
                    let sub_model = base * Mat4::from_translation(offset) * Mat4::from_scale(scale);
                    let sub_id = format!("{}__ax_{:.0}{:.0}{:.0}", mesh.id, color[0]*9.0, color[1]*9.0, color[2]*9.0);
                    upload_mesh_uniform(device, queue, pipe, &self.canvas_id, &sub_id,
                        vp * sub_model, sub_model, [color[0],color[1],color[2],1.0], light_dir,
                        elapsed, [1.0,1.0], [0.0;4], [0.0;4]);
                    push_draw(pipe, &self.canvas_id, &sub_id, DrawKind::Box, None, None);
                }
                continue;
            }

            // ── Upload / refresh vertex buffer for Loaded meshes ──────────
            if let MeshKind::Loaded(verts) = &mesh.kind {
                let key = (self.canvas_id.clone(), mesh.id.clone());
                if !pipe.loaded_vbufs.contains_key(&key) {
                    let flat: Vec<f32> = verts.iter().flat_map(|v| v.iter().copied()).collect();
                    let buf = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("three_loaded"),
                        size:  (flat.len() * 4) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: true,
                    });
                    buf.slice(..).get_mapped_range_mut().copy_from_slice(bytemuck::cast_slice(&flat));
                    buf.unmap();
                    pipe.loaded_vbufs.insert(key, (buf, verts.len() as u32));
                }
            }

            let model    = mesh.model_matrix();
            let draw_kind = match &mesh.kind {
                MeshKind::Box    => DrawKind::Box,
                MeshKind::Plane  => DrawKind::Plane,
                MeshKind::Sphere => DrawKind::Sphere,
                MeshKind::Loaded(_) => DrawKind::Loaded,
                MeshKind::Axes   => unreachable!(),
            };
            upload_mesh_uniform(device, queue, pipe, &self.canvas_id, &mesh.id,
                vp * model, model,
                [mesh.color[0], mesh.color[1], mesh.color[2], 1.0],
                light_dir, elapsed, mesh.uv_scale, mesh.custom0, mesh.custom1);

            push_draw(pipe, &self.canvas_id, &mesh.id, draw_kind,
                mesh.shader_id.clone(), mesh.texture_id.clone());
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        rp: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(pipe) = resources.get::<ThreePipeline>() else { return; };
        let Some(draws) = pipe.draw_lists.get(&self.canvas_id) else { return; };
        if draws.is_empty() { return; }

        for cmd in draws {
            let ukey = (self.canvas_id.clone(), cmd.mesh_id.clone());
            let Some(mu) = pipe.mesh_uniforms.get(&ukey) else { continue; };

            // Select pipeline
            let pipeline = if let Some(ref sk) = cmd.shader_key {
                pipe.custom_pipelines.get(sk).unwrap_or(&pipe.pipeline_default)
            } else if cmd.texture_id.is_some() {
                &pipe.pipeline_textured
            } else {
                &pipe.pipeline_default
            };

            // Texture bind group (BG 1) — use assigned texture or 1×1 white fallback
            let tex_bg = cmd.texture_id.as_ref()
                .and_then(|id| pipe.gpu_textures.get(id))
                .map(|t| &t.bind_group)
                .unwrap_or(&pipe.white_tex_bg);

            rp.set_pipeline(pipeline);
            rp.set_bind_group(0, &mu.bind_group, &[]);
            rp.set_bind_group(1, tex_bg, &[]);

            match cmd.kind {
                DrawKind::Box    => { rp.set_vertex_buffer(0, pipe.box_vbuf.slice(..));    rp.draw(0..pipe.box_vcount,    0..1); }
                DrawKind::Plane  => { rp.set_vertex_buffer(0, pipe.plane_vbuf.slice(..));  rp.draw(0..pipe.plane_vcount,  0..1); }
                DrawKind::Sphere => { rp.set_vertex_buffer(0, pipe.sphere_vbuf.slice(..)); rp.draw(0..pipe.sphere_vcount, 0..1); }
                DrawKind::Loaded => {
                    if let Some((vbuf, vcount)) = pipe.loaded_vbufs.get(&ukey) {
                        rp.set_vertex_buffer(0, vbuf.slice(..));
                        rp.draw(0..*vcount, 0..1);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn upload_mesh_uniform(
    device: &wgpu::Device,
    queue:  &wgpu::Queue,
    pipe:   &mut ThreePipeline,
    canvas_id: &str,
    mesh_id:   &str,
    mvp:   Mat4,
    model: Mat4,
    color: [f32; 4],
    light_dir: Vec3,
    time:      f32,
    uv_scale:  [f32; 2],
    custom0:   [f32; 4],
    custom1:   [f32; 4],
) {
    let u = Uniforms {
        mvp:       mvp.to_cols_array_2d(),
        model:     model.to_cols_array_2d(),
        color,
        light_dir: [light_dir.x, light_dir.y, light_dir.z, 0.0],
        custom0,
        custom1,
        time,
        _pad0:     0.0,
        uv_scale,
    };
    let key = (canvas_id.to_string(), mesh_id.to_string());
    if !pipe.mesh_uniforms.contains_key(&key) {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("three_uniforms"),
            size:  std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("three_bg_uniform"),
            layout:  &pipe.bgl_uniform,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        });
        pipe.mesh_uniforms.insert(key.clone(), MeshUniform { buffer: buf, bind_group: bg });
    }
    queue.write_buffer(&pipe.mesh_uniforms[&key].buffer, 0, bytemuck::bytes_of(&u));
}

fn push_draw(pipe: &mut ThreePipeline, canvas_id: &str, mesh_id: &str, kind: DrawKind, shader_key: Option<String>, texture_id: Option<String>) {
    pipe.draw_lists.entry(canvas_id.to_string()).or_default().push(DrawCmd {
        mesh_id: mesh_id.to_string(),
        kind,
        shader_key,
        texture_id,
    });
}

fn create_gpu_texture(
    device: &wgpu::Device,
    queue:  &wgpu::Queue,
    pixels: &[u8],
    width:  u32,
    height: u32,
    bgl:    &wgpu::BindGroupLayout,
) -> GpuTexture {
    let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
    let tex  = device.create_texture(&wgpu::TextureDescriptor {
        label:           Some("three_tex"),
        size,
        mip_level_count: 1,
        sample_count:    1,
        dimension:       wgpu::TextureDimension::D2,
        format:          wgpu::TextureFormat::Rgba8UnormSrgb,
        usage:           wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats:    &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        pixels,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: Some(height) },
        size,
    );
    let view    = tex.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label:            Some("three_sampler"),
        address_mode_u:   wgpu::AddressMode::Repeat,
        address_mode_v:   wgpu::AddressMode::Repeat,
        mag_filter:       wgpu::FilterMode::Linear,
        min_filter:       wgpu::FilterMode::Linear,
        mipmap_filter:    wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label:   Some("three_bg_tex"),
        layout:  bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    });
    GpuTexture { _texture: tex, bind_group }
}

fn compile_pipeline(
    device: &wgpu::Device,
    src:    &str,
    bgl_u:  &wgpu::BindGroupLayout,
    bgl_t:  &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> Option<wgpu::RenderPipeline> {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label:  Some("three_custom"),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:                Some("three_custom_pl"),
        bind_group_layouts:   &[bgl_u, bgl_t],
        push_constant_ranges: &[],
    });
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: (8 * 4) as wgpu::BufferAddress,
        step_mode:    wgpu::VertexStepMode::Vertex,
        attributes:   &[
            wgpu::VertexAttribute { offset:  0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
        ],
    };
    Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:  Some("three_custom_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module, entry_point: Some("vs"),
            buffers: &[vertex_layout], compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: None,
        multisample:   wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module, entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None, cache: None,
    }))
}
