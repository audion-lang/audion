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

pub mod aui_file;
pub mod runner;
pub mod three;
pub mod three_loader;
pub(crate) mod three_gpu;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

// ---------------------------------------------------------------------------
// Widget style overrides
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct WidgetStyle {
    /// Accent / fill color in sRGB 0-255. Applied to slider fill, button color, pressed keys, etc.
    pub color: Option<[u8; 3]>,
    /// Background color override (white key fill for piano, bg for other widgets).
    pub bg_color: Option<[u8; 3]>,
    /// Explicit widget width in pixels (overrides available-width).
    pub width: Option<f32>,
    /// Explicit widget height in pixels.
    pub height: Option<f32>,
    /// Color for highlighted array cells (playback head). Default: yellow.
    pub highlight_color: Option<[u8; 3]>,
    /// Tab/page visibility — `Some(false)` skips rendering entirely. Default (`None`): visible.
    /// Driven by `.style("visible", 0|1)` for building tabbed UIs in Audion scripts.
    pub visible: Option<bool>,
}

// ---------------------------------------------------------------------------
// 2D canvas draw commands
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum DrawCmd {
    /// Erase all previous commands.
    Clear,
    /// Flood-fill the background with a solid color.
    Fill([u8; 3]),
    /// Filled or outlined rectangle.
    Rect { x: f32, y: f32, w: f32, h: f32, color: [u8; 3], filled: bool },
    /// Filled or outlined circle.
    Circle { cx: f32, cy: f32, r: f32, color: [u8; 3], filled: bool },
    /// Line segment with explicit stroke width.
    Line { x1: f32, y1: f32, x2: f32, y2: f32, color: [u8; 3], width: f32 },
    /// Text string at a position with a given font size.
    Text { x: f32, y: f32, s: String, size: f32, color: [u8; 3] },
}

#[derive(Debug, Default)]
pub struct Canvas2dData {
    /// Complete frame — UI thread reads this (always a finished draw cycle).
    pub cmds: Vec<DrawCmd>,
    /// Audion thread writes here; published to `cmds` on the next `clear()`.
    pub pending: Vec<DrawCmd>,
    pub width: f32,
    pub height: f32,
}

impl Canvas2dData {
    pub fn new(width: f32, height: f32) -> Self {
        Self { cmds: Vec::new(), pending: Vec::new(), width, height }
    }
}


static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub fn ui_registry() -> &'static Mutex<Vec<Arc<UiHandle>>> {
    static R: OnceLock<Mutex<Vec<Arc<UiHandle>>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Vec::new()))
}

// ---------------------------------------------------------------------------
// Window config
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub enum BgImageMode {
    #[default]
    Fill,    // cover: fill screen, maintain aspect ratio, crop edges
    Fit,     // letterbox: show whole image, may have bars
    Center,  // native size, centered
    Stretch, // stretch to fill exactly (ignores aspect ratio)
    Tile,    // repeat/tile (uses TextureWrapMode::Repeat — UVs > 1.0)
}

impl BgImageMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "fit"     => Self::Fit,
            "center"  => Self::Center,
            "stretch" => Self::Stretch,
            "tile"    => Self::Tile,
            _         => Self::Fill,
        }
    }
}

pub struct WindowConfig {
    pub title: String,
    pub width: f32,
    pub height: f32,
    /// Path to the companion .aui file — set by the interpreter when widgets are created.
    pub aui_path: Option<std::path::PathBuf>,
    /// True when ui.window() was just called and InnerSize hasn't been sent yet.
    pub size_dirty: bool,
    /// Solid background color (RGBA 0-255). Painted first, image on top if both set.
    pub bg_color: Option<[u8; 4]>,
    /// Absolute path to a background image.
    pub bg_image: Option<String>,
    pub bg_image_mode: BgImageMode,
    /// Alpha tint for the background image (0-255, default 255 = opaque).
    pub bg_image_alpha: u8,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Audion".to_string(),
            width: 800.0,
            height: 600.0,
            aui_path: None,
            size_dirty: false,
            bg_color: None,
            bg_image: None,
            bg_image_mode: BgImageMode::default(),
            bg_image_alpha: 255,
        }
    }
}

// ---------------------------------------------------------------------------
// Widget kinds
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum WidgetKind {
    SliderH,
    SliderV,
    SliderRange,
    Button,
    Toggle,
    Knob,
    Number,
    Dropdown,
    TextLabel,
    TextInput,
    /// Toggle array — each element is 0 or 1 (bool). Initial size = usize.
    Array(usize),
    /// Float/int array — each element is a draggable number. Initial size = usize.
    ArrayNumbers(usize),
    /// Hardware-rendered 3D canvas via egui_wgpu paint callback.
    ThreeCanvas,
    /// Software-painted 2D canvas — draw commands issued from Audion each frame.
    Canvas2d,
    /// Native OS file-picker button. `filters` is a list of allowed extensions.
    FilePicker { filters: Vec<String> },
    /// Native OS folder-picker button.
    FolderPicker,
    /// Piano keyboard widget — mouse + optional qwerty keyboard input.
    Piano,
}

// ---------------------------------------------------------------------------
// Widget value
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum WidgetValue {
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Vec<bool>),
    ArrayF(Vec<f64>),
    Range(f64, f64),
    Three(Arc<Mutex<three::ThreeSceneData>>),
    Canvas2d(Arc<Mutex<Canvas2dData>>),
    Piano(Arc<Mutex<PianoData>>),
}

impl Default for WidgetValue {
    fn default() -> Self {
        WidgetValue::Float(0.0)
    }
}

// ---------------------------------------------------------------------------
// Widget config (driven by .aui file or defaults)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct WidgetConfig {
    pub kind: WidgetKind,
    pub min: f64,
    pub max: f64,
    pub label: Option<String>,
    pub options: Vec<String>, // dropdown choices
    pub style: WidgetStyle,
    /// Absolute position in the window (set by drag-to-arrange edit mode / .aui file).
    pub x: Option<f32>,
    pub y: Option<f32>,
}

impl WidgetConfig {
    pub fn new(kind: WidgetKind) -> Self {
        Self { kind, min: 0.0, max: 1.0, label: None, options: Vec::new(), style: WidgetStyle::default(), x: None, y: None }
    }
}

// ---------------------------------------------------------------------------
// Widget state — shared between interpreter thread and UI thread
// ---------------------------------------------------------------------------

pub struct WidgetState {
    pub id: String,
    pub value: WidgetValue,
    pub dirty: bool,
    pub config: WidgetConfig,
    /// Cells to illuminate (playback head, etc.) — set by interpreter, rendered distinctly.
    pub highlighted: Vec<usize>,
}

// ---------------------------------------------------------------------------
// Piano widget data
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PianoData {
    pub active_notes:  std::collections::HashSet<u8>,
    pub hold_mode:     bool,
    pub keyboard_mode: bool,
    pub octaves:       u8,
    pub start_note:    u8,
}

impl Default for PianoData {
    fn default() -> Self {
        Self {
            active_notes:  Default::default(),
            hold_mode:     false,
            keyboard_mode: false,
            octaves:       2,
            start_note:    60,  // C4
        }
    }
}

impl WidgetState {
    pub fn new(id: String, config: WidgetConfig) -> Self {
        let value = default_value_for_kind(&config);
        Self { id, value, dirty: false, config, highlighted: Vec::new() }
    }
}

fn default_value_for_kind(config: &WidgetConfig) -> WidgetValue {
    match &config.kind {
        WidgetKind::Toggle | WidgetKind::Button => WidgetValue::Bool(false),
        WidgetKind::TextLabel | WidgetKind::TextInput => WidgetValue::Str(String::new()),
        WidgetKind::Array(n) => WidgetValue::Array(vec![false; *n]),
        WidgetKind::ArrayNumbers(n) => WidgetValue::ArrayF(vec![0.0; *n]),
        WidgetKind::SliderRange => WidgetValue::Range(config.min, config.max),
        WidgetKind::ThreeCanvas => WidgetValue::Three(
            Arc::new(Mutex::new(three::ThreeSceneData::default()))
        ),
        WidgetKind::Canvas2d => WidgetValue::Canvas2d(
            Arc::new(Mutex::new(Canvas2dData::default()))
        ),
        WidgetKind::FilePicker { .. } | WidgetKind::FolderPicker => WidgetValue::Str(String::new()),
        WidgetKind::Piano => WidgetValue::Piano(Arc::new(Mutex::new(PianoData::default()))),
        _ => WidgetValue::Float((config.min + config.max) / 2.0),
    }
}

// ---------------------------------------------------------------------------
// Three canvas registration
// ---------------------------------------------------------------------------

/// Register a 3D canvas in the UiHandle. Returns the shared scene Arc so the
/// interpreter can hold a ThreeRef to it.
pub fn create_canvas(
    handle: &Arc<UiHandle>,
    id: &str,
    width: f32,
    height: f32,
) -> Arc<Mutex<three::ThreeSceneData>> {
    let mut widgets = handle.widgets.lock().unwrap();
    if let Some(existing) = widgets.get(id) {
        // Already created — return the existing scene arc
        let st = existing.lock().unwrap();
        if let WidgetValue::Three(scene_arc) = &st.value {
            return scene_arc.clone();
        }
    }

    let scene = three::ThreeSceneData {
        id: id.to_string(),
        width,
        height,
        ..Default::default()
    };
    let scene_arc = Arc::new(Mutex::new(scene));

    let config = WidgetConfig::new(WidgetKind::ThreeCanvas);
    let state = WidgetState {
        id: id.to_string(),
        value: WidgetValue::Three(scene_arc.clone()),
        dirty: false,
        config,
        highlighted: Vec::new(),
    };
    let state_arc = Arc::new(Mutex::new(state));
    widgets.insert(id.to_string(), state_arc);
    drop(widgets);
    handle.widget_order.lock().unwrap().push(id.to_string());
    scene_arc
}

// ---------------------------------------------------------------------------
// UiHandle — one per ui_desktop() call, one OS window
// ---------------------------------------------------------------------------

pub struct UiHandle {
    pub id: u64,
    pub widgets: Mutex<HashMap<String, Arc<Mutex<WidgetState>>>>,
    pub config: Mutex<WindowConfig>,
    /// Ordered list of widget IDs for stable render order
    pub widget_order: Mutex<Vec<String>>,
}

impl std::fmt::Debug for UiHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UiHandle({})", self.id)
    }
}

impl std::fmt::Debug for WidgetState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WidgetState({})", self.id)
    }
}

impl UiHandle {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            widgets: Mutex::new(HashMap::new()),
            config: Mutex::new(WindowConfig::default()),
            widget_order: Mutex::new(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// Global registry operations
// ---------------------------------------------------------------------------

pub fn create_ui_handle() -> Arc<UiHandle> {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let handle = Arc::new(UiHandle::new(id));
    ui_registry().lock().unwrap().push(handle.clone());
    handle
}

/// Get or create a widget in the given UiHandle. Thread-safe.
pub fn get_or_create_widget(
    handle: &Arc<UiHandle>,
    id: &str,
    config: WidgetConfig,
) -> Arc<Mutex<WidgetState>> {
    let mut widgets = handle.widgets.lock().unwrap();
    if let Some(existing) = widgets.get(id) {
        return existing.clone();
    }
    let state = Arc::new(Mutex::new(WidgetState::new(id.to_string(), config)));
    widgets.insert(id.to_string(), state.clone());
    drop(widgets);
    handle.widget_order.lock().unwrap().push(id.to_string());
    state
}

/// Register a 2D canvas widget. Returns the shared Canvas2dData Arc.
pub fn create_canvas2d(
    handle: &Arc<UiHandle>,
    id: &str,
    width: f32,
    height: f32,
) -> Arc<Mutex<Canvas2dData>> {
    let mut widgets = handle.widgets.lock().unwrap();
    if let Some(existing) = widgets.get(id) {
        let st = existing.lock().unwrap();
        if let WidgetValue::Canvas2d(data_arc) = &st.value {
            return data_arc.clone();
        }
    }

    let data_arc = Arc::new(Mutex::new(Canvas2dData::new(width, height)));
    let mut config = WidgetConfig::new(WidgetKind::Canvas2d);
    config.style.width  = Some(width);
    config.style.height = Some(height);
    let state = WidgetState {
        id: id.to_string(),
        value: WidgetValue::Canvas2d(data_arc.clone()),
        dirty: false,
        config,
        highlighted: Vec::new(),
    };
    let state_arc = Arc::new(Mutex::new(state));
    widgets.insert(id.to_string(), state_arc);
    drop(widgets);
    handle.widget_order.lock().unwrap().push(id.to_string());
    data_arc
}
