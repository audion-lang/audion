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
    three_supported: bool,
    edit_mode: bool,
    /// Widget IDs whose Area was hovered last frame — used to highlight frames.
    edit_hovered: std::collections::HashSet<String>,
    /// Cached background textures: handle_id → (image_path, mode, TextureHandle)
    bg_textures: std::collections::HashMap<u64, (String, super::BgImageMode, egui::TextureHandle)>,
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
        Self { interpreter_done, three_supported, edit_mode: false, edit_hovered: Default::default(), bg_textures: Default::default() }
    }
}

impl eframe::App for AudionUiApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        // When a custom background is configured, clear to transparent black so alpha
        // in bg_color blends with black rather than egui's opaque panel fill.
        let has_bg = ui_registry().lock().unwrap().first()
            .map(|h| {
                let cfg = h.config.lock().unwrap();
                cfg.bg_color.is_some() || cfg.bg_image.is_some()
            })
            .unwrap_or(false);
        if has_bg {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            let c = visuals.panel_fill;
            [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0]
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let registry: Vec<Arc<UiHandle>> = ui_registry().lock().unwrap().clone();

        let done = self.interpreter_done.load(Ordering::Relaxed);

        if registry.is_empty() && done {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if registry.is_empty() {
            egui::CentralPanel::default().show(ctx, |_ui| {});
            ctx.request_repaint_after(Duration::from_millis(16));
            return;
        }

        // Toggle edit mode with Ctrl+E
        let toggle = ctx.input(|i| i.key_pressed(egui::Key::E) && i.modifiers.ctrl);
        if toggle {
            if self.edit_mode { self.exit_edit_mode(&registry, ctx); }
            else              { self.enter_edit_mode(&registry); }
        }

        let first = &registry[0];
        {
            let mut cfg = first.config.lock().unwrap();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(cfg.title.clone()));
            // Only resize window when the script explicitly calls ui.window(), not every frame.
            // This lets the OS / user freely resize the window otherwise.
            if cfg.size_dirty {
                if let Some(ref aui_path) = cfg.aui_path.clone() {
                    // Override window size with saved value if present
                    if let Some((w, h)) = crate::ui::aui_file::load_window_size(aui_path) {
                        cfg.width  = w;
                        cfg.height = h;
                    }
                    // Load background settings (only if the script hasn't set them already)
                    if cfg.bg_color.is_none() && cfg.bg_image.is_none() {
                        if let Some(bg) = crate::ui::aui_file::load_window_background(aui_path) {
                            cfg.bg_color       = bg.color;
                            cfg.bg_image       = bg.image;
                            cfg.bg_image_mode  = bg.image_mode;
                            cfg.bg_image_alpha = bg.image_alpha;
                        }
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::new(cfg.width, cfg.height)));
                cfg.size_dirty = false;
            }
        }

        let ts = self.three_supported;
        let edit = self.edit_mode;
        let mut exit_edit = false;

        // Paint window background (color / image) on the background layer behind all panels
        paint_window_background(ctx, first, &mut self.bg_textures);

        let has_bg = {
            let cfg = first.config.lock().unwrap();
            cfg.bg_color.is_some() || cfg.bg_image.is_some()
        };

        let hovered = &mut self.edit_hovered;
        let panel = egui::CentralPanel::default();
        let panel = if has_bg { panel.frame(egui::Frame::none()) } else { panel };
        panel.show(ctx, |ui| {
            exit_edit = render_edit_toolbar(ui, edit);
            render_widgets(ctx, ui, first, ts, edit, hovered);
        });

        for handle in registry.iter().skip(1) {
            render_as_viewport(ctx, handle, ts, edit, hovered);
        }

        if exit_edit {
            self.exit_edit_mode(&registry, ctx);
        }

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl AudionUiApp {
    fn enter_edit_mode(&mut self, registry: &[Arc<UiHandle>]) {
        for handle in registry {
            auto_assign_positions(handle);
        }
        self.edit_mode = true;
    }

    fn exit_edit_mode(&mut self, registry: &[Arc<UiHandle>], ctx: &egui::Context) {
        let window_size = {
            let r = ctx.screen_rect();
            Some((r.width(), r.height()))
        };
        for handle in registry {
            save_layout_to_aui(handle, window_size);
        }
        self.edit_mode = false;
    }
}

/// Assign default grid positions to widgets that don't have explicit x/y yet.
fn auto_assign_positions(handle: &Arc<UiHandle>) {
    let order = handle.widget_order.lock().unwrap().clone();
    let widgets = handle.widgets.lock().unwrap();
    let mut x = 16.0_f32;
    let mut y = 16.0_f32;
    let col_width = 220.0_f32;
    let row_height = 60.0_f32;
    let max_cols = 3usize;
    let mut col = 0usize;

    for id in &order {
        if let Some(state_arc) = widgets.get(id) {
            let mut state = state_arc.lock().unwrap();
            if state.config.x.is_none() {
                state.config.x = Some(x);
                state.config.y = Some(y);
            }
            col += 1;
            if col >= max_cols {
                col = 0;
                x = 16.0;
                y += row_height;
            } else {
                x += col_width;
            }
        }
    }
}

/// Write widget positions + sizes and window dimensions to the companion .aui file.
fn save_layout_to_aui(handle: &Arc<UiHandle>, window_size: Option<(f32, f32)>) {
    use super::aui_file::{self, WidgetLayout};
    use std::collections::HashMap;

    let aui_path = {
        let cfg = handle.config.lock().unwrap();
        match cfg.aui_path.clone() {
            Some(p) => p,
            None => return,
        }
    };

    let order = handle.widget_order.lock().unwrap().clone();
    let widgets = handle.widgets.lock().unwrap();
    let mut layouts: HashMap<String, WidgetLayout> = HashMap::new();

    for id in &order {
        if let Some(state_arc) = widgets.get(id) {
            let state = state_arc.lock().unwrap();
            if let (Some(x), Some(y)) = (state.config.x, state.config.y) {
                layouts.insert(id.clone(), WidgetLayout {
                    x,
                    y,
                    width:  state.config.style.width,
                    height: state.config.style.height,
                });
            }
        }
    }

    aui_file::save_layout(&aui_path, &layouts, window_size);
}

/// Draw the edit-mode toolbar. Returns true if the user clicked "Done".
fn render_edit_toolbar(ui: &mut egui::Ui, edit_mode: bool) -> bool {
    if !edit_mode { return false; }

    let mut done = false;
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_premultiplied(40, 20, 0, 220))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("✏  Edit Layout").color(egui::Color32::from_rgb(255, 180, 60)).strong());
                ui.label(egui::RichText::new("drag widgets · resize with handles · Ctrl+E to finish").weak().small());
                if ui.button(egui::RichText::new("  ✓ Done  ").color(egui::Color32::from_rgb(100, 220, 100))).clicked() {
                    done = true;
                }
            });
        });
    ui.add_space(6.0);
    done
}

fn render_as_viewport(ctx: &egui::Context, handle: &Arc<UiHandle>, three_supported: bool, edit_mode: bool, hovered: &mut std::collections::HashSet<String>) {
    let (title, width, height) = {
        let cfg = handle.config.lock().unwrap();
        (cfg.title.clone(), cfg.width, cfg.height)
    };

    let viewport_id = egui::ViewportId::from_hash_of(handle.id);
    let builder = egui::ViewportBuilder::default()
        .with_title(&title)
        .with_inner_size([width, height]);

    let handle_clone = handle.clone();
    let mut vp_hovered = hovered.clone();
    // Secondary windows share the same egui Context but need their own texture cache key space.
    // We clone the bg_textures ref by moving a clone — secondary windows are rare.
    ctx.show_viewport_immediate(viewport_id, builder, move |ctx, _class| {
        let mut exit_edit = false;
        let has_bg = {
            let cfg = handle_clone.config.lock().unwrap();
            cfg.bg_color.is_some() || cfg.bg_image.is_some()
        };
        let mut vp_textures = std::collections::HashMap::new();
        paint_window_background(ctx, &handle_clone, &mut vp_textures);
        let panel = egui::CentralPanel::default();
        let panel = if has_bg { panel.frame(egui::Frame::none()) } else { panel };
        panel.show(ctx, |ui| {
            exit_edit = render_edit_toolbar(ui, edit_mode);
            render_widgets(ctx, ui, &handle_clone, three_supported, edit_mode, &mut vp_hovered);
        });
        if exit_edit {
            save_layout_to_aui(&handle_clone, None);
        }
    });
}

// ---------------------------------------------------------------------------
// Widget rendering
// ---------------------------------------------------------------------------

fn render_widgets(ctx: &egui::Context, ui: &mut egui::Ui, handle: &UiHandle, three_supported: bool, edit_mode: bool, edit_hovered: &mut std::collections::HashSet<String>) {
    let order: Vec<String> = handle.widget_order.lock().unwrap().clone();
    // Clone the Arc map so we don't hold the widgets mutex during rendering
    let widget_map: std::collections::HashMap<String, std::sync::Arc<std::sync::Mutex<super::WidgetState>>> = {
        handle.widgets.lock().unwrap().clone()
    };

    for id in &order {
        let Some(state_arc) = widget_map.get(id) else { continue };

        let (has_pos, pos_x, pos_y) = {
            let s = state_arc.lock().unwrap();
            (s.config.x.is_some() && s.config.y.is_some(), s.config.x.unwrap_or(0.0), s.config.y.unwrap_or(0.0))
        };

        if edit_mode || has_pos {
            let area_id = egui::Id::new(("audion_widget", id.as_str()));

            // KEY: in edit mode use default_pos (not fixed_pos) so egui owns the drag.
            // fixed_pos would override egui's memory every frame, killing X movement.
            let area = if edit_mode {
                egui::Area::new(area_id)
                    .default_pos(egui::pos2(pos_x, pos_y))
                    .movable(true)
                    .order(egui::Order::Middle)
            } else {
                egui::Area::new(area_id)
                    .fixed_pos(egui::pos2(pos_x, pos_y))
                    .movable(false)
                    .order(egui::Order::Middle)
            };

            // Highlight state from last frame (one-frame lag, imperceptible at 60fps)
            let is_hovered = edit_mode && edit_hovered.contains(id);
            let (stroke_color, stroke_w, handle_color, handle_bg) = if is_hovered {
                (
                    egui::Color32::from_rgb(255, 230, 120),  // bright gold border
                    2.0_f32,
                    egui::Color32::WHITE,
                    egui::Color32::from_rgba_premultiplied(255, 200, 60, 40),
                )
            } else {
                (
                    egui::Color32::from_rgb(255, 160, 40),   // normal orange border
                    1.5_f32,
                    egui::Color32::from_rgb(200, 130, 30),
                    egui::Color32::TRANSPARENT,
                )
            };

            let resp = area.show(ctx, |ui| {
                let cap_w = {
                    let s = state_arc.lock().unwrap();
                    s.config.style.width.unwrap_or(240.0)
                };
                ui.set_max_width(cap_w);

                if edit_mode {
                    egui::Frame::new()
                        .stroke(egui::Stroke::new(stroke_w, stroke_color))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            // Drag handle strip — detects hover for cursor change
                            let avail_w = ui.available_width().max(60.0);
                            let (handle_rect, handle_resp) = ui.allocate_exact_size(
                                egui::vec2(avail_w, 16.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(handle_rect, 2.0, handle_bg);
                            ui.painter().text(
                                handle_rect.left_center() + egui::vec2(4.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                "⠿  drag",
                                egui::FontId::proportional(11.0),
                                handle_color,
                            );
                            if handle_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            let mut state = state_arc.lock().unwrap();
                            render_widget(ui, &mut state, three_supported);
                            render_resize_handle(ui, &mut state);
                        });
                } else {
                    let mut state = state_arc.lock().unwrap();
                    render_widget(ui, &mut state, three_supported);
                }
            });

            if edit_mode {
                // Update hover set for next frame
                if resp.response.hovered() {
                    edit_hovered.insert(id.clone());
                } else {
                    edit_hovered.remove(id.as_str());
                }
                // Read back position
                let new_pos = resp.response.rect.min;
                let mut state = state_arc.lock().unwrap();
                state.config.x = Some(new_pos.x);
                state.config.y = Some(new_pos.y);
            }
        } else {
            // Flow layout (no position set)
            let mut state = state_arc.lock().unwrap();
            render_widget(ui, &mut state, three_supported);
            ui.add_space(4.0);
        }
    }
}

fn render_widget(ui: &mut egui::Ui, state: &mut super::WidgetState, three_supported: bool) {
    // Apply per-widget style overrides inside a scoped child Ui so visuals don't leak.
    let style = state.config.style.clone();
    ui.scope(|ui| {
        if let Some([r, g, b]) = style.color {
            let c = egui::Color32::from_rgb(r, g, b);
            ui.visuals_mut().selection.bg_fill              = c;
            ui.visuals_mut().widgets.active.bg_fill         = c;
            ui.visuals_mut().widgets.hovered.bg_fill        = c.gamma_multiply(0.8);
            ui.visuals_mut().widgets.inactive.bg_fill       = c.gamma_multiply(0.5);
            ui.visuals_mut().hyperlink_color                = c;
        }
        if let Some([r, g, b]) = style.bg_color {
            let c = egui::Color32::from_rgb(r, g, b);
            ui.visuals_mut().extreme_bg_color = c;
            ui.visuals_mut().faint_bg_color   = c.gamma_multiply(1.1);
        }
        render_widget_inner(ui, state, three_supported, &style);
    });
}

fn render_widget_inner(
    ui: &mut egui::Ui,
    state: &mut super::WidgetState,
    three_supported: bool,
    style: &super::WidgetStyle,
) {
    let label = state.config.label.clone().unwrap_or_else(|| state.id.clone());
    let highlighted = state.highlighted.clone();

    match state.config.kind.clone() {
        WidgetKind::SliderH => {
            if let WidgetValue::Float(v) = &mut state.value {
                let min = state.config.min as f32;
                let max = state.config.max as f32;
                let mut fv = *v as f32;
                let avail_w = style.width.unwrap_or(ui.available_width());
                let (rect, _) = ui.allocate_exact_size(
                    egui::Vec2::new(avail_w, ui.spacing().interact_size.y),
                    egui::Sense::hover(),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
                if child.add(egui::Slider::new(&mut fv, min..=max).text(&label)).changed() {
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
                let w = style.width.unwrap_or(20.0);
                let h = style.height.unwrap_or(120.0);
                if ui.add_sized([w, h], egui::Slider::new(&mut fv, min..=max).vertical()).changed() {
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
                if range_slider(ui, lo, hi, min, max, ui.id().with(&state.id), style) {
                    state.dirty = true;
                }
            }
        }

        WidgetKind::Button => {
            let resp = if let Some(w) = style.width {
                ui.add_sized([w, ui.spacing().interact_size.y], egui::Button::new(&label))
            } else {
                ui.button(&label)
            };
            if resp.clicked() {
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
                let size = style.width.unwrap_or(56.0).min(style.height.unwrap_or(56.0));
                if knob(ui, v, state.config.min, state.config.max, size, ui.id().with(&state.id), style) {
                    state.dirty = true;
                }
                ui.label(&label);
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
                    if bits.len() > 1 && ui.small_button("–").clicked() {
                        bits.pop();
                        state.dirty = true;
                    }
                    let hl_color = style.highlight_color
                        .map(|[r,g,b]| egui::Color32::from_rgb(r,g,b))
                        .unwrap_or(egui::Color32::from_rgb(255, 230, 60));

                    for i in 0..bits.len() {
                        let is_lit = highlighted.contains(&i);
                        if is_lit {
                            ui.scope(|ui| {
                                ui.visuals_mut().selection.bg_fill        = hl_color;
                                ui.visuals_mut().widgets.inactive.bg_fill = hl_color.gamma_multiply(0.5);
                                ui.visuals_mut().widgets.active.bg_fill   = hl_color;
                                ui.visuals_mut().widgets.hovered.bg_fill  = hl_color.gamma_multiply(0.9);
                                if ui.toggle_value(&mut bits[i], i.to_string()).changed() {
                                    state.dirty = true;
                                }
                            });
                        } else if ui.toggle_value(&mut bits[i], i.to_string()).changed() {
                            state.dirty = true;
                        }
                    }
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
                    if nums.len() > 1 && ui.small_button("–").clicked() {
                        nums.pop();
                        state.dirty = true;
                    }
                    for v in nums.iter_mut() {
                        if ui.add(egui::DragValue::new(v).speed(0.1)).changed() {
                            state.dirty = true;
                        }
                    }
                    if ui.small_button("+").clicked() {
                        nums.push(0.0);
                        state.dirty = true;
                    }
                });
            }
        }

        WidgetKind::FilePicker { filters } => {
            let path = if let WidgetValue::Str(s) = &state.value { s.clone() } else { String::new() };
            ui.horizontal(|ui| {
                let btn_label = if path.is_empty() { format!("📂  {}", label) } else { "📂  …".to_string() };
                if ui.button(&btn_label).clicked() {
                    let mut dialog = rfd::FileDialog::new();
                    for ext in &filters {
                        dialog = dialog.add_filter(ext, &[ext.as_str()]);
                    }
                    if let Some(p) = dialog.pick_file() {
                        state.value = WidgetValue::Str(p.to_string_lossy().into_owned());
                        state.dirty = true;
                    }
                }
                if !path.is_empty() {
                    let display = std::path::Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or(path.clone());
                    ui.label(&display).on_hover_text(&path);
                }
            });
        }

        WidgetKind::FolderPicker => {
            let path = if let WidgetValue::Str(s) = &state.value { s.clone() } else { String::new() };
            ui.horizontal(|ui| {
                let btn_label = if path.is_empty() { format!("📁  {}", label) } else { "📁  …".to_string() };
                if ui.button(&btn_label).clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        state.value = WidgetValue::Str(p.to_string_lossy().into_owned());
                        state.dirty = true;
                    }
                }
                if !path.is_empty() {
                    let display = std::path::Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or(path.clone());
                    ui.label(&display).on_hover_text(&path);
                }
            });
        }

        WidgetKind::Piano => {
            if let WidgetValue::Piano(piano_arc) = &mut state.value {
                let id = ui.id().with(&state.id);
                let mut piano = piano_arc.lock().unwrap();
                if render_piano(ui, &mut piano, id, style) {
                    state.dirty = true;
                }
            }
        }

        WidgetKind::Canvas2d => {
            if let WidgetValue::Canvas2d(data_arc) = &state.value {
                render_canvas2d(ui, data_arc, style.width, style.height);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Window background — color + image (fill/fit/center/stretch/tile)
// ---------------------------------------------------------------------------

fn paint_window_background(
    ctx: &egui::Context,
    handle: &std::sync::Arc<super::UiHandle>,
    textures: &mut std::collections::HashMap<u64, (String, super::BgImageMode, egui::TextureHandle)>,
) {
    let (bg_color, bg_image, bg_mode, bg_alpha) = {
        let cfg = handle.config.lock().unwrap();
        (cfg.bg_color, cfg.bg_image.clone(), cfg.bg_image_mode.clone(), cfg.bg_image_alpha)
    };

    if bg_color.is_none() && bg_image.is_none() { return; }

    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());

    if let Some([r, g, b, a]) = bg_color {
        painter.rect_filled(screen, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, a));
    }

    if let Some(ref path) = bg_image {
        let needs_load = textures.get(&handle.id)
            .map(|(p, m, _)| p != path || m != &bg_mode)
            .unwrap_or(true);

        if needs_load {
            if let Some(tex) = load_bg_texture(ctx, path, &bg_mode) {
                textures.insert(handle.id, (path.clone(), bg_mode.clone(), tex));
            }
        }

        if let Some((_, _, tex)) = textures.get(&handle.id) {
            let tint = egui::Color32::from_rgba_unmultiplied(255, 255, 255, bg_alpha);
            paint_bg_image(&painter, screen, tex, &bg_mode, tint);
        }
    }
}

fn load_bg_texture(ctx: &egui::Context, path: &str, mode: &super::BgImageMode) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let pixels = img.into_raw();

    let wrap = if *mode == super::BgImageMode::Tile {
        egui::TextureWrapMode::Repeat
    } else {
        egui::TextureWrapMode::ClampToEdge
    };

    Some(ctx.load_texture(
        format!("audion_bg:{}", path),
        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels),
        egui::TextureOptions {
            magnification: egui::TextureFilter::Linear,
            minification:  egui::TextureFilter::Linear,
            wrap_mode: wrap,
            ..Default::default()
        },
    ))
}

fn paint_bg_image(
    painter: &egui::Painter,
    screen: egui::Rect,
    tex: &egui::TextureHandle,
    mode: &super::BgImageMode,
    tint: egui::Color32,
) {
    let ts = tex.size_vec2();
    let sw = screen.width();
    let sh = screen.height();
    let img_aspect    = ts.x / ts.y;
    let screen_aspect = sw / sh;

    let (draw_rect, uv) = match mode {
        super::BgImageMode::Fill => {
            // Cover: fill screen, maintain aspect, crop excess
            let uv = if img_aspect > screen_aspect {
                // Image wider → fit height, crop left/right
                let uv_w = screen_aspect / img_aspect;
                let pad  = (1.0 - uv_w) / 2.0;
                egui::Rect::from_min_max(egui::pos2(pad, 0.0), egui::pos2(1.0 - pad, 1.0))
            } else {
                // Image taller → fit width, crop top/bottom
                let uv_h = img_aspect / screen_aspect;
                let pad  = (1.0 - uv_h) / 2.0;
                egui::Rect::from_min_max(egui::pos2(0.0, pad), egui::pos2(1.0, 1.0 - pad))
            };
            (screen, uv)
        }
        super::BgImageMode::Fit => {
            // Letterbox: show whole image, add bars
            let (dw, dh) = if img_aspect > screen_aspect {
                (sw, sw / img_aspect)
            } else {
                (sh * img_aspect, sh)
            };
            let rect = egui::Rect::from_center_size(screen.center(), egui::vec2(dw, dh));
            (rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)))
        }
        super::BgImageMode::Center => {
            // Native size centered, no scaling
            let rect = egui::Rect::from_center_size(screen.center(), ts);
            (rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)))
        }
        super::BgImageMode::Stretch => {
            // Stretch to fill exactly
            (screen, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)))
        }
        super::BgImageMode::Tile => {
            // Tile: UVs > 1.0 with TextureWrapMode::Repeat
            let uv_max = egui::pos2(sw / ts.x, sh / ts.y);
            (screen, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), uv_max))
        }
    };

    painter.image(tex.id(), draw_rect, uv, tint);
}

// ---------------------------------------------------------------------------
// Edit-mode resize handle — drag corner to set style.width / style.height
// ---------------------------------------------------------------------------

fn render_resize_handle(ui: &mut egui::Ui, state: &mut super::WidgetState) {
    let handle_size = 10.0_f32;
    let (rect, resp) = ui.allocate_exact_size(
        egui::Vec2::new(ui.available_width().max(handle_size), handle_size),
        egui::Sense::drag(),
    );
    let corner = egui::pos2(rect.right(), rect.bottom());
    let handle_rect = egui::Rect::from_center_size(corner, egui::Vec2::splat(handle_size));
    let handle_resp = ui.interact(handle_rect, resp.id.with("resize"), egui::Sense::drag());

    let painter = ui.painter();
    let color = if handle_resp.hovered() || handle_resp.dragged() {
        egui::Color32::from_rgb(255, 160, 40)
    } else {
        egui::Color32::from_rgba_premultiplied(255, 160, 40, 100)
    };
    // Draw a small resize grip (3 diagonal lines)
    for i in 0..3 {
        let offset = (i as f32 + 1.0) * 3.0;
        painter.line_segment(
            [egui::pos2(corner.x - offset, corner.y), egui::pos2(corner.x, corner.y - offset)],
            egui::Stroke::new(1.0, color),
        );
    }

    if handle_resp.dragged() {
        let delta = handle_resp.drag_delta();
        let current_w = state.config.style.width.unwrap_or(rect.width());
        let current_h = state.config.style.height.unwrap_or(20.0);
        state.config.style.width  = Some((current_w + delta.x).max(40.0));
        state.config.style.height = Some((current_h + delta.y).max(20.0));
    }

    let _ = rect;
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
    style: &super::WidgetStyle,
) -> bool {
    let track_height = 4.0_f32;
    let handle_r     = 8.0_f32;
    let min_gap_px   = handle_r * 2.0 + 2.0; // handles can't be closer than this
    let widget_h     = handle_r * 2.0 + 6.0;

    let width = style.width.unwrap_or_else(|| ui.available_width()).max(120.0);
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

// ---------------------------------------------------------------------------
// Circular knob — drag-to-rotate, arc from -225° to +45° (270° sweep)
// Returns true if the value changed this frame.
// ---------------------------------------------------------------------------

fn knob(
    ui: &mut egui::Ui,
    value: &mut f64,
    min: f64,
    max: f64,
    size: f32,
    id: egui::Id,
    style: &super::WidgetStyle,
) -> bool {
    let r_outer = size / 2.0;
    let r_inner = r_outer * 0.55;
    let r_dot   = r_outer * 0.12;

    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::drag());
    if !ui.is_rect_visible(rect) { return false; }

    let mut changed = false;
    if resp.dragged() {
        let delta = resp.drag_delta();
        let range = (max - min).abs().max(1e-10);
        // 270° sweep → drag full height equals full range
        let sensitivity = range / (size as f64 * 3.0);
        *value = (*value - delta.y as f64 * sensitivity).clamp(min, max);
        changed = true;
    }

    let center = rect.center();
    let painter = ui.painter_at(rect);
    let vis = ui.visuals();

    // Map value to angle: -225° at min, +45° at max (i.e., -5π/4 .. π/4)
    let t = if (max - min).abs() < 1e-10 { 0.0 } else { ((*value - min) / (max - min)).clamp(0.0, 1.0) as f32 };
    let start_angle = std::f32::consts::PI * 1.25; // 225°
    let sweep       = std::f32::consts::PI * 1.5;  // 270°
    let val_angle   = start_angle + t * sweep;

    // Accent color (style override or selection fill)
    let accent = if let Some([r, g, b]) = style.color {
        egui::Color32::from_rgb(r, g, b)
    } else {
        vis.selection.bg_fill
    };

    // Track ring (inactive)
    let track_stroke = egui::Stroke::new(3.0, vis.widgets.inactive.bg_fill);
    draw_arc(&painter, center, r_outer * 0.78, start_angle, start_angle + sweep, track_stroke);

    // Value arc (accent)
    let val_stroke = egui::Stroke::new(3.0, accent);
    draw_arc(&painter, center, r_outer * 0.78, start_angle, val_angle, val_stroke);

    // Knob body
    let body_fill = if resp.dragged() || resp.hovered() {
        vis.widgets.active.bg_fill
    } else {
        vis.widgets.inactive.bg_fill
    };
    painter.circle(center, r_inner, body_fill, egui::Stroke::new(1.5, vis.widgets.inactive.fg_stroke.color));

    // Indicator dot
    let (sin, cos) = val_angle.sin_cos();
    let dot_pos = egui::pos2(center.x + cos * r_inner * 0.65, center.y + sin * r_inner * 0.65);
    painter.circle_filled(dot_pos, r_dot, accent);

    // Tooltip with current value
    if resp.hovered() || resp.dragged() {
        resp.clone().on_hover_text(format!("{:.3}", value));
    }

    let _ = id; // reserved for future state persistence
    changed
}

/// Approximate arc with line segments (egui has no native arc primitive).
fn draw_arc(painter: &egui::Painter, center: egui::Pos2, r: f32, a0: f32, a1: f32, stroke: egui::Stroke) {
    let steps = ((a1 - a0).abs() * r / 2.0).ceil() as usize;
    let steps = steps.max(4).min(64);
    let pts: Vec<egui::Pos2> = (0..=steps)
        .map(|i| {
            let angle = a0 + (a1 - a0) * (i as f32 / steps as f32);
            let (s, c) = angle.sin_cos();
            egui::pos2(center.x + c * r, center.y + s * r)
        })
        .collect();
    for w in pts.windows(2) {
        painter.line_segment([w[0], w[1]], stroke);
    }
}

// ---------------------------------------------------------------------------
// Piano keyboard widget
// ---------------------------------------------------------------------------

/// Semitone → is black key?
const IS_BLACK: [bool; 12] = [false, true, false, true, false, false, true, false, true, false, true, false];

/// X centre of black keys within one octave, in units of white-key-widths from octave left edge.
const BLACK_X: [f32; 12] = [
    0.0, 0.72, 0.0, 1.72, 0.0, 0.0, 3.70, 0.0, 4.72, 0.0, 5.72, 0.0,
];

/// qwerty → semitone offset from start_note
const KB_MAP: &[(egui::Key, u8)] = &[
    (egui::Key::A, 0),  (egui::Key::W, 1),  (egui::Key::S, 2),  (egui::Key::E, 3),
    (egui::Key::D, 4),  (egui::Key::F, 5),  (egui::Key::T, 6),  (egui::Key::G, 7),
    (egui::Key::Y, 8),  (egui::Key::H, 9),  (egui::Key::U, 10), (egui::Key::J, 11),
    (egui::Key::K, 12), (egui::Key::O, 13), (egui::Key::L, 14), (egui::Key::P, 15),
    (egui::Key::Semicolon, 16),
];

fn render_piano(
    ui: &mut egui::Ui,
    piano: &mut super::PianoData,
    base_id: egui::Id,
    style: &super::WidgetStyle,
) -> bool {
    let octaves    = piano.octaves.max(1) as usize;
    let start_note = piano.start_note;
    let num_white  = octaves * 7;

    let wkw = style.width .map(|w| w / num_white as f32).unwrap_or(24.0);
    let wkh = style.height.unwrap_or(80.0);
    let bkw = wkw * 0.62;
    let bkh = wkh * 0.62;

    let total_w = wkw * num_white as f32;
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(total_w, wkh), egui::Sense::hover());
    if !ui.is_rect_visible(rect) { return false; }

    let painter = ui.painter_at(rect);
    let ox = rect.min.x;
    let oy = rect.min.y;

    // Build key rects
    let mut white_keys: Vec<(u8, egui::Rect)> = Vec::new();
    let mut black_keys: Vec<(u8, egui::Rect)> = Vec::new();
    let mut white_idx = 0usize;

    for oct in 0..octaves {
        for semi in 0..12usize {
            let note = (start_note as usize + oct * 12 + semi) as u8;
            if note > 127 { break; }
            if IS_BLACK[semi] {
                let cx = ox + (oct * 7) as f32 * wkw + BLACK_X[semi] * wkw;
                black_keys.push((note, egui::Rect::from_min_size(
                    egui::pos2(cx - bkw / 2.0, oy),
                    egui::vec2(bkw, bkh),
                )));
            } else {
                let x = ox + white_idx as f32 * wkw;
                white_keys.push((note, egui::Rect::from_min_size(
                    egui::pos2(x, oy),
                    egui::vec2(wkw - 1.0, wkh),
                )));
                white_idx += 1;
            }
        }
    }

    // --- Mouse interaction ---
    let ptr     = ui.input(|i| i.pointer.interact_pos());
    let pressed = ui.input(|i| i.pointer.primary_pressed());
    let released= ui.input(|i| i.pointer.primary_released());
    let mut changed = false;

    if pressed {
        if let Some(pos) = ptr {
            if rect.contains(pos) {
                let hit = black_keys.iter().find(|(_, r)| r.contains(pos))
                    .or_else(|| white_keys.iter().find(|(_, r)| r.contains(pos)));
                if let Some((note, _)) = hit {
                    if piano.hold_mode {
                        if piano.active_notes.contains(note) { piano.active_notes.remove(note); }
                        else                                 { piano.active_notes.insert(*note); }
                    } else {
                        piano.active_notes.clear();
                        piano.active_notes.insert(*note);
                    }
                    changed = true;
                }
            }
        }
    }
    if released && !piano.hold_mode && !piano.active_notes.is_empty() {
        // Only release if the pointer was inside the piano (dragged off = keep)
        if ptr.map(|p| !rect.contains(p)).unwrap_or(true) {
            piano.active_notes.clear();
            changed = true;
        }
    }

    // --- Keyboard input ---
    if piano.keyboard_mode {
        for (key, offset) in KB_MAP {
            let note = start_note.saturating_add(*offset);
            if note > 127 { continue; }
            let kp = ui.ctx().input(|i| i.key_pressed(*key));
            let kr = ui.ctx().input(|i| i.key_released(*key));
            if kp {
                if piano.hold_mode {
                    if piano.active_notes.contains(&note) { piano.active_notes.remove(&note); }
                    else                                   { piano.active_notes.insert(note); }
                } else {
                    piano.active_notes.insert(note);
                }
                changed = true;
            }
            if kr && !piano.hold_mode {
                if piano.active_notes.remove(&note) { changed = true; }
            }
        }
    }

    // --- Resolve style colors ---
    let pressed_color = style.color
        .map(|[r,g,b]| egui::Color32::from_rgb(r,g,b))
        .unwrap_or(egui::Color32::from_rgb(80, 160, 255));
    let white_key_color = style.bg_color
        .map(|[r,g,b]| egui::Color32::from_rgb(r,g,b))
        .unwrap_or(egui::Color32::from_rgb(238, 238, 238));
    // Black key default: a darkened version of white_key_color, or near-black
    let black_key_color = style.bg_color
        .map(|[r,g,b]| egui::Color32::from_rgb(r/5, g/5, b/5))
        .unwrap_or(egui::Color32::from_gray(18));
    let pressed_black = pressed_color.gamma_multiply(0.75);
    let border_color  = white_key_color.gamma_multiply(0.35);

    // --- Draw white keys ---
    for (note, kr) in &white_keys {
        let active = piano.active_notes.contains(note);
        let fill = if active { pressed_color } else { white_key_color };
        painter.rect_filled(*kr, 2.0, fill);
        painter.rect_stroke(*kr, 2.0, egui::Stroke::new(1.0, border_color), egui::StrokeKind::Middle);
        // Dot on every C for orientation
        let semitone = (note.wrapping_sub(start_note)) % 12;
        if semitone == 0 {
            painter.circle_filled(
                egui::pos2(kr.center().x, kr.max.y - 6.0),
                2.5,
                if active { egui::Color32::WHITE } else { border_color },
            );
        }
    }

    // --- Draw black keys (on top) ---
    for (note, kr) in &black_keys {
        let active = piano.active_notes.contains(note);
        let fill = if active { pressed_black } else { black_key_color };
        painter.rect_filled(*kr, 2.0, fill);
        if active {
            painter.rect_stroke(*kr, 2.0, egui::Stroke::new(1.5, pressed_color), egui::StrokeKind::Middle);
        }
    }

    let _ = base_id;
    changed
}

// ---------------------------------------------------------------------------
// 2D canvas — replay DrawCmd list via egui Painter
// ---------------------------------------------------------------------------

fn render_canvas2d(ui: &mut egui::Ui, data_arc: &std::sync::Arc<std::sync::Mutex<super::Canvas2dData>>, override_w: Option<f32>, override_h: Option<f32>) {
    use super::DrawCmd;

    let (cmds, w, h) = {
        let d = data_arc.lock().unwrap();
        (d.cmds.clone(), override_w.unwrap_or(d.width), override_h.unwrap_or(d.height))
    };

    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(w, h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) { return; }

    let painter = ui.painter_at(rect);
    let origin  = rect.min;

    // Default background
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(15, 15, 15));

    for cmd in &cmds {
        match cmd {
            DrawCmd::Clear => {
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(15, 15, 15));
            }
            DrawCmd::Fill([r, g, b]) => {
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(*r, *g, *b));
            }
            DrawCmd::Rect { x, y, w: rw, h: rh, color: [r, g, b], filled } => {
                let min = egui::pos2(origin.x + x, origin.y + y);
                let max = egui::pos2(origin.x + x + rw, origin.y + y + rh);
                let draw_rect = egui::Rect::from_min_max(min, max);
                let color = egui::Color32::from_rgb(*r, *g, *b);
                if *filled {
                    painter.rect_filled(draw_rect, 0.0, color);
                } else {
                    painter.rect_stroke(draw_rect, 0.0, egui::Stroke::new(1.0, color), egui::StrokeKind::Middle);
                }
            }
            DrawCmd::Circle { cx, cy, r, color: [cr, cg, cb], filled } => {
                let c = egui::pos2(origin.x + cx, origin.y + cy);
                let color = egui::Color32::from_rgb(*cr, *cg, *cb);
                if *filled {
                    painter.circle_filled(c, *r, color);
                } else {
                    painter.circle_stroke(c, *r, egui::Stroke::new(1.0, color));
                }
            }
            DrawCmd::Line { x1, y1, x2, y2, color: [r, g, b], width: lw } => {
                painter.line_segment(
                    [egui::pos2(origin.x + x1, origin.y + y1), egui::pos2(origin.x + x2, origin.y + y2)],
                    egui::Stroke::new(*lw, egui::Color32::from_rgb(*r, *g, *b)),
                );
            }
            DrawCmd::Text { x, y, s, size, color: [r, g, b] } => {
                painter.text(
                    egui::pos2(origin.x + x, origin.y + y),
                    egui::Align2::LEFT_TOP,
                    s.as_str(),
                    egui::FontId::proportional(*size),
                    egui::Color32::from_rgb(*r, *g, *b),
                );
            }
        }
    }
}
