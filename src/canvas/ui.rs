#[cfg(feature = "wgpu")]
use super::gpu::{CompositionCallback, GpuShape};
use super::rasterizer::sample_color_at;
use crate::app_state::AppState;
use eframe::egui;

/// Renderiza y maneja las interacciones para el área del canvas central.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, main_ui_enabled: bool) {
    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        // Materialize `ElementKeyframes` into temporary `Shape` instances
        // sampled at the current playhead time. This lets the existing
        // canvas rendering/pathfinding logic keep working while the
        // canonical storage is `ElementKeyframes`.
        let mut live_shapes: Vec<crate::scene::Shape> = Vec::new();
        for elem in &state.scene {
            let frame_idx = crate::shapes::element_store::seconds_to_frame(state.time, state.fps);
            if let Some(s) = elem.to_shape_at_frame(frame_idx, state.fps) {
                live_shapes.push(s);
            }
        }

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
        while base_step * zoom > 200.0 {
            base_step /= 10.0;
        }
        while base_step * zoom < 20.0 {
            base_step *= 10.0;
        }

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
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    grid_stroke,
                );
            }
            x += step;
        }

        let mut y = start_y;
        while y <= rect.bottom() + step {
            if y >= rect.top() {
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    grid_stroke,
                );
            }
            y += step;
        }

        if grid_origin.x >= rect.left() && grid_origin.x <= rect.right() {
            painter.line_segment(
                [
                    egui::pos2(grid_origin.x, rect.top()),
                    egui::pos2(grid_origin.x, rect.bottom()),
                ],
                origin_stroke_y,
            );
        }
        if grid_origin.y >= rect.top() && grid_origin.y <= rect.bottom() {
            painter.line_segment(
                [
                    egui::pos2(rect.left(), grid_origin.y),
                    egui::pos2(rect.right(), grid_origin.y),
                ],
                origin_stroke_x,
            );
        }

        let composition_size =
            egui::vec2(state.render_width as f32, state.render_height as f32) * zoom;
        let composition_min = grid_origin - composition_size / 2.0;
        let composition_rect = egui::Rect::from_min_size(composition_min, composition_size);

        state.last_composition_rect = Some(composition_rect);

        let shadow_rect = composition_rect.expand(4.0 * zoom);
        painter.rect_filled(shadow_rect, 0.0, egui::Color32::from_black_alpha(100));

        painter.rect_filled(composition_rect, 0.0, egui::Color32::WHITE);
        painter.rect_stroke(
            composition_rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        );

        // PositionCache removed — always compute on-the-fly; no cached frame available.

        let mut gpu_shapes = Vec::new();
        // position cache removed => no cached per-frame positions available
        let cached: Option<&Vec<(f32, f32)>> = None;
        let mut flat_idx: usize = 0;

        // Información de cada elemento Text encontrado en la escena durante fill_gpu_shapes.
        // Guardamos el índice en gpu_shapes donde insertamos el placeholder y el shape clonado
        // para rasterizarlo por separado después.
        struct TextEntry {
            gpu_idx: usize, // posición en gpu_shapes (antes del reverse)
            shape: crate::scene::Shape,
            parent_spawn: f32,
        }
        let mut text_entries: Vec<TextEntry> = Vec::new();

        #[allow(clippy::too_many_arguments)]
        fn fill_gpu_shapes(
            elements: &[crate::shapes::element_store::ElementKeyframes],
            gpu_shapes: &mut Vec<GpuShape>,
            text_entries: &mut Vec<TextEntry>,
            render_width: f32,
            render_height: f32,
            parent_spawn: f32,
            current_time: f32,
            project_duration: f32,
            cached: Option<&Vec<(f32, f32)>>,
            flat_idx: &mut usize,
            handlers: &[crate::dsl::runtime::DslHandler],
            preview_fps: u32,
        ) {
            let frame_idx = crate::shapes::element_store::seconds_to_frame(current_time, preview_fps);
            for ek in elements {
                // Respect spawn/kill ranges early
                if frame_idx < ek.spawn_frame {
                    continue;
                }
                if let Some(kf) = ek.kill_frame {
                    if frame_idx >= kf {
                        *flat_idx += 1;
                        continue;
                    }
                }

                // Materialize a temporary Shape from ElementKeyframes so we can
                // reuse the existing animation/event handler infrastructure.
                if let Some(shape) = ek.to_shape_at_frame(frame_idx, preview_fps) {
                    let my_spawn = (ek.spawn_frame as f32 / preview_fps as f32).max(parent_spawn);

                    // Apply event handlers to a transient copy so per-shape
                    // descriptor implementations see the same runtime-modified
                    // state that the rasterizer/preview worker would.
                    *flat_idx += 1; // keep frame indexing consistent with previous behaviour
                    let mut transient = shape.clone();
                    crate::events::time_changed_event::apply_on_time_handlers(
                        std::slice::from_mut(&mut transient),
                        handlers,
                        current_time,
                        frame_idx.try_into().unwrap(),
                    );

                    let before = gpu_shapes.len();
                    if let Some(desc) = transient.descriptor() {
                        desc.append_gpu_shapes(
                            &transient,
                            gpu_shapes,
                            current_time,
                            project_duration,
                            my_spawn,
                            render_width,
                            render_height,
                        );

                        // If this descriptor represents `text` we still need to
                        // record a TextEntry for rasterization/UV population.
                        if desc.dsl_keyword() == "text" {
                            // descriptor should have appended one placeholder
                            text_entries.push(TextEntry {
                                gpu_idx: before,
                                shape: transient.clone(),
                                parent_spawn: my_spawn,
                            });
                        }
                    }
                }
            }
        }

        // Use ElementKeyframes (canonical store) to populate GPU shapes.
        fill_gpu_shapes(
            &state.scene,
            &mut gpu_shapes,
            &mut text_entries,
            state.render_width as f32,
            state.render_height as f32,
            0.0,
            state.time,
            state.duration_secs,
            cached,
            &mut flat_idx,
            &state.dsl_event_handlers,
            state.preview_fps,
        );

        // Scene index 0 = top of scene graph = rendered last (on top).
        // Reversing achieves painter's-algorithm ordering without z_index.
        gpu_shapes.reverse();

        // --- Atlas de texto por elemento ---
        // Rasterizamos cada Text por separado en una franja del atlas vertical.
        // Atlas: ancho = render_width, alto = render_height * N (una fila por texto).
        // GpuShape placeholder en gpu_shapes ya tiene shape_type=2; actualizamos sus UVs.
        //
        // NOTA: gpu_shapes fue invertido, así que el índice original `gpu_idx` ahora está
        // en la posición `gpu_shapes.len() - 1 - gpu_idx` dentro del vec.
        let n_texts = text_entries.len();
        let text_pixels = if n_texts > 0 {
            let rw = state.render_width;
            let rh = state.render_height;
            let atlas_h = rh * n_texts as u32;
            let mut atlas = vec![0u8; (rw * atlas_h * 4) as usize];

            for (tile_idx, entry) in text_entries.iter().enumerate() {
                // UV de la franja de este tile en el atlas
                let uv0_y = tile_idx as f32 / n_texts as f32;
                let uv1_y = (tile_idx + 1) as f32 / n_texts as f32;

                // Rasterizar este texto a su propio buffer rh×rw
                if let Some(result) = crate::canvas::text_rasterizer::rasterize_single_text(
                    &entry.shape,
                    rw,
                    rh,
                    state.time,
                    state.duration_secs,
                    &mut state.font_arc_cache,
                    &state.font_map,
                    &state.dsl_event_handlers,
                    entry.parent_spawn,
                ) {
                    // Copiar píxeles al atlas en la franja correcta
                    let row_offset = (tile_idx as u32 * rh * rw * 4) as usize;
                    let copy_len = (rw * rh * 4) as usize;
                    atlas[row_offset..row_offset + copy_len].copy_from_slice(&result.pixels);
                }

                // Actualizar UVs del placeholder (que ahora está en posición invertida)
                let reversed_idx = gpu_shapes.len() - 1 - entry.gpu_idx;
                if let Some(s) = gpu_shapes.get_mut(reversed_idx) {
                    s.uv0 = [0.0, uv0_y];
                    s.uv1 = [1.0, uv1_y];
                }
            }

            Some((atlas, rw, atlas_h))
        } else {
            None
        };

        let magnifier_pos = if state.picker_active {
            ui.input(|i| i.pointer.hover_pos())
        } else {
            None
        };

        // Check if we have a native GPU cached frame for this time. When a
        // side panel that can modify the scene (Code or SceneGraph) is open
        // we prefer the live composition callback to avoid swapping between
        // 'cached texture' and 'callback render' which produces visible
        // flicker on high-refresh displays.
        let mut drawn_from_cache = false;
        if state.active_tab.is_none() {
            if let Some(tex_id) = state.preview_native_texture_id {
                if let Some(t) = state.preview_cache_center_time {
                    // Tolerance of 1/2 frame
                    if (t - state.time).abs() < (1.0 / state.fps as f32) * 0.5 {
                        painter.image(
                            tex_id,
                            composition_rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                        drawn_from_cache = true;
                    }
                }
            }
        }

        if !drawn_from_cache {
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
                    shared_device: None,
                    shared_queue: None,
                    text_pixels,
                },
            );
            painter.add(cb);
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
                        let hit_path = crate::shapes::shapes_manager::find_hit_path(
                            &live_shapes,
                            pos,
                            composition_rect,
                            zoom,
                            state.time,
                            0.0,
                            state.render_height,
                        );
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
            if let Some(node) = crate::shapes::shapes_manager::get_shape(&live_shapes, path) {
                crate::shapes::shapes_manager::draw_highlight_recursive(
                    &painter,
                    node,
                    composition_rect,
                    stroke,
                    state.time,
                    0.0,
                    state.render_height,
                );
            }
        }

        let menu_pos = rect.min + egui::vec2(10.0, 10.0);
        if !state.code_fullscreen {
            egui::Area::new("canvas_quick_settings")
                .fixed_pos(menu_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::none()
                        .fill(egui::Color32::from_black_alpha(150))
                        .rounding(4.0)
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                let picker_btn = egui::Button::new(
                                    egui::RichText::new("📷").size(14.0),
                                )
                                .fill(if state.picker_active {
                                    egui::Color32::from_rgb(255, 100, 0)
                                } else {
                                    egui::Color32::TRANSPARENT
                                });
                                if ui
                                    .add(picker_btn)
                                    .on_hover_text("Color Picker & Magnifier")
                                    .clicked()
                                {
                                    state.picker_active = !state.picker_active;
                                }

                                let (r, _) = ui.allocate_at_least(
                                    egui::vec2(14.0, 14.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    r.shrink(2.0),
                                    2.0,
                                    egui::Color32::from_rgb(
                                        state.picker_color[0],
                                        state.picker_color[1],
                                        state.picker_color[2],
                                    ),
                                );
                                ui.painter().rect_stroke(
                                    r.shrink(2.0),
                                    2.0,
                                    egui::Stroke::new(1.0, egui::Color32::GRAY),
                                );

                                ui.separator();
                                ui.menu_button(
                                    format!("Preview: {}x", state.preview_multiplier),
                                    |ui| {
                                        for &m in &[0.125, 0.25, 0.5, 1.0, 1.125, 1.25, 1.5, 2.0] {
                                            if ui
                                                .selectable_label(
                                                    state.preview_multiplier == m,
                                                    format!("{}x", m),
                                                )
                                                .clicked()
                                            {
                                                state.preview_multiplier = m;
                                                ui.close_menu();
                                            }
                                        }
                                    },
                                );
                                ui.separator();
                                ui.add(
                                    egui::DragValue::new(&mut state.preview_fps)
                                        .prefix("FPS: ")
                                        .clamp_range(1..=240),
                                );
                                ui.separator();
                                if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                                    let pct_x = (mouse_pos.x - composition_rect.min.x)
                                        / composition_rect.width();
                                    let pct_y = (mouse_pos.y - composition_rect.min.y)
                                        / composition_rect.height();
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "X: {:.2}%, Y: {:.2}%",
                                            pct_x * 100.0,
                                            pct_y * 100.0
                                        ))
                                        .monospace()
                                        .color(egui::Color32::LIGHT_BLUE),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new("X: ---%, Y: ---%")
                                            .monospace()
                                            .color(egui::Color32::GRAY),
                                    );
                                }
                            });
                        });
                });
        }
    });
}
