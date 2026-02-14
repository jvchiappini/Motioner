use crate::app_state::AppState;
use eframe::egui;

/// Render and handle interactions for the central canvas area.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, main_ui_enabled: bool) {
    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        // Use Sense::click() to properly capture clicks and respect occlusion by modals
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::BLACK); // Canvas bg

        // Helper to convert normalized (x: 0.0..1.0) coords to pixel space inside the canvas rect.
        let to_pixel_pos = |nx: f32, ny: f32| {
            egui::pos2(rect.left() + nx * rect.width(), rect.top() + ny * rect.height())
        };

        // Draw objects from the scene
        for (i, shape) in state.scene.iter().enumerate() {
            let is_selected = Some(i) == state.selected;
            let stroke = if is_selected {
                egui::Stroke::new(2.0, egui::Color32::YELLOW)
            } else {
                egui::Stroke::NONE
            };

            match shape {
                crate::scene::Shape::Circle { x, y, radius, color } => {
                    let center = to_pixel_pos(*x, *y);
                    painter.circle(
                        center,
                        *radius,
                        egui::Color32::from_rgb(color[0], color[1], color[2]),
                        stroke,
                    );
                }
                crate::scene::Shape::Rect { x, y, w, h, color } => {
                    let min = to_pixel_pos(*x, *y);
                    painter.rect(
                        egui::Rect::from_min_size(min, egui::vec2(*w, *h)),
                        0.0,
                        egui::Color32::from_rgb(color[0], color[1], color[2]),
                        stroke,
                    );
                }
            }
        }

        // Interaction: clicks / selection
        if main_ui_enabled && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let mut hit = None;
                for (i, shape) in state.scene.iter().enumerate() {
                    match shape {
                        crate::scene::Shape::Circle { x, y, radius, .. } => {
                            let center = to_pixel_pos(*x, *y);
                            if pos.distance(center) <= *radius {
                                hit = Some(i);
                            }
                        }
                        crate::scene::Shape::Rect { x, y, w, h, .. } => {
                            let min = to_pixel_pos(*x, *y);
                            let max = min + egui::vec2(*w, *h);
                            if pos.x >= min.x && pos.x <= max.x && pos.y >= min.y && pos.y <= max.y {
                                hit = Some(i);
                            }
                        }
                    }
                }
                state.selected = hit;
            } else {
                state.selected = None;
            }
        }
    });
}
