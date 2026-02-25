use crate::app_state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);

        let button_size = egui::vec2(36.0, 36.0);

        use crate::app_state::PanelTab;

        if toolbar_button(ui, "⚙", "Settings", button_size, state.show_settings)
            .on_hover_text("Settings")
            .clicked()
        {
            state.show_settings = !state.show_settings;
        }
        ui.add_space(4.0);
        
        let is_scene = state.active_tab == Some(PanelTab::SceneGraph);
        if toolbar_button(ui, "☰", "Scene Graph", button_size, is_scene)
            .on_hover_text("Scene Graph")
            .clicked()
        {
            if is_scene {
                state.active_tab = None;
            } else {
                state.active_tab = Some(PanelTab::SceneGraph);
            }
        }

        ui.add_space(4.0);

        let is_code = state.active_tab == Some(PanelTab::Code);
        if toolbar_button(ui, "{}", "Code", button_size, is_code)
            .on_hover_text("Generate Code")
            .clicked()
        {
            if is_code {
                state.active_tab = None;
            } else {
                state.active_tab = Some(PanelTab::Code);
            }
        }
    });
}

fn toolbar_button(
    ui: &mut egui::Ui,
    icon: &str,
    _tooltip: &str,
    size: egui::Vec2,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    let painter = ui.painter();
    let draw_rect = rect.shrink(2.0);

    if selected {
        painter.rect_filled(draw_rect, 6.0, egui::Color32::from_rgb(60, 60, 70));
        painter.rect_stroke(
            draw_rect,
            6.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 150, 255)),
        );
    } else if response.is_pointer_button_down_on() {
        painter.rect_filled(draw_rect, 6.0, egui::Color32::from_white_alpha(40));
    } else if response.hovered() {
        painter.rect_filled(draw_rect, 6.0, egui::Color32::from_white_alpha(15));
    }

    let text_color = if selected || response.hovered() {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_gray(200)
    };

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(20.0),
        text_color,
    );

    response
}
