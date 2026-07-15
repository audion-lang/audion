# Audion UI Architecture

## Vision

Audion UI turns any `.au` script into an interactive audiovisual performance tool.
The same language that sequences SuperCollider synths now drives sliders, knobs,
3D scenes, and multi-monitor installations — with zero boilerplate.

```au
let ui = ui_desktop();
ui.window("main", 800, 600);

let tempo  = ui.widgets.slider("tempo");
let launch = ui.widgets.button("launch");

thread b {
    loop {
        if launch.has_changed() { bpm(tempo.value()); }
        wait(0.05);
    }
}
fn main() { wait(1); }
```

---

## Threading Model

```
Main thread (macOS requires UI here)
┌──────────────────────────────────────────────────────────┐
│  eframe event loop — renders all UiHandle viewports      │
│  first ui_desktop() → root OS window                     │
│  each extra ui_desktop() → new OS window (viewport)      │
└──────────────────────────────────────────────────────────┘
           ▲  shared:  Arc<Mutex<WidgetState>> per widget
           │
Background thread
┌──────────────────────────────────────────────────────────┐
│  Audion interpreter  (loops, wait(), sequencing, audio)  │
│  ui_desktop() → creates UiHandle → registers globally    │
│  widget.has_changed() / widget.value() → lock-free read  │
└──────────────────────────────────────────────────────────┘
```

Activate with `audion run --ui my_song.au`. Without `--ui`, the interpreter
runs on the main thread as before — zero overhead for non-UI scripts.

---

## Multi-Window

Every `ui_desktop()` call is a new OS window:
- **First call** → owns the root eframe window (title + size set by `ui.window()`)
- **Subsequent calls** → each gets its own OS window via `show_viewport_immediate`

All windows share the same global widget registry. A slider in window A can be
read or written from window B — state is shared via `Arc<Mutex<WidgetState>>`,
no message passing needed.

```au
let ctrl = ui_desktop();
let viz  = ui_desktop();

ctrl.window("Controls", 400, 300);
viz.window("Visuals",   1920, 1080);

let x = ctrl.widgets.slider("x_pos");
// viz.three.mesh(...) reads x.value() — Phase 4
```

---

## Audion API

### Window setup
```au
let ui = ui_desktop();              // new OS window
ui.window("title", width, height);  // set title + size (call once after ui_desktop)
```

### Widget namespace — `ui.widgets.*`

| Method | `.value()` type | Notes |
|--------|----------------|-------|
| `ui.widgets.slider("id")` | `float` | horizontal, range 0–1 by default |
| `ui.widgets.slider_v("id")` | `float` | vertical |
| `ui.widgets.slider_range("id")` | `[lo, hi]` array | two-handle range |
| `ui.widgets.knob("id")` | `float` | drag-value placeholder; custom painter Phase 4 |
| `ui.widgets.button("id")` | `bool` | one-shot — true once per click |
| `ui.widgets.toggle("id")` | `bool` | latching on/off |
| `ui.widgets.number("id")` | `float` | draggable number input |
| `ui.widgets.dropdown("id", "A", "B", "C")` | `float` | index of selected option |
| `ui.widgets.text_label("id", "initial text")` | `string` | display-only label |
| `ui.widgets.text_input("id")` | `string` | editable single-line text |
| `ui.widgets.array("id", n)` | `[bool …]` | toggle array; **+** / **–** resize, min 1 |
| `ui.widgets.array_numbers("id", n)` | `[float …]` | float array; **+** / **–** resize, min 1 |

All widgets share the same range defaults (`min=0`, `max=1`).
Override per-widget in the `.aui` config file (Phase 5).

### Widget methods
```au
widget.value()        // → current value (float / bool / string / [float] / [bool])
widget.has_changed()  // → bool, ONE-SHOT: clears dirty flag on read
widget.set(v)         // → set value programmatically from code
```

### Example
```au
let ui = ui_desktop();
ui.window("Studio", 600, 400);

let tempo   = ui.widgets.slider("tempo");
let launch  = ui.widgets.button("launch");
let enabled = ui.widgets.toggle("enabled");
let mode    = ui.widgets.dropdown("mode", "Poly", "Mono", "Duo");
let status  = ui.widgets.text_label("status", "ready");
let pattern = ui.widgets.array("pattern", 8);
let freqs   = ui.widgets.array_numbers("freqs", 4);

tempo.set(0.64);  // 0–1 range → maps to your bpm range in code

thread ui_poll {
    loop {
        if launch.has_changed() {
            status.set("playing");
        }
        if tempo.has_changed() {
            bpm(60 + tempo.value() * 140);
        }
        wait(0.05);
    }
}

fn main() { wait(1); }
```

### 3D namespace — `ui.three.*`  _(Phase 4)_
```au
ui.three.mesh(...)
ui.three.camera(...)
ui.three.shader(...)
```
Full three-d facade — Rust layer wraps the `three-d` crate.

---

## `.aui` Config File  _(Phase 5)_

For every `my_song.au`, a `my_song.aui` TOML file stores per-widget overrides.
Auto-generated with defaults on first run if absent.

```toml
[slider.tempo]
min     = 60.0
max     = 200.0
default = 128.0
label   = "Tempo (BPM)"

[slider.tempo.style]
color = "#ff6600"
width = 240.0

[dropdown.mode]
label = "Voice Mode"

[array.pattern]
default = [1,0,1,0,1,0,1,0]
```

Widget IDs come from the first argument to each `ui.widgets.*()` call.
Unknown IDs are appended with type-defaults on next run.

---

## Value Types in the Interpreter

Three `Value` variants added (non-breaking, purely additive):

| Variant | Description |
|---------|-------------|
| `Value::UiContext(Arc<UiHandle>)` | The `ui` object returned by `ui_desktop()` |
| `Value::UiNs(Arc<UiHandle>, String)` | Sub-namespace: `ui.widgets`, `ui.three` |
| `Value::WidgetRef(Arc<Mutex<WidgetState>>)` | Widget object returned by `ui.widgets.*()` |

`WidgetValue` enum covers all value types:
`Float(f64)` · `Bool(bool)` · `Str(String)` · `Array(Vec<bool>)` · `ArrayF(Vec<f64>)` · `Range(f64, f64)`

Method dispatch is intercepted in `Expr::Call` before the callee is resolved —
the receiver context (which UiHandle) is always available, no indirection needed.

---

## File Structure

```
src/ui/
  mod.rs        — UiHandle, WidgetState, WidgetValue, WidgetKind, global registry
  runner.rs     — eframe AudionUiApp, per-viewport rendering, all widget renderers
  aui_file.rs   — .aui TOML stub (Phase 5)
UI.md           — this file
examples/ui_demo.au
```

---

## Phase Roadmap

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | `--ui` flag · interpreter on bg thread · eframe on main · global registry | ✅ done |
| 2 | Core: `ui_desktop()` · `ui.window()` · multi-OS-window · `.value()` · `.has_changed()` · `.set()` | ✅ done |
| 3 | Full widget set: all 12 types including array +/– resize · `examples/ui_demo.au` | ✅ done |
| 4 | `ui.three.*` facade — GPU 3D canvas via `egui_wgpu` paint callback (WGSL Phong shader, box/plane/sphere/axes, Painter's-algorithm depth sort) | ✅ done |
| 5 | Custom shaders · OBJ/GLB loading · textures · per-mesh uniforms (`custom0/1`) | ✅ done |
| 6 | `.aui` config — TOML per-widget overrides (min/max/label/style), auto-generate, hot reload | 🔲 planned |
| 6 | Accessibility — keyboard nav audit, screen-reader annotations | 🔲 planned |

---

## Known Constraints

**Threading / window**
- `--ui` flag required: eframe must own the main thread (macOS mandate).
- `--ui` forces the wgpu backend (`eframe::Renderer::Wgpu`). glow/OpenGL fallback is not available when using `ui.three.*`.

**3D rendering**
- Depth sorting is painter's algorithm at the **mesh level** (not per-triangle). Intersecting meshes render incorrectly; non-intersecting scenes are fine.
- No depth buffer in the egui paint callback pass — full GPU depth buffering would require a separate off-screen render target (not yet implemented).
- Viewport positioning is baked into the MVP matrix (no `set_viewport`) to avoid wgpu panics from Retina-display overflow. If you notice sub-pixel position drift at extreme panel offsets, this is why.

**Custom shaders**
- `scene.shader()` (fragment-only mode) injects the standard `Uniforms` struct + vertex shader as a prelude. Your WGSL must define `@fragment fn fs(in: VOut) -> @location(0) vec4<f32>` — no other entry points.
- `scene.shader_full()` owns the whole module; it must replicate the 208-byte `Uniforms` struct layout exactly or uniform values will be garbled.
- Custom shaders compile on the **first frame** they're used — expect a one-frame hitch. Compilation errors go to stderr only (no in-scene feedback yet).
- Only one custom shader recompile per shader name: if you call `scene.shader()` again with the same name, the GPU pipeline is **not** recompiled. Restart the script to pick up edits.

**Model loading**
- OBJ: no MTL/material support. Colors and textures must be set via `scene.color()` / `scene.mesh_texture()`. Tangents are not computed.
- GLTF/GLB: only `Triangles` primitive mode is loaded; strips, fans, and morph targets are ignored. Prefer `.glb` (self-contained binary) over `.gltf` + external `.bin`.
- Models load synchronously on the interpreter thread — large files will block the audio loop momentarily.

**Textures**
- Texture format is always `Rgba8UnormSrgb`. Greyscale and HDR images are upconverted by the `image` crate.
- Textures are uploaded once and cached — calling `scene.texture()` again with the same name does nothing (restart to reload).

**Knob widget**
- Still a `DragValue` placeholder; a circular painter implementation is a standalone task unrelated to the 3D work.
