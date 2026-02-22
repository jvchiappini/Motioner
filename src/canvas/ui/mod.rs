/// Este módulo orquestra la interfaz de usuario del canvas central.
/// Divide la lógica en interacción, cuadrícula, barra de herramientas y procesamiento de texto.

use eframe::egui;
use crate::app_state::AppState;

mod interaction;
mod grid;
mod toolbar;
mod text_atlas;

#[cfg(feature = "wgpu")]
use super::gpu::CompositionCallback;

/// Punto de entrada principal para renderizar el área del canvas.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, main_ui_enabled: bool) {
    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::drag().union(egui::Sense::click()),
        );

        // 1. Manejar Pan y Zoom
        if main_ui_enabled {
            interaction::handle_pan_zoom(ui, state, rect, &response);
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(60, 60, 60));

        let zoom = state.canvas_zoom;
        let pan = egui::vec2(state.canvas_pan_x, state.canvas_pan_y);

        // 2. Dibujar Cuadrícula
        grid::draw_grid(&painter, rect, zoom, pan);

        // 3. Calcular Rect de Composición
        let comp_size = egui::vec2(state.render_width as f32, state.render_height as f32) * zoom;
        let comp_origin = rect.center() + pan - comp_size / 2.0;
        let composition_rect = egui::Rect::from_min_size(comp_origin, comp_size);
        state.last_composition_rect = Some(composition_rect);

        // Sombra y fondo del papel
        painter.rect_filled(composition_rect.expand(4.0 * zoom), 0.0, egui::Color32::from_black_alpha(100));
        painter.rect_filled(composition_rect, 0.0, egui::Color32::WHITE);
        painter.rect_stroke(composition_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::BLACK));

        // 4. Procesar Texto (Solo si WGPU está activo)
        let (text_pixels, text_overrides) = if state.wgpu_render_state.is_some() {
            text_atlas::prepare_text_atlas(state)
        } else {
            (None, None)
        };

        let magnifier_pos = if state.picker_active { ui.input(|i| i.pointer.hover_pos()) } else { None };
        
        // 5. Renderizado 100% GPU
        // Hemos eliminado el uso de caché de texturas (egui::Image) para el área de composición.
        // Esto garantiza fidelidad absoluta y elimina problemas de espacios de color entre texturas y renderizado directo.
        
        let cb = eframe::egui_wgpu::Callback::new_paint_callback(
                rect,
                CompositionCallback {
                    render_width: state.render_width as f32,
                    render_height: state.render_height as f32,
                    preview_multiplier: state.preview_multiplier,
                    paper_rect: composition_rect,
                    viewport_rect: rect,
                    magnifier_pos,
                    time: state.time,
                    text_pixels,
                    elements: state.wgpu_render_state.as_ref().map(|_| state.scene.clone()),
                    current_frame: crate::shapes::element_store::seconds_to_frame(state.time, state.fps) as u32,
                    fps: state.fps,
                    scene_version: state.scene_version,
                    text_overrides,
                },
            );
            painter.add(cb);
        // The closing brace for the `egui::Frame::canvas(...).show` closure was misplaced here.
        // It is now removed, allowing the following code to be part of the closure.

        // 7. Manejar Clics, selección y redimensionado
        // If resize or move mode is enabled we want to update the cursor even when
        // the user is merely hovering near a shape, not only when they click.
        // Thus we invoke the interaction handler whenever the canvas is
        // hovered in those modes, or on a click/drag as before.
        let is_interacting = state.resize_mode || state.move_mode;
        if main_ui_enabled
            && (response.clicked()
                || (is_interacting && (response.hovered() || response.dragged_by(egui::PointerButton::Primary))))
        {
            interaction::handle_canvas_clicks(ui, state, &response, composition_rect, zoom);
        }

        // 8. Dibujar Highlight de selección
        if let Some(path) = &state.selected_node_path {
            let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 165, 0));
            let frame_idx = crate::shapes::element_store::seconds_to_frame(state.time, state.fps);
            if let Some(elem) = state.scene.get(path[0]) {
                 if let Some(node) = elem.to_shape_at_frame(frame_idx, state.fps) {
                    crate::shapes::shapes_manager::draw_highlight_recursive(
                        &painter, &node, composition_rect, stroke, state.time, 0.0, state.render_height,
                    );
                }
            }
        }

        // 8b. When move mode is active show directional arrows on the
        // selected element so the user can see that dragging will translate
        // the shape.  This is similar to the cursor change above but
        // provides a persistent visual hint.
        if state.move_mode {
            // reuse the same selection logic as for highlights
            let maybe_path = state
                .selected_node_path
                .as_ref()
                .cloned()
                .or_else(|| state.selected.map(|i| vec![i]));
            if let Some(path) = maybe_path {
                let frame_idx = crate::shapes::element_store::seconds_to_frame(state.time, state.fps);
                if let Some(elem) = state.scene.get(path[0]) {
                    if let Some(node) = elem.to_shape_at_frame(frame_idx, state.fps) {
                        // compute bounding rectangle for the shape
                        let bounds = match &node {
                            crate::scene::Shape::Rect(r) => {
                                let centre = composition_rect.left_top()
                                    + egui::vec2(
                                        r.x * composition_rect.width(),
                                        r.y * composition_rect.height(),
                                    );
                                let w = r.w * composition_rect.width();
                                let h = r.h * composition_rect.height();
                                egui::Rect::from_center_size(centre, egui::vec2(w, h))
                            }
                            crate::scene::Shape::Circle(c) => {
                                let centre = composition_rect.left_top()
                                    + egui::vec2(
                                        c.x * composition_rect.width(),
                                        c.y * composition_rect.height(),
                                    );
                                egui::Rect::from_center_size(
                                    centre,
                                    egui::vec2(
                                        2.0 * c.radius * composition_rect.width(),
                                        2.0 * c.radius * composition_rect.width(),
                                    ),
                                )
                            }
                            crate::scene::Shape::Text(t) => {
                                let centre = composition_rect.left_top()
                                    + egui::vec2(
                                        t.x * composition_rect.width(),
                                        t.y * composition_rect.height(),
                                    );
                                let h = t.size * composition_rect.height();
                                let w = t.value.len() as f32 * h * 0.5;
                                egui::Rect::from_center_size(centre, egui::vec2(w, h))
                            }
                            _ => egui::Rect::EVERYTHING,
                        };
                        // draw professional move gizmo arrows (Red for X, Green for Y)
                        let cen = bounds.center();
                        let arrow_len = 60.0;
                        let head_size = 12.0;

                        // Highlight if hovering
                        let hover_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or(egui::Pos2::ZERO);
                        let x_arrow_rect = egui::Rect::from_min_max(
                            egui::pos2(cen.x, cen.y - 12.0),
                            egui::pos2(cen.x + arrow_len, cen.y + 12.0),
                        );
                        let y_arrow_rect = egui::Rect::from_min_max(
                            egui::pos2(cen.x - 12.0, cen.y),
                            egui::pos2(cen.x + 12.0, cen.y + arrow_len),
                        );

                        let color_x = if x_arrow_rect.contains(hover_pos) { egui::Color32::LIGHT_RED } else { egui::Color32::from_rgb(200, 40, 40) };
                        let color_y = if y_arrow_rect.contains(hover_pos) { egui::Color32::LIGHT_GREEN } else { egui::Color32::from_rgb(40, 200, 40) };

                        // X axis (Horizontal)
                        let x_end = egui::pos2(cen.x + arrow_len, cen.y);
                        painter.line_segment([cen, x_end], egui::Stroke::new(3.0, color_x));
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                egui::pos2(x_end.x, x_end.y),
                                egui::pos2(x_end.x - head_size, x_end.y - head_size * 0.5),
                                egui::pos2(x_end.x - head_size, x_end.y + head_size * 0.5),
                            ],
                            color_x,
                            egui::Stroke::NONE,
                        ));

                        // Y axis (Vertical)
                        let y_end = egui::pos2(cen.x, cen.y + arrow_len);
                        painter.line_segment([cen, y_end], egui::Stroke::new(3.0, color_y));
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                egui::pos2(y_end.x, y_end.y),
                                egui::pos2(y_end.x - head_size * 0.5, y_end.y - head_size),
                                egui::pos2(y_end.x + head_size * 0.5, y_end.y - head_size),
                            ],
                            color_y,
                            egui::Stroke::NONE,
                        ));

                        // Origin pivot point
                        painter.circle_filled(cen, 4.0, egui::Color32::WHITE);
                        painter.circle_stroke(cen, 4.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
                    }
                }
            }
        }

        // 9. Mostrar Barra de Herramientas
        if main_ui_enabled && !state.code_fullscreen {
            toolbar::show_toolbar(ui, state, composition_rect);
        }
    });
}
