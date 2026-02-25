use crate::app_state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(30, 30, 32))
        .show(ui, |ui| {
            let (rect, response) = ui.allocate_at_least(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            
            let ruler_height = 28.0;
            let row_height = 24.0;
            let gutter_width = 150.0;
            let font_id = egui::FontId::proportional(11.0);
            
            // Background grid for tracks
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(25, 25, 27));

            // Pan and Zoom logic
            if response.dragged_by(egui::PointerButton::Middle) {
                state.timeline_pan_x -= response.drag_delta().x;
                state.timeline_pan_y -= response.drag_delta().y;
            }
            
            let zoom = state.timeline_zoom;
            let pan_x = state.timeline_pan_x;
            let pan_y = state.timeline_pan_y;
            let time_origin_x = rect.left() + gutter_width - pan_x;

            // --- Ruler ---
            let ruler_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), ruler_height));
            painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(45, 45, 47));
            
            let start_t = (pan_x / zoom).floor() as i32;
            let end_t = ((pan_x + rect.width()) / zoom).ceil() as i32;
            
            for t in start_t..=end_t {
                if t < 0 { continue; }
                let x = time_origin_x + t as f32 * zoom;
                if x < rect.left() + gutter_width || x > rect.right() { continue; }
                
                painter.line_segment(
                    [egui::pos2(x, ruler_rect.top() + 15.0), egui::pos2(x, ruler_rect.bottom())],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(100))
                );
                painter.text(
                    egui::pos2(x + 2.0, ruler_rect.top() + 4.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}s", t),
                    font_id.clone(),
                    egui::Color32::from_gray(180)
                );
            }

            // --- Tracks ---
            let tracks_clip_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + ruler_height),
                rect.max
            );
            
            let mut current_y = rect.top() + ruler_height - pan_y;
            for (i, shape) in state.scene.iter().enumerate() {
                let track_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), current_y),
                    egui::vec2(rect.width(), row_height)
                );
                
                if track_rect.bottom() > tracks_clip_rect.top() && track_rect.top() < tracks_clip_rect.bottom() {
                    // Gutter background
                    let gutter_rect = egui::Rect::from_min_size(track_rect.min, egui::vec2(gutter_width, row_height));
                    painter.rect_filled(gutter_rect, 0.0, egui::Color32::from_rgb(32, 32, 34));
                    
                    // Gutter text
                    painter.text(
                        gutter_rect.min + egui::vec2(8.0, 6.0),
                        egui::Align2::LEFT_TOP,
                        shape.name(),
                        font_id.clone(),
                        if state.selected == Some(i) { egui::Color32::WHITE } else { egui::Color32::from_gray(150) }
                    );
                    
                    // Track content
                    let bar_start = time_origin_x;
                    let bar_end = time_origin_x + state.duration_secs * zoom;
                    let bar_rect = egui::Rect::from_min_max(
                        egui::pos2(bar_start.max(rect.left() + gutter_width), current_y + 4.0),
                        egui::pos2(bar_end.min(rect.right()), current_y + row_height - 4.0)
                    );
                    
                    if bar_rect.width() > 0.0 {
                        painter.rect_filled(bar_rect, 2.0, egui::Color32::from_rgb(70, 70, 90));
                        if state.selected == Some(i) {
                            painter.rect_stroke(bar_rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 150, 255)));
                        }
                    }
                }
                current_y += row_height;
            }

            // Gutter separator
            painter.line_segment(
                [egui::pos2(rect.left() + gutter_width, rect.top()), egui::pos2(rect.left() + gutter_width, rect.bottom())],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(20, 20, 22))
            );

            // --- Playhead ---
            let ph_x = time_origin_x + state.time * zoom;
            if ph_x >= rect.left() + gutter_width && ph_x <= rect.right() {
                painter.line_segment(
                    [egui::pos2(ph_x, rect.top()), egui::pos2(ph_x, rect.bottom())],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 80, 80))
                );
                // Handle playhead triangle on ruler
                let tri_size = 6.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(ph_x - tri_size, rect.top()),
                        egui::pos2(ph_x + tri_size, rect.top()),
                        egui::pos2(ph_x, rect.top() + tri_size * 2.0),
                    ],
                    egui::Color32::from_rgb(255, 80, 80),
                    egui::Stroke::NONE
                ));
            }

            // Scrubbing
            if response.dragged_by(egui::PointerButton::Primary) {
                if let Some(pos) = response.interact_pointer_pos() {
                    if pos.x >= rect.left() + gutter_width {
                        let new_time = (pos.x - time_origin_x) / zoom;
                        state.set_time(new_time);
                    }
                }
            }
        });
}
