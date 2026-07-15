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
    /// False when wgpu is unavailable (headless / CI); ThreeCanvas degrades to placeholder.
    three_supported: bool,
}

impl AudionUiApp {
    pub fn new(cc: &eframe::CreationContext, interpreter_done: Arc<AtomicBool>) -> Self {
        let three_supported = if let Some(rs) = &cc.wgpu_render_state {
            super::three_gpu::init(
                &rs.device,
                &rs.queue,
                rs.target_format,
                &mut rs.renderer.write().callback_resources,
            );
            true
        } else {
            false
        };
        Self { interpreter_done, three_supported }
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
        let ts = self.three_supported;
        egui::CentralPanel::default().show(ctx, |ui| {
            render_widgets(ui, first, ts);
        });

        // Every additional handle gets its own OS window via an immediate viewport.
        for handle in registry.iter().skip(1) {
            render_as_viewport(ctx, handle, ts);
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

fn render_as_viewport(ctx: &egui::Context, handle: &Arc<UiHandle>, three_supported: bool) {
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
            render_widgets(ui, &handle_clone, three_supported);
        });
    });
}

// ---------------------------------------------------------------------------
// Widget rendering
// ---------------------------------------------------------------------------

fn render_widgets(ui: &mut egui::Ui, handle: &UiHandle, three_supported: bool) {
    let order: Vec<String> = handle.widget_order.lock().unwrap().clone();
    let widgets = handle.widgets.lock().unwrap();

    for id in &order {
        if let Some(state_arc) = widgets.get(id) {
            let mut state = state_arc.lock().unwrap();
            render_widget(ui, &mut state, three_supported);
            ui.add_space(4.0);
        }
    }
}

fn render_widget(ui: &mut egui::Ui, state: &mut super::WidgetState, three_supported: bool) {
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
                let min = state.config.min;
                let max = state.config.max;
                ui.label(&label);
                if range_slider(ui, lo, hi, min, max, ui.id().with(&state.id)) {
                    state.dirty = true;
                }
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

        WidgetKind::Array(_) => {
            if let WidgetValue::Array(bits) = &mut state.value {
                ui.label(&label);
                ui.horizontal(|ui| {
                    // – removes last element (min 1)
                    if bits.len() > 1 && ui.small_button("–").clicked() {
                        bits.pop();
                        state.dirty = true;
                    }
                    for i in 0..bits.len() {
                        if ui.toggle_value(&mut bits[i], i.to_string()).changed() {
                            state.dirty = true;
                        }
                    }
                    // + adds a new false element
                    if ui.small_button("+").clicked() {
                        bits.push(false);
                        state.dirty = true;
                    }
                });
            }
        }

        WidgetKind::ThreeCanvas => {
            if let WidgetValue::Three(scene_arc) = &state.value {
                if three_supported {
                    let (w, h) = {
                        let s = scene_arc.lock().unwrap();
                        (s.width, s.height)
                    };
                    let (rect, _) = ui.allocate_exact_size(
                        egui::Vec2::new(w, h),
                        egui::Sense::hover(),
                    );
                    if ui.is_rect_visible(rect) {
                        ui.painter().add(eframe::egui_wgpu::Callback::new_paint_callback(
                            rect,
                            super::three_gpu::ThreeCallback {
                                canvas_id: state.id.clone(),
                                scene: scene_arc.clone(),
                                viewport_size: [w, h],
                                egui_rect: rect,
                            },
                        ));
                    }
                } else {
                    let s = scene_arc.lock().unwrap();
                    let (w, h) = (s.width, s.height);
                    drop(s);
                    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(w, h), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 30));
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "3D (no wgpu)", egui::FontId::proportional(14.0), egui::Color32::GRAY);
                }
            }
        }

        WidgetKind::ArrayNumbers(_) => {
            if let WidgetValue::ArrayF(nums) = &mut state.value {
                ui.label(&label);
                ui.horizontal(|ui| {
                    // – removes last element (min 1)
                    if nums.len() > 1 && ui.small_button("–").clicked() {
                        nums.pop();
                        state.dirty = true;
                    }
                    for v in nums.iter_mut() {
                        if ui.add(egui::DragValue::new(v).speed(0.1)).changed() {
                            state.dirty = true;
                        }
                    }
                    // + adds a new zero element
                    if ui.small_button("+").clicked() {
                        nums.push(0.0);
                        state.dirty = true;
                    }
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Custom two-handle range slider
// One track, two circular handles, minimum pixel gap so they never overlap.
// Returns true if either value changed this frame.
// ---------------------------------------------------------------------------

fn range_slider(
    ui: &mut egui::Ui,
    lo: &mut f64,
    hi: &mut f64,
    min: f64,
    max: f64,
    base_id: egui::Id,
) -> bool {
    let track_height = 4.0_f32;
    let handle_r     = 8.0_f32;
    let min_gap_px   = handle_r * 2.0 + 2.0; // handles can't be closer than this
    let widget_h     = handle_r * 2.0 + 6.0;

    let width = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::Vec2::new(width, widget_h),
        egui::Sense::hover(),
    );

    if !ui.is_rect_visible(rect) {
        return false;
    }

    // Track runs inside the handle radius margins
    let track_x0 = rect.left()  + handle_r;
    let track_x1 = rect.right() - handle_r;
    let track_w  = (track_x1 - track_x0).max(1.0);
    let cy       = rect.center().y;

    let val_to_x = |v: f64| -> f32 {
        let t = ((v - min) / (max - min)).clamp(0.0, 1.0) as f32;
        track_x0 + t * track_w
    };
    let x_to_val = |x: f32| -> f64 {
        let t = ((x - track_x0) / track_w).clamp(0.0, 1.0) as f64;
        min + t * (max - min)
    };

    let lo_x = val_to_x(*lo);
    let hi_x = val_to_x(*hi);

    // Hit rects — offset vertically so both are fully inside the allocated rect
    let lo_rect = egui::Rect::from_center_size(
        egui::pos2(lo_x, cy),
        egui::Vec2::splat(handle_r * 2.0),
    );
    let hi_rect = egui::Rect::from_center_size(
        egui::pos2(hi_x, cy),
        egui::Vec2::splat(handle_r * 2.0),
    );

    let lo_resp = ui.interact(lo_rect, base_id.with("lo"), egui::Sense::drag());
    let hi_resp = ui.interact(hi_rect, base_id.with("hi"), egui::Sense::drag());

    let mut changed = false;

    if lo_resp.dragged() {
        let new_x = (lo_x + lo_resp.drag_delta().x)
            .clamp(track_x0, hi_x - min_gap_px);
        *lo = x_to_val(new_x);
        changed = true;
    }
    if hi_resp.dragged() {
        let new_x = (hi_x + hi_resp.drag_delta().x)
            .clamp(lo_x + min_gap_px, track_x1);
        *hi = x_to_val(new_x);
        changed = true;
    }

    // Recalculate positions after any drag
    let lo_x = val_to_x(*lo);
    let hi_x = val_to_x(*hi);

    // --- draw ---
    let vis = ui.visuals();
    let painter = ui.painter();

    // Track background
    let track_rect = egui::Rect::from_min_max(
        egui::pos2(track_x0, cy - track_height / 2.0),
        egui::pos2(track_x1, cy + track_height / 2.0),
    );
    painter.rect_filled(track_rect, track_height / 2.0, vis.widgets.inactive.bg_fill);

    // Active fill between handles
    let active_rect = egui::Rect::from_min_max(
        egui::pos2(lo_x, cy - track_height / 2.0),
        egui::pos2(hi_x, cy + track_height / 2.0),
    );
    painter.rect_filled(active_rect, track_height / 2.0, vis.selection.bg_fill);

    // Lo handle
    let lo_fill = if lo_resp.dragged() || lo_resp.hovered() {
        vis.widgets.active.bg_fill
    } else {
        vis.widgets.inactive.bg_fill
    };
    painter.circle(
        egui::pos2(lo_x, cy),
        handle_r,
        lo_fill,
        vis.widgets.inactive.fg_stroke,
    );

    // Hi handle
    let hi_fill = if hi_resp.dragged() || hi_resp.hovered() {
        vis.widgets.active.bg_fill
    } else {
        vis.widgets.inactive.bg_fill
    };
    painter.circle(
        egui::pos2(hi_x, cy),
        handle_r,
        hi_fill,
        vis.widgets.inactive.fg_stroke,
    );

    // Value text below handles
    painter.text(
        egui::pos2(lo_x, cy + handle_r + 2.0),
        egui::Align2::CENTER_TOP,
        format!("{:.2}", lo),
        egui::FontId::proportional(10.0),
        vis.text_color(),
    );
    painter.text(
        egui::pos2(hi_x, cy + handle_r + 2.0),
        egui::Align2::CENTER_TOP,
        format!("{:.2}", hi),
        egui::FontId::proportional(10.0),
        vis.text_color(),
    );

    changed
}
