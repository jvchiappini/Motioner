use crate::app_state::{AppState, Tool};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        
        let button_size = egui::vec2(36.0, 36.0);
        
        // Selection tool
        if toolbar_button(ui, "🏹", "Selection (S)", button_size, state.active_tool == Tool::Select).clicked() {
            state.active_tool = Tool::Select;
        }
        ui.add_space(8.0);
        
        // Shapes
        if toolbar_button(ui, "⬜", "Rectangle (R)", button_size, state.active_tool == Tool::Rectangle).clicked() {
            state.active_tool = Tool::Rectangle;
        }
        ui.add_space(4.0);
        if toolbar_button(ui, "⚪", "Circle (C)", button_size, state.active_tool == Tool::Circle).clicked() {
            state.active_tool = Tool::Circle;
        }
        ui.add_space(4.0);
        if toolbar_button(ui, "Ｔ", "Text (T)", button_size, state.active_tool == Tool::Text).clicked() {
            state.active_tool = Tool::Text;
        }
        
        ui.add_space(24.0);
        ui.separator();
        ui.add_space(24.0);
        
        // Settings or other tools
        toolbar_button(ui, "⚙", "Settings", button_size, false).on_hover_text("Settings");
        ui.add_space(4.0);
        toolbar_button(ui, "🧱", "Scene Graph", button_size, false).on_hover_text("Scene Graph");
        ui.add_space(4.0);
        toolbar_button(ui, "{}", "Generate Code", button_size, false).on_hover_text("Generate Code");
    });
}

fn toolbar_button(ui: &mut egui::Ui, icon: &str, _tooltip: &str, size: egui::Vec2, selected: bool) -> egui::Response {
    let response = ui.add_sized(size, egui::Button::new(egui::RichText::new(icon).size(20.0))
        .frame(false)
        .fill(egui::Color32::TRANSPARENT));
    
    let painter = ui.painter();
    let rect = response.rect.shrink(2.0);
    
    if selected {
        painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(60, 60, 70));
        painter.rect_stroke(rect, 6.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 150, 255)));
    } else if response.clicked() {
        painter.rect_filled(rect, 6.0, egui::Color32::from_white_alpha(40));
    } else if response.hovered() {
        painter.rect_filled(rect, 6.0, egui::Color32::from_white_alpha(15));
    }
    
    response
}
