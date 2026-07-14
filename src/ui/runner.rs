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

use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{ui_registry, UiHandle, WidgetKind, WidgetValue};

pub struct AudionUiApp {
    interpreter_done: Arc<AtomicBool>,
}

impl AudionUiApp {
    pub fn new(interpreter_done: Arc<AtomicBool>) -> Self {
        Self { interpreter_done }
    }
}

impl eframe::App for AudionUiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let registry: Vec<Arc<UiHandle>> = ui_registry().lock().unwrap().clone();

        let done = self.interpreter_done.load(Ordering::Relaxed);

        if registry.is_empty() && done {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Brief startup gap before interpreter registers its first handle.
        if registry.is_empty() {
            egui::CentralPanel::default().show(ctx, |_ui| {});
            ctx.request_repaint_after(Duration::from_millis(16));
            return;
        }

        // First handle owns the root OS window (set its title/size, render there).
        let first = &registry[0];
        {
            let cfg = first.config.lock().unwrap();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(cfg.title.clone()));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                egui::Vec2::new(cfg.width, cfg.height),
            ));
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            render_widgets(ui, first);
        });

        // Every additional handle gets its own OS window via an immediate viewport.
        for handle in registry.iter().skip(1) {
            render_as_viewport(ctx, handle);
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

fn render_as_viewport(ctx: &egui::Context, handle: &Arc<UiHandle>) {
    let (title, width, height) = {
        let cfg = handle.config.lock().unwrap();
        (cfg.title.clone(), cfg.width, cfg.height)
    };

    let viewport_id = egui::ViewportId::from_hash_of(handle.id);
    let builder = egui::ViewportBuilder::default()
        .with_title(&title)
        .with_inner_size([width, height]);

    let handle_clone = handle.clone();
    ctx.show_viewport_immediate(viewport_id, builder, move |ctx, _class| {
        egui::CentralPanel::default().show(ctx, |ui| {
            render_widgets(ui, &handle_clone);
        });
    });
}

// ---------------------------------------------------------------------------
// Widget rendering
// ---------------------------------------------------------------------------

fn render_widgets(ui: &mut egui::Ui, handle: &UiHandle) {
    let order: Vec<String> = handle.widget_order.lock().unwrap().clone();
    let widgets = handle.widgets.lock().unwrap();

    for id in &order {
        if let Some(state_arc) = widgets.get(id) {
            let mut state = state_arc.lock().unwrap();
            render_widget(ui, &mut state);
            ui.add_space(4.0);
        }
    }
}

fn render_widget(ui: &mut egui::Ui, state: &mut super::WidgetState) {
    let label = state.config.label.clone().unwrap_or_else(|| state.id.clone());

    match state.config.kind.clone() {
        WidgetKind::SliderH => {
            if let WidgetValue::Float(v) = &mut state.value {
                let min = state.config.min as f32;
                let max = state.config.max as f32;
                let mut fv = *v as f32;
                if ui.add(egui::Slider::new(&mut fv, min..=max).text(&label)).changed() {
                    *v = fv as f64;
                    state.dirty = true;
                }
            }
        }

        WidgetKind::SliderV => {
            if let WidgetValue::Float(v) = &mut state.value {
                let min = state.config.min as f32;
                let max = state.config.max as f32;
                let mut fv = *v as f32;
                if ui.add(egui::Slider::new(&mut fv, min..=max).vertical()).changed() {
                    *v = fv as f64;
                    state.dirty = true;
                }
                ui.label(&label);
            }
        }

        WidgetKind::SliderRange => {
            if let WidgetValue::Range(lo, hi) = &mut state.value {
                let min = state.config.min as f32;
                let max = state.config.max as f32;
                let mut flo = *lo as f32;
                let mut fhi = *hi as f32;
                ui.label(&label);
                ui.horizontal(|ui| {
                    let changed_lo = ui.add(egui::Slider::new(&mut flo, min..=fhi)).changed();
                    let changed_hi = ui.add(egui::Slider::new(&mut fhi, flo..=max)).changed();
                    if changed_lo || changed_hi {
                        *lo = flo as f64;
                        *hi = fhi as f64;
                        state.dirty = true;
                    }
                });
            }
        }

        WidgetKind::Button => {
            if ui.button(&label).clicked() {
                state.value = WidgetValue::Bool(true);
                state.dirty = true;
            }
        }

        WidgetKind::Toggle => {
            if let WidgetValue::Bool(b) = &mut state.value {
                if ui.toggle_value(b, &label).changed() {
                    state.dirty = true;
                }
            }
        }

        WidgetKind::Knob => {
            if let WidgetValue::Float(v) = &mut state.value {
                ui.horizontal(|ui| {
                    ui.label(&label);
                    if ui.add(
                        egui::DragValue::new(v)
                            .range(state.config.min..=state.config.max)
                            .speed(0.01),
                    ).changed() {
                        state.dirty = true;
                    }
                });
            }
        }

        WidgetKind::Number => {
            if let WidgetValue::Float(v) = &mut state.value {
                ui.horizontal(|ui| {
                    ui.label(&label);
                    if ui.add(
                        egui::DragValue::new(v)
                            .range(state.config.min..=state.config.max),
                    ).changed() {
                        state.dirty = true;
                    }
                });
            }
        }

        WidgetKind::Dropdown => {
            if let WidgetValue::Float(idx) = &mut state.value {
                let options = state.config.options.clone();
                let selected = (*idx as usize).min(options.len().saturating_sub(1));
                let current = options.get(selected).map(|s| s.as_str()).unwrap_or("—");
                egui::ComboBox::from_label(&label)
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (i, opt) in options.iter().enumerate() {
                            if ui.selectable_value(idx, i as f64, opt).changed() {
                                state.dirty = true;
                            }
                        }
                    });
            }
        }

        WidgetKind::TextLabel => {
            if let WidgetValue::Str(s) = &state.value {
                ui.label(s.as_str());
            } else {
                ui.label(&label);
            }
        }

        WidgetKind::TextInput => {
            if let WidgetValue::Str(s) = &mut state.value {
                ui.horizontal(|ui| {
                    ui.label(&label);
                    if ui.text_edit_singleline(s).changed() {
                        state.dirty = true;
                    }
                });
            }
        }

        WidgetKind::Array(n) => {
            if let WidgetValue::Array(bits) = &mut state.value {
                ui.label(&label);
                ui.horizontal(|ui| {
                    for i in 0..n {
                        if i < bits.len() {
                            if ui.toggle_value(&mut bits[i], i.to_string()).changed() {
                                state.dirty = true;
                            }
                        }
                    }
                });
            }
        }
    }
}
