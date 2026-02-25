use crate::app_state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical_centered(|ui| {
        ui.add_space(10.0);
        
        // Custom button style for toolbar
        let button_size = egui::vec2(32.0, 32.0);
        
        toolbar_button(ui, "⬈", "Selection (S)", button_size);
        ui.add_space(4.0);
        toolbar_button(ui, "▭", "Rectangle (R)", button_size);
        ui.add_space(4.0);
        toolbar_button(ui, "◯", "Circle (C)", button_size);
        ui.add_space(4.0);
        toolbar_button(ui, "T", "Text (T)", button_size);
        
        ui.add_space(20.0);
        ui.separator();
        ui.add_space(20.0);
        
        let play_icon = if state.playing { "⏸" } else { "⏵" };
        if toolbar_button(ui, play_icon, "Play/Pause (Space)", button_size).clicked() {
            state.playing = !state.playing;
        }

        ui.add_space(4.0);
        if toolbar_button(ui, "⏮", "Reset Time", button_size).clicked() {
            state.set_time(0.0);
        }
    });
}

fn toolbar_button(ui: &mut egui::Ui, icon: &str, tooltip: &str, size: egui::Vec2) -> egui::Response {
    let response = ui.add_sized(size, egui::Button::new(egui::RichText::new(icon).size(18.0).strong())
        .frame(false)
        .fill(egui::Color32::TRANSPARENT));
    
    let response = response.on_hover_text(tooltip);
    
    // Custom hover/click feedback
    let painter = ui.painter();
    if response.clicked() {
        painter.rect_filled(response.rect.shrink(2.0), 4.0, egui::Color32::from_white_alpha(40));
    } else if response.hovered() {
        painter.rect_filled(response.rect.shrink(2.0), 4.0, egui::Color32::from_white_alpha(20));
    }
    
    response
}
