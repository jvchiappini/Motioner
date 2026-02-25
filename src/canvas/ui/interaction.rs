use crate::app_state::AppState;
use eframe::egui;

/// Procesa el zoom y pan del canvas según la entrada del ratón.
pub fn handle_pan_zoom(
    ui: &egui::Ui,
    state: &mut AppState,
    rect: egui::Rect,
    response: &egui::Response,
) {
    // Paneo (Pan)
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        state.canvas_pan_x += response.drag_delta().x;
        state.canvas_pan_y += response.drag_delta().y;
    }

    // Zoom
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
                state.canvas_pan_x = (state.canvas_pan_x - (mouse_pos.x - center.x)) * actual_delta
                    + (mouse_pos.x - center.x);
                state.canvas_pan_y = (state.canvas_pan_y - (mouse_pos.y - center.y)) * actual_delta
                    + (mouse_pos.y - center.y);
            } else {
                state.canvas_zoom *= zoom_delta;
                state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
            }
        }
    }
}
