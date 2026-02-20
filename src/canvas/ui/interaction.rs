/// Maneja las interacciones del usuario con el canvas: zoom, pan y selección de objetos.

use eframe::egui;
use crate::app_state::AppState;
use super::super::rasterizer::sample_color_at;

/// Procesa el zoom y pan del canvas según la entrada del ratón.
pub fn handle_pan_zoom(ui: &egui::Ui, state: &mut AppState, rect: egui::Rect, response: &egui::Response) {
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

/// Maneja los clics en el canvas para seleccionar elementos o usar el cuentagotas.
pub fn handle_canvas_clicks(
    ui: &mut egui::Ui,
    state: &mut AppState,
    response: &egui::Response,
    composition_rect: egui::Rect,
    zoom: f32
) {
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
                state.toast_message = Some(format!("Color {} copiado al portapapeles!", hex));
                state.toast_type = crate::app_state::ToastType::Success;
                state.toast_deadline = ui.input(|i| i.time) + 3.0;
                state.picker_active = false;
            } else {
                // Realizar Hit Test para seleccionar formas
                let frame_idx = crate::shapes::element_store::seconds_to_frame(state.time, state.fps);
                let mut live_shapes: Vec<crate::scene::Shape> = Vec::with_capacity(state.scene.len());
                for elem in &state.scene {
                    if frame_idx >= elem.spawn_frame && elem.kill_frame.map_or(true, |k| frame_idx < k) {
                        if let Some(s) = elem.to_shape_at_frame(frame_idx, state.fps) {
                            live_shapes.push(s);
                        }
                    }
                }

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
