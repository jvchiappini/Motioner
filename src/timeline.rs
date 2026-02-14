use eframe::egui;
use crate::app_state::AppState;
use crate::scene::Shape;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    // Dark background implicit in panel, but we can force a frame for the specific look
    egui::Frame::none().fill(egui::Color32::from_rgb(40, 40, 42)).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        
        let ruler_height = 24.0;
        let row_height = 24.0;

        // --- Input Handling (Pan & Zoom) ---
        // Panning with Middle Mouse or Shift+Scroll
        if response.dragged_by(egui::PointerButton::Middle) {
             state.timeline_pan_x -= response.drag_delta().x;
             state.timeline_pan_y -= response.drag_delta().y;
        }
        
        // Zoom with Ctrl + Scroll
        let scroll_delta = ui.input(|i| i.raw_scroll_delta);
        if ui.input(|i| i.modifiers.ctrl) && scroll_delta.y != 0.0 {
             let zoom_factor = if scroll_delta.y > 0.0 { 1.1 } else { 0.9 };
             state.timeline_zoom *= zoom_factor;
             state.timeline_zoom = state.timeline_zoom.clamp(10.0, 1000.0);
        } else if scroll_delta.x != 0.0 || scroll_delta.y != 0.0 {
            // Standard pan with scroll wheel
             state.timeline_pan_x -= scroll_delta.x;
             state.timeline_pan_y -= scroll_delta.y;
        }

        // Clamp pan to sensible limits
        let visible_track_height = rect.height() - ruler_height;
        let total_content_height = state.scene.len() as f32 * row_height;
        // Allow scrolling only if content is larger than view
        // The max scroll is set so that the bottom of the content aligns with the bottom of the view
        let max_pan_y = (total_content_height - visible_track_height).max(0.0);
        
        if state.timeline_pan_x < 0.0 { state.timeline_pan_x = 0.0; }
        state.timeline_pan_y = state.timeline_pan_y.clamp(0.0, max_pan_y);
        
        let painter = ui.painter_at(rect);
        let font_id = egui::FontId::proportional(10.0);
        
        // --- 1. Draw Ruler (Top Strip) ---
        // let ruler_height = 24.0; // Already defined above
        let ruler_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), ruler_height));
        
        painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(55, 55, 57));
        painter.line_segment(
            [ruler_rect.left_bottom(), ruler_rect.right_bottom()], 
            egui::Stroke::new(1.0, egui::Color32::from_gray(80))
        );

        // Draw time ticks
        let pixels_per_sec = state.timeline_zoom;
        let step_secs = if pixels_per_sec > 200.0 { 0.1 } else if pixels_per_sec > 50.0 { 1.0 } else { 5.0 };
        
        let start_sec = (state.timeline_pan_x / pixels_per_sec).floor() as i32;
        let visible_width = rect.width();
        let end_sec = ((state.timeline_pan_x + visible_width) / pixels_per_sec).ceil() as i32;

        for s in start_sec..=end_sec {
            let sec_val = s as f32 * step_secs;
            if sec_val < 0.0 { continue; }
            
            let x = rect.left() + (sec_val * pixels_per_sec) - state.timeline_pan_x;
            if x < rect.left() { continue; }
            
            // Major tick
            painter.line_segment(
                [egui::pos2(x, ruler_rect.bottom()), egui::pos2(x, ruler_rect.bottom() - 10.0)],
                egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY)
            );

            // Time label
            let time_text = format!("{:02}:{:02}", (sec_val as u32) / 60, (sec_val as u32) % 60);
            painter.text(
                egui::pos2(x + 2.0, ruler_rect.top() + 4.0),
                egui::Align2::LEFT_TOP,
                time_text,
                font_id.clone(),
                egui::Color32::GRAY,
            );
            
            // Subticks
            let subticks = 4;
            for i in 1..subticks {
                let sub_x = x + (pixels_per_sec * step_secs / (subticks as f32)) * (i as f32);
                 if sub_x > rect.right() { break; }
                 painter.line_segment(
                    [egui::pos2(sub_x, ruler_rect.bottom()), egui::pos2(sub_x, ruler_rect.bottom() - 4.0)],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(90))
                );
            }
        }

        // --- 2. Tracks Background Area ---
        let track_area_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() + ruler_height),
            rect.max
        );
        
        // Clip content to track area
        let mut track_painter = ui.painter_at(track_area_rect);
        track_painter.set_clip_rect(track_area_rect); // Enforce clip

        // Example Track Rows (Dynamic based on scene content)
        // let row_height = 24.0; // Already defined
        let start_y = track_area_rect.top() - state.timeline_pan_y;
        
        // Render rows
        for (i, shape) in state.scene.iter().enumerate() {
            let y = start_y + (i as f32 * row_height);
            if y > track_area_rect.bottom() { break; }
            if y + row_height < track_area_rect.top() { continue; }
            
            let is_selected = Some(i) == state.selected;
            let bg_color = if is_selected { egui::Color32::from_rgb(60, 65, 80) } else { egui::Color32::TRANSPARENT };
            
            // Row background
            let row_rect = egui::Rect::from_min_size(
                egui::pos2(track_area_rect.left(), y), 
                egui::vec2(track_area_rect.width(), row_height)
            );
            track_painter.rect_filled(row_rect, 0.0, bg_color);
            track_painter.line_segment(
                [row_rect.left_bottom(), row_rect.right_bottom()], 
                egui::Stroke::new(1.0, egui::Color32::from_gray(50))
            );

            // Label (Left side, maybe sticky?)
            // For now just draw it at scrolling position, maybe sticky later
            let label = match shape {
                Shape::Circle { .. } => format!("Circle #{}", i),
                Shape::Rect { .. } => format!("Rect #{}", i),
                Shape::Group { .. } => format!("Group #{}", i),
            };
            track_painter.text(
                egui::pos2(track_area_rect.left() + 4.0, y + 4.0),
                egui::Align2::LEFT_TOP,
                label,
                font_id.clone(),
                if is_selected { egui::Color32::WHITE } else { egui::Color32::GRAY },
            );

            // Dummy keyframe indicator (just visual for now)
            let kp_x = rect.left() + (1.0 * pixels_per_sec) - state.timeline_pan_x;
            if kp_x > track_area_rect.left() {
                track_painter.circle_filled(egui::pos2(kp_x, y + row_height/2.0), 4.0, egui::Color32::YELLOW);
            }
        }

        // --- 3. Playhead (Scrubber) ---
        // Draw Playhead Line
        let playhead_x = rect.left() + (state.time * pixels_per_sec) - state.timeline_pan_x;
        if playhead_x >= rect.left() && playhead_x <= rect.right() {
             painter.line_segment(
                [egui::pos2(playhead_x, ruler_rect.bottom()), egui::pos2(playhead_x, rect.bottom())],
                 egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 50, 50))
            );
            // Playhead Knob in Ruler
            painter.rect_filled(
                 egui::Rect::from_center_size(egui::pos2(playhead_x, ruler_rect.bottom() - 6.0), egui::vec2(12.0, 12.0)),
                 2.0,
                 egui::Color32::from_rgb(255, 50, 50)
            );
        }

        // Scrubbing Interaction on Ruler
        if response.hovered() && ui.input(|i| i.pointer.primary_down()) {
             if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                // Only scrub if clicking in ruler area or near playhead
                if pos.y <= ruler_rect.bottom() + 10.0 {
                    let new_time = (pos.x - rect.left() + state.timeline_pan_x) / pixels_per_sec;
                    state.time = new_time.max(0.0);
                }
             }
        }

    });
}
