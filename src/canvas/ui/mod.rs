use crate::app_state::AppState;
use eframe::egui;

pub mod grid;
pub mod interaction;
pub mod toolbar;
pub mod transport_bar;

pub fn show(ui: &mut egui::Ui, state: &mut AppState, _main_ui_enabled: bool) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

    // Basic AutoCAD-like grid
    let painter = ui.painter_at(rect);
    grid::draw_grid(&painter, rect, state.canvas_zoom, egui::vec2(state.canvas_pan_x, state.canvas_pan_y));

    // UI Overlay (Transport Controls)
    transport_bar::show(ui, state, rect);

    // Simple interaction stubs
    interaction::handle_pan_zoom(ui, state, rect, &response);
}
