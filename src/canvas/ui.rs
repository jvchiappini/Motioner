use crate::app_state::AppState;
use eframe::egui;
use crate::animations::animations_manager::animated_xy_for;
use super::position_cache::{build_position_cache, cached_frame_for};
use super::rasterizer::sample_color_at;
#[cfg(feature = "wgpu")]
use super::gpu::{GpuShape, CompositionCallback};

/// Renderiza y maneja las interacciones para el área del canvas central.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, main_ui_enabled: bool) {
    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::drag().union(egui::Sense::click()),
        );

        if main_ui_enabled {
            if response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle)
            {
                state.canvas_pan_x += response.drag_delta().x;
                state.canvas_pan_y += response.drag_delta().y;
            }

            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let zoom_delta = (scroll * 0.002).exp();

                    if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                        let zoom_before = state.canvas_zoom;
                        state.canvas_zoom *= zoom_delta;
                        state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
                        let actual_delta = state.canvas_zoom / zoom_before;

                        let center = rect.center();
                        state.canvas_pan_x = (state.canvas_pan_x - (mouse_pos.x - center.x))
                            * actual_delta
                            + (mouse_pos.x - center.x);
                        state.canvas_pan_y = (state.canvas_pan_y - (mouse_pos.y - center.y))
                            * actual_delta
                            + (mouse_pos.y - center.y);
                    } else {
                        state.canvas_zoom *= zoom_delta;
                        state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
                    }
                }
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(60, 60, 60));

        let zoom = state.canvas_zoom;
        let pan = egui::vec2(state.canvas_pan_x, state.canvas_pan_y);
        let center = rect.center();

        let mut base_step = 100.0;
        while base_step * zoom > 200.0 { base_step /= 10.0; }
        while base_step * zoom < 20.0 { base_step *= 10.0; }

        let step = base_step * zoom;
        let grid_origin = center + pan;

        let start_x = rect.left() + (grid_origin.x - rect.left()) % step - step;
        let start_y = rect.top() + (grid_origin.y - rect.top()) % step - step;

        let grid_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 40));
        let origin_stroke_x = egui::Stroke::new(2.0, egui::Color32::from_rgb(150, 50, 50));
        let origin_stroke_y = egui::Stroke::new(2.0, egui::Color32::from_rgb(50, 150, 50));

        let mut x = start_x;
        while x <= rect.right() + step {
            if x >= rect.left() {
                painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], grid_stroke);
            }
            x += step;
        }

        let mut y = start_y;
        while y <= rect.bottom() + step {
            if y >= rect.top() {
                painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], grid_stroke);
            }
            y += step;
        }

        if grid_origin.x >= rect.left() && grid_origin.x <= rect.right() {
            painter.line_segment([egui::pos2(grid_origin.x, rect.top()), egui::pos2(grid_origin.x, rect.bottom())], origin_stroke_y);
        }
        if grid_origin.y >= rect.top() && grid_origin.y <= rect.bottom() {
            painter.line_segment([egui::pos2(rect.left(), grid_origin.y), egui::pos2(rect.right(), grid_origin.y)], origin_stroke_x);
        }

        let composition_size = egui::vec2(state.render_width as f32, state.render_height as f32) * zoom;
        let composition_min = grid_origin - composition_size / 2.0;
        let composition_rect = egui::Rect::from_min_size(composition_min, composition_size);

        state.last_composition_rect = Some(composition_rect);

        let shadow_rect = composition_rect.expand(4.0 * zoom);
        painter.rect_filled(shadow_rect, 0.0, egui::Color32::from_black_alpha(100));

        painter.rect_filled(composition_rect, 0.0, egui::Color32::WHITE);
        painter.rect_stroke(composition_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::BLACK));

        if state.position_cache.is_none() {
            if let Some(pc) = build_position_cache(state) {
                state.position_cache = Some(pc);
            }
        }

        #[cfg(feature = "wgpu")]
        {
            let mut gpu_shapes = Vec::new();
            let cached = cached_frame_for(state, state.time);
            let mut flat_idx: usize = 0;

            fn fill_gpu_shapes(
                shapes: &[crate::scene::Shape],
                gpu_shapes: &mut Vec<GpuShape>,
                _render_width: f32,
                _render_height: f32,
                parent_spawn: f32,
                current_time: f32,
                project_duration: f32,
                cached: Option<&Vec<(f32, f32)>>,
                flat_idx: &mut usize,
                handlers: &[crate::dsl::runtime::DslHandler],
            ) {
                let frame_idx = (current_time * 60.0).round() as u32; // Default to 60fps for fallback calc
                for shape in shapes {
                    let my_spawn = shape.spawn_time().max(parent_spawn);
                    match shape {
                        crate::scene::Shape::Circle { radius, color, .. } => {
                            let (eval_x, eval_y) = if let Some(frame) = cached {
                                let p = frame.get(*flat_idx).copied().unwrap_or((0.0, 0.0));
                                *flat_idx += 1;
                                p
                            } else {
                                *flat_idx += 1;
                                // Build a transient copy to apply DSL for live feedback without cache
                                let mut transient = shape.clone();
                                crate::events::time_changed_event::apply_on_time_handlers(
                                    std::slice::from_mut(&mut transient),
                                    handlers,
                                    current_time,
                                    frame_idx,
                                );
                                animated_xy_for(&transient, current_time, project_duration)
                            };
                            gpu_shapes.push(GpuShape {
                                pos: [eval_x, eval_y],
                                size: [*radius, 0.0],
                                color: [
                                    color[0] as f32 / 255.0,
                                    color[1] as f32 / 255.0,
                                    color[2] as f32 / 255.0,
                                    color[3] as f32 / 255.0,
                                ],
                                shape_type: 0,
                                spawn_time: my_spawn,
                                p1: 0,
                                p2: 0,
                            });
                        }
                        crate::scene::Shape::Rect { w, h, color, .. } => {
                            let (eval_x, eval_y) = if let Some(frame) = cached {
                                let p = frame.get(*flat_idx).copied().unwrap_or((0.0, 0.0));
                                *flat_idx += 1;
                                p
                            } else {
                                *flat_idx += 1;
                                // Build a transient copy for live feedback
                                let mut transient = shape.clone();
                                crate::events::time_changed_event::apply_on_time_handlers(
                                    std::slice::from_mut(&mut transient),
                                    handlers,
                                    current_time,
                                    frame_idx,
                                );
                                animated_xy_for(&transient, current_time, project_duration)
                            };
                            gpu_shapes.push(GpuShape {
                                pos: [eval_x + *w / 2.0, eval_y + *h / 2.0],
                                size: [*w / 2.0, *h / 2.0],
                                color: [
                                    color[0] as f32 / 255.0,
                                    color[1] as f32 / 255.0,
                                    color[2] as f32 / 255.0,
                                    color[3] as f32 / 255.0,
                                ],
                                shape_type: 1,
                                spawn_time: my_spawn,
                                p1: 0,
                                p2: 0,
                            });
                        }
                        crate::scene::Shape::Group { children, .. } => {
                            fill_gpu_shapes(children, gpu_shapes, _render_width, _render_height, my_spawn, current_time, project_duration, cached, flat_idx, handlers);
                        }
                    }
                }
            }

            fill_gpu_shapes(&state.scene, &mut gpu_shapes, state.render_width as f32, state.render_height as f32, 0.0, state.time, state.duration_secs, cached, &mut flat_idx, &state.dsl_event_handlers);

            let magnifier_pos = if state.picker_active { ui.input(|i| i.pointer.hover_pos()) } else { None };

            let cb = eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                CompositionCallback {
                    shapes: gpu_shapes,
                    render_width: state.render_width as f32,
                    render_height: state.render_height as f32,
                    preview_multiplier: state.preview_multiplier,
                    paper_rect: composition_rect,
                    viewport_rect: rect,
                    magnifier_pos,
                    time: state.time,
                },
            );
            painter.add(cb);
        }

        #[cfg(not(feature = "wgpu"))]
        {
            let cached = cached_frame_for(state, state.time);
            let mut flat_idx: usize = 0;

            fn draw_shapes_recursive(
                ui_painter: &egui::Painter,
                shapes: &[crate::scene::Shape],
                composition_rect: egui::Rect,
                _zoom: f32,
                current_time: f32,
                parent_spawn: f32,
                project_duration: f32,
                cached: Option<&Vec<(f32, f32)>>,
                flat_idx: &mut usize,
                handlers: &[crate::dsl::runtime::DslHandler],
            ) {
                let frame_idx = (current_time * 60.0).round() as u32;
                for shape in shapes {
                    let actual_spawn = shape.spawn_time().max(parent_spawn);
                    if current_time < actual_spawn { continue; }
                    match shape {
                        crate::scene::Shape::Circle { radius, color, .. } => {
                            let (eval_x, eval_y) = if let Some(frame) = cached {
                                let p = frame.get(*flat_idx).copied().unwrap_or((0.0, 0.0));
                                *flat_idx += 1;
                                p
                            } else {
                                *flat_idx += 1;
                                let mut transient = shape.clone();
                                crate::events::time_changed_event::apply_on_time_handlers(
                                    std::slice::from_mut(&mut transient),
                                    handlers,
                                    current_time,
                                    frame_idx,
                                );
                                animated_xy_for(&transient, current_time, project_duration)
                            };
                            let pos = composition_rect.min + egui::vec2(eval_x * composition_rect.width(), eval_y * composition_rect.height());
                            let scaled_radius = radius * composition_rect.width();
                            let c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                            ui_painter.circle_filled(pos, scaled_radius, c);
                        }
                        crate::scene::Shape::Rect { w, h, color, .. } => {
                            let (eval_x, eval_y) = if let Some(frame) = cached {
                                let p = frame.get(*flat_idx).copied().unwrap_or((0.0, 0.0));
                                *flat_idx += 1;
                                p
                            } else {
                                *flat_idx += 1;
                                let mut transient = shape.clone();
                                crate::events::time_changed_event::apply_on_time_handlers(
                                    std::slice::from_mut(&mut transient),
                                    handlers,
                                    current_time,
                                    frame_idx,
                                );
                                animated_xy_for(&transient, current_time, project_duration)
                            };
                            let pos = composition_rect.min + egui::vec2(eval_x * composition_rect.width(), eval_y * composition_rect.height());
                            let size = egui::vec2(w * composition_rect.width(), h * composition_rect.height());
                            let c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                            ui_painter.rect_filled(egui::Rect::from_min_size(pos, size), 0.0, c);
                        }
                        crate::scene::Shape::Group { children, .. } => {
                            draw_shapes_recursive(ui_painter, children, composition_rect, _zoom, current_time, actual_spawn, project_duration, cached, flat_idx, handlers);
                        }
                    }
                }
            }

            draw_shapes_recursive(&painter, &state.scene, composition_rect, zoom, state.time, 0.0, state.duration_secs, cached, &mut flat_idx, &state.dsl_event_handlers);
        }

        if main_ui_enabled && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if composition_rect.contains(pos) {
                    let paper_uv = egui::pos2(
                        (pos.x - composition_rect.min.x) / composition_rect.width(),
                        (pos.y - composition_rect.min.y) / composition_rect.height(),
                    );

                    if state.picker_active {
                        let color = sample_color_at(state, paper_uv, state.time);
                        let hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
                        ui.output_mut(|o| o.copied_text = hex.clone());
                        state.picker_color = color;
                        state.toast_message = Some(format!("Color {} copied to clipboard!", hex));
                        state.toast_type = crate::app_state::ToastType::Success;
                        state.toast_deadline = ui.input(|i| i.time) + 3.0;
                        state.picker_active = false;
                    } else {
                        fn find_hit_path(
                            shapes: &[crate::scene::Shape],
                            pos: egui::Pos2,
                            composition_rect: egui::Rect,
                            zoom: f32,
                            current_path: Vec<usize>,
                            current_time: f32,
                            parent_spawn: f32,
                        ) -> Option<Vec<usize>> {
                            for (i, shape) in shapes.iter().enumerate().rev() {
                                let actual_spawn = shape.spawn_time().max(parent_spawn);
                                if current_time < actual_spawn { continue; }
                                let mut path = current_path.clone();
                                path.push(i);
                                match shape {
                                    crate::scene::Shape::Circle { x, y, radius, .. } => {
                                        let center = composition_rect.left_top() + egui::vec2(*x * composition_rect.width(), *y * composition_rect.height());
                                        if pos.distance(center) <= radius * composition_rect.width() { return Some(path); }
                                    }
                                    crate::scene::Shape::Rect { x, y, w, h, .. } => {
                                        let min = composition_rect.left_top() + egui::vec2(*x * composition_rect.width(), *y * composition_rect.height());
                                        let rect = egui::Rect::from_min_size(min, egui::vec2(*w * composition_rect.width(), *h * composition_rect.height()));
                                        if rect.contains(pos) { return Some(path); }
                                    }
                                    crate::scene::Shape::Group { children, .. } => {
                                        if let Some(cp) = find_hit_path(children, pos, composition_rect, zoom, path, current_time, actual_spawn) { return Some(cp); }
                                    }
                                }
                            }
                            None
                        }

                        let hit_path = find_hit_path(&state.scene, pos, composition_rect, zoom, Vec::new(), state.time, 0.0);
                        if let Some(p) = hit_path {
                            state.selected = Some(p[0]);
                            state.selected_node_path = Some(p);
                        } else {
                            state.selected = None;
                            state.selected_node_path = None;
                        }
                    }
                } else {
                    state.selected = None;
                }
            }
        }

        if let Some(path) = &state.selected_node_path {
            let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 165, 0));
            fn draw_highlight_recursive(painter: &egui::Painter, shape: &crate::scene::Shape, composition_rect: egui::Rect, stroke: egui::Stroke, current_time: f32, parent_spawn: f32) {
                let actual_spawn = shape.spawn_time().max(parent_spawn);
                if current_time < actual_spawn { return; }
                match shape {
                    crate::scene::Shape::Circle { x, y, radius, .. } => {
                        let center = composition_rect.left_top() + egui::vec2(*x * composition_rect.width(), *y * composition_rect.height());
                        painter.circle_stroke(center, radius * composition_rect.width(), stroke);
                    }
                    crate::scene::Shape::Rect { x, y, w, h, .. } => {
                        let min = composition_rect.left_top() + egui::vec2(*x * composition_rect.width(), *y * composition_rect.height());
                        painter.rect_stroke(egui::Rect::from_min_size(min, egui::vec2(*w * composition_rect.width(), *h * composition_rect.height())), 0.0, stroke);
                    }
                    crate::scene::Shape::Group { children, .. } => {
                        for child in children { draw_highlight_recursive(painter, child, composition_rect, stroke, current_time, actual_spawn); }
                    }
                }
            }
            let mut current_node = state.scene.get(path[0]);
            for &idx in &path[1..] {
                current_node = match current_node { Some(crate::scene::Shape::Group { children, .. }) => children.get(idx), _ => None };
            }
            if let Some(node) = current_node {
                draw_highlight_recursive(&painter, node, composition_rect, stroke, state.time, 0.0);
            }
        }

        let menu_pos = rect.min + egui::vec2(10.0, 10.0);
        if !state.code_fullscreen {
            egui::Area::new("canvas_quick_settings").fixed_pos(menu_pos).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::none().fill(egui::Color32::from_black_alpha(150)).rounding(4.0).inner_margin(4.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let picker_btn = egui::Button::new(egui::RichText::new("📷").size(14.0))
                            .fill(if state.picker_active { egui::Color32::from_rgb(255, 100, 0) } else { egui::Color32::TRANSPARENT });
                        if ui.add(picker_btn).on_hover_text("Color Picker & Magnifier").clicked() { state.picker_active = !state.picker_active; }

                        let (r, _) = ui.allocate_at_least(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter().rect_filled(r.shrink(2.0), 2.0, egui::Color32::from_rgb(state.picker_color[0], state.picker_color[1], state.picker_color[2]));
                        ui.painter().rect_stroke(r.shrink(2.0), 2.0, egui::Stroke::new(1.0, egui::Color32::GRAY));

                        ui.separator();
                        ui.menu_button(format!("Preview: {}x", state.preview_multiplier), |ui| {
                            for &m in &[0.125, 0.25, 0.5, 1.0, 1.125, 1.25, 1.5, 2.0] {
                                if ui.selectable_label(state.preview_multiplier == m, format!("{}x", m)).clicked() { state.preview_multiplier = m; ui.close_menu(); }
                            }
                        });
                        ui.separator();
                        ui.add(egui::DragValue::new(&mut state.preview_fps).prefix("FPS: ").clamp_range(1..=240));
                        ui.separator();
                        if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                            let pct_x = (mouse_pos.x - composition_rect.min.x) / composition_rect.width();
                            let pct_y = (mouse_pos.y - composition_rect.min.y) / composition_rect.height();
                            ui.label(egui::RichText::new(format!("X: {:.2}%, Y: {:.2}%", pct_x * 100.0, pct_y * 100.0)).monospace().color(egui::Color32::LIGHT_BLUE));
                        } else {
                            ui.label(egui::RichText::new("X: ---%, Y: ---%").monospace().color(egui::Color32::GRAY));
                        }
                    });
                });
            });
        }
    });
}
