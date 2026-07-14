# Audion UI Architecture

## Vision

Audion UI turns any `.au` script into an interactive audiovisual performance tool.
The same language that sequences SuperCollider synths now drives sliders, buttons, 3D scenes, and multi-monitor installations — with zero boilerplate.

```au
let ui = ui_desktop();
ui.window("main", 800, 600);

let tempo  = ui.widgets.slider("tempo");
let launch = ui.widgets.button("launch");

loop {
    if launch.has_changed() { bpm(tempo.value()); }
    wait(0.05);
}
```

---

## Threading Model

```
Main thread (macOS requires UI here)
┌──────────────────────────────────────────────────────────┐
│  eframe event loop  ←──── renders all UiHandle viewports │
│  (one viewport per ui_desktop() call)                    │
└──────────────────────────────────────────────────────────┘
           ▲  shared:  Arc<Mutex<WidgetState>> per widget
           │
Background thread
┌──────────────────────────────────────────────────────────┐
│  Audion interpreter  (loops, wait(), sequencing, audio)  │
│  ui_desktop() → creates UiHandle → registers globally   │
│  widget.has_changed() / widget.value() → lock-free read  │
└──────────────────────────────────────────────────────────┘
```

UI is activated with `audion run --ui my_song.au`. Without `--ui`, the interpreter
runs on the main thread as before (zero overhead for non-UI scripts).

---

## Multi-Window Strategy

Every call to `ui_desktop()` spawns a **new OS window** (eframe viewport).
All windows share the same **global widget registry** — a slider in window A
can be read from window B:

```au
let ctrl  = ui_desktop();  // window 0: control surface
let viz   = ui_desktop();  // window 1: visualizer

ctrl.window("Controls", 400, 300);
viz.window("Visuals",   1920, 1080);

let x = ctrl.widgets.slider("x_pos");
// viz draws based on x.value()...
```

Windows communicate through the shared `Arc<Mutex<WidgetState>>` — no message
passing needed, the state is directly shared.

---

## Audion API

### Window setup
```au
let ui = ui_desktop();           // creates a new OS window + UiHandle
ui.window("title", width, height); // set window dimensions (call once)
```

### Widget namespace  `ui.widgets.*`
| Call | Returns | `.value()` type |
|------|---------|-----------------|
| `ui.widgets.slider("id")` | WidgetRef | float |
| `ui.widgets.slider_v("id")` | WidgetRef | float (vertical) |
| `ui.widgets.slider_range("id")` | WidgetRef | [min, max] array |
| `ui.widgets.knob("id")` | WidgetRef | float |
| `ui.widgets.button("id")` | WidgetRef | bool (one-shot) |
| `ui.widgets.toggle("id")` | WidgetRef | bool |
| `ui.widgets.number("id")` | WidgetRef | float |
| `ui.widgets.dropdown("id")` | WidgetRef | float (selected index) |
| `ui.widgets.text_label("id")` | WidgetRef | — (display only) |
| `ui.widgets.text_input("id")` | WidgetRef | string |
| `ui.widgets.array("id", n)` | WidgetRef | bool array length n |

### Widget object methods
```au
widget.value()        // → current value (float / bool / string / array)
widget.has_changed()  // → bool, ONE-SHOT: clears dirty flag on read
widget.set(v)         // → set value programmatically (e.g. defaults)
```

### 3D namespace  `ui.three.*`  _(Phase 5)_
```au
ui.three.mesh(...)
ui.three.camera(...)
ui.three.shader(...)
```
Full three-d facade — the Rust layer wraps `three-d` crate and keeps the API
in sync as the library updates.

---

## `.aui` Config File

For every `my_song.au`, a `my_song.aui` TOML file stores per-widget config.
Auto-generated with defaults on first run if absent.

```toml
[slider.tempo]
min     = 60.0
max     = 200.0
default = 128.0
label   = "Tempo (BPM)"
position = [20, 40]

[slider.tempo.style]
color = "#ff6600"
width = 240.0

[button.launch]
label = "Launch!"
position = [20, 100]
```

Widget IDs come from the first argument to each `ui.widgets.*()` call.
Unknown IDs are appended with their type-defaults when the script runs.

---

## Value Types in the Interpreter

Three new `Value` variants (non-breaking, purely additive):

| Variant | Description |
|---------|-------------|
| `Value::UiContext(Arc<UiHandle>)` | The `ui` object returned by `ui_desktop()` |
| `Value::UiNs(Arc<UiHandle>, String)` | Sub-namespace: `ui.widgets`, `ui.three` |
| `Value::WidgetRef(Arc<Mutex<WidgetState>>)` | Widget object returned by `ui.widgets.*()` |

Method dispatch is intercepted in `Expr::Call` before the callee is resolved
to a `Value`, so no `Value::BuiltinFn` indirection is needed and the receiver
context (which UiHandle) is always available.

---

## File Structure

```
src/ui/
  mod.rs        — UiHandle, WidgetState, WidgetValue, WidgetKind, global registry
  runner.rs     — eframe AudionUiApp, multi-viewport rendering, widget egui render
  aui_file.rs   — .aui TOML load / auto-generate
```

---

## Phase Roadmap & Progress

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | Threading: `--ui` flag, interpreter→bg thread, eframe on main, global registry | ✅ done |
| 2 | Slider + Button end-to-end: `ui_desktop()`, `ui.window()`, `ui.widgets.slider/button`, `.value()`, `.has_changed()`, `.set()` | ✅ done |
| 3 | Full MVP widget set: slider_v, slider_range, knob, toggle, number, dropdown, text_label, text_input, array | ⏳ next |
| 4 | `.aui` file: read TOML config, auto-generate with defaults, live reload | ⏳ next |
| 5 | `ui.three.*` facade: integrate `three-d` crate for 3D rendering on the canvas | 🔲 planned |
| 6 | Accessibility: keyboard navigation audit, screen-reader annotations | 🔲 planned |

---

## Known Constraints

- `--ui` flag required: eframe must run on the main thread (macOS mandate). Without
  the flag the interpreter runs on the main thread as before.
- one eframe process hosts all viewports; true "20 windows" multi-monitor art
  installations work because each `ui_desktop()` = one eframe viewport on its
  own OS window.
- three-d integration (Phase 5) will migrate the render backend from eframe's
  wgpu to three-d's wgpu context; the egui layer stays identical.
