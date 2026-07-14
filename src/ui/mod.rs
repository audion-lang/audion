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

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub fn ui_registry() -> &'static Mutex<Vec<Arc<UiHandle>>> {
    static R: OnceLock<Mutex<Vec<Arc<UiHandle>>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Vec::new()))
}

// ---------------------------------------------------------------------------
// Window config
// ---------------------------------------------------------------------------

pub struct WindowConfig {
    pub title: String,
    pub width: f32,
    pub height: f32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self { title: "Audion".to_string(), width: 800.0, height: 600.0 }
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
}

impl WidgetConfig {
    pub fn new(kind: WidgetKind) -> Self {
        Self { kind, min: 0.0, max: 1.0, label: None, options: Vec::new() }
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
}

impl WidgetState {
    pub fn new(id: String, config: WidgetConfig) -> Self {
        let value = default_value_for_kind(&config);
        Self { id, value, dirty: false, config }
    }
}

fn default_value_for_kind(config: &WidgetConfig) -> WidgetValue {
    match &config.kind {
        WidgetKind::Toggle | WidgetKind::Button => WidgetValue::Bool(false),
        WidgetKind::TextLabel | WidgetKind::TextInput => WidgetValue::Str(String::new()),
        WidgetKind::Array(n) => WidgetValue::Array(vec![false; *n]),
        WidgetKind::ArrayNumbers(n) => WidgetValue::ArrayF(vec![0.0; *n]),
        WidgetKind::SliderRange => WidgetValue::Range(config.min, config.max),
        _ => WidgetValue::Float((config.min + config.max) / 2.0),
    }
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
