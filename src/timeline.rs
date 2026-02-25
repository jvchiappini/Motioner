use crate::app_state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let frame_fill = egui::Color32::from_rgb(20, 20, 22);

    egui::Frame::none().fill(frame_fill).show(ui, |ui| {
        let (rect, response) =
            ui.allocate_at_least(ui.available_size(), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        let ruler_height = 32.0;
        let row_height = 28.0;
        let gutter_width = 180.0;
        let font_id = egui::FontId::proportional(12.0);
        let small_font = egui::FontId::proportional(10.0);

        // Background for tracks area
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(15, 15, 17));

        // Pan and Zoom logic
        if response.dragged_by(egui::PointerButton::Middle) {
            state.timeline_pan_x -= response.drag_delta().x;
            state.timeline_pan_y -= response.drag_delta().y;
        }

        // --- Ruler ---
        let ruler_rect =
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), ruler_height));
        painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(30, 30, 32));

        // Vertical line separating gutter from tracks
        painter.line_segment(
            [
                egui::pos2(rect.left() + gutter_width, rect.top()),
                egui::pos2(rect.left() + gutter_width, rect.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 48)),
        );

        // Dynamic interval calculation for a clean UI
        let min_label_spacing = 80.0; // Desired minimum pixels between labels
        let intervals = [
            0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
        ];

        let mut major_interval = 1.0;
        for &int in &intervals {
            if state.timeline_zoom * int >= min_label_spacing {
                major_interval = int;
                break;
            }
            major_interval = int;
        }

        let sub_steps = if major_interval >= 60.0 {
            6
        } else if major_interval >= 1.0 {
            10
        } else {
            5
        };
        let minor_interval = major_interval / sub_steps as f32;

        // Clamp pan to prevent negative time view
        state.timeline_pan_x = state.timeline_pan_x.max(0.0);

        let zoom = state.timeline_zoom;
        let pan_x = state.timeline_pan_x;
        let pan_y = state.timeline_pan_y;
        let time_origin_x = rect.left() + gutter_width - pan_x;

        // Time marks
        let start_step = (pan_x / (zoom * major_interval)).floor() as i32;
        let end_step = ((pan_x + rect.width()) / (zoom * major_interval)).ceil() as i32;

        for i in (start_step - 1)..=end_step {
            let t = i as f32 * major_interval;
            if t < 0.0 {
                continue;
            } // Don't show negative time

            let x = time_origin_x + t * zoom;

            if x >= rect.left() + gutter_width && x <= rect.right() {
                // Major tick
                painter.line_segment(
                    [
                        egui::pos2(x, ruler_rect.top() + 18.0),
                        egui::pos2(x, ruler_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
                );

                // Time label
                let label = if major_interval < 0.11 {
                    format!("{:.2}s", t)
                } else if major_interval < 1.0 {
                    format!("{:.1}s", t)
                } else if major_interval < 3600.0 {
                    let minutes = (t as i32) / 60;
                    let seconds = (t as i32) % 60;
                    format!("{:02}:{:02}", minutes, seconds)
                } else {
                    let hours = (t as i32) / 3600;
                    let minutes = ((t as i32) % 3600) / 60;
                    format!("{:02}:{:02}:{:02}", hours, minutes, (t as i32) % 60)
                };

                painter.text(
                    egui::pos2(x + 4.0, ruler_rect.top() + 4.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    small_font.clone(),
                    egui::Color32::from_gray(180),
                );
            }

            // Minor ticks
            for j in 1..sub_steps {
                let sub_t = t + j as f32 * minor_interval;
                if sub_t < 0.0 {
                    continue;
                }

                let sub_x = time_origin_x + sub_t * zoom;
                if sub_x < rect.left() + gutter_width || sub_x > rect.right() {
                    continue;
                }

                let tick_len = if sub_steps > 5 && j == sub_steps / 2 {
                    8.0
                } else {
                    4.0
                };
                painter.line_segment(
                    [
                        egui::pos2(sub_x, ruler_rect.bottom() - tick_len),
                        egui::pos2(sub_x, ruler_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
                );
            }
        }

        // --- Tracks ---
        let tracks_clip_rect =
            egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + ruler_height), rect.max);

        let mut current_y = rect.top() + ruler_height - pan_y;
        for (i, shape) in state.scene.iter().enumerate() {
            let track_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left(), current_y),
                egui::vec2(rect.width(), row_height),
            );

            if track_rect.bottom() > tracks_clip_rect.top()
                && track_rect.top() < tracks_clip_rect.bottom()
            {
                let is_selected = state.selected == Some(i);

                // Track row background highlight
                if is_selected {
                    painter.rect_filled(track_rect, 0.0, egui::Color32::from_rgb(35, 38, 48));
                } else if let Some(pos) = response.hover_pos() {
                    if track_rect.contains(pos) {
                        painter.rect_filled(track_rect, 0.0, egui::Color32::from_rgb(25, 25, 28));
                    }
                }

                // Gutter content
                let gutter_rect =
                    egui::Rect::from_min_size(track_rect.min, egui::vec2(gutter_width, row_height));
                painter.text(
                    gutter_rect.min + egui::vec2(12.0, row_height / 2.0),
                    egui::Align2::LEFT_CENTER,
                    shape.name(),
                    font_id.clone(),
                    if is_selected {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_gray(160)
                    },
                );

                // Track Content (The bar)
                let bar_start = time_origin_x; // Assuming all start at 0 for now
                let bar_end = time_origin_x + state.duration_secs * zoom;

                let clip_min_x = rect.left() + gutter_width;
                let clip_max_x = rect.right();

                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(bar_start.max(clip_min_x), current_y + 6.0),
                    egui::pos2(bar_end.min(clip_max_x), current_y + row_height - 6.0),
                );

                if bar_rect.width() > 0.0 {
                    let bar_color = if is_selected {
                        egui::Color32::from_rgb(100, 200, 255)
                    } else {
                        egui::Color32::from_rgb(100, 200, 255).linear_multiply(0.7)
                    };

                    painter.rect_filled(bar_rect, 6.0, bar_color);

                    // Reflection/Highlight on top
                    let highlight_rect = egui::Rect::from_min_max(
                        bar_rect.min,
                        egui::pos2(bar_rect.right(), bar_rect.top() + bar_rect.height() * 0.4),
                    );
                    painter.rect_filled(highlight_rect, 6.0, egui::Color32::from_white_alpha(30));

                    if is_selected {
                        painter.rect_stroke(
                            bar_rect,
                            6.0,
                            egui::Stroke::new(1.5, egui::Color32::WHITE),
                        );
                    }
                }

                // Horizontal separator
                painter.line_segment(
                    [
                        egui::pos2(rect.left(), track_rect.bottom()),
                        egui::pos2(rect.right(), track_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(35, 35, 38)),
                );
            }
            current_y += row_height;
        }

        // --- Playhead ---
        let ph_x = time_origin_x + state.time * zoom;
        if ph_x >= rect.left() + gutter_width && ph_x <= rect.right() {
            // Main line
            painter.line_segment(
                [
                    egui::pos2(ph_x, rect.top()),
                    egui::pos2(ph_x, rect.bottom()),
                ],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 60, 60)),
            );

            // Head (Square with rounded corners)
            let head_size = 10.0;
            let head_rect = egui::Rect::from_center_size(
                egui::pos2(ph_x, rect.top() + head_size / 2.0 + 2.0),
                egui::vec2(head_size, head_size),
            );
            painter.rect_filled(head_rect, 2.0, egui::Color32::from_rgb(255, 60, 60));
        }

        // --- Interactions ---
        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                // Scrubbing logic
                if pos.y <= rect.top() + ruler_height || response.drag_delta().x.abs() > 2.0 {
                    let new_time = (pos.x - time_origin_x) / zoom;
                    state.set_time(new_time);
                }
            }
        }

        // Support zooming with Ctrl + Wheel
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if response.hovered() && zoom_delta != 1.0 {
            let old_zoom = state.timeline_zoom;
            state.timeline_zoom = (state.timeline_zoom * zoom_delta).clamp(5.0, 5000.0);

            // Zoom towards mouse pointer
            if let Some(mouse_pos) = response.hover_pos() {
                let time_at_mouse = (mouse_pos.x - time_origin_x) / old_zoom;
                let new_time_origin_x = mouse_pos.x - time_at_mouse * state.timeline_zoom;
                state.timeline_pan_x = rect.left() + gutter_width - new_time_origin_x;
            }
        }

        // Normal scroll to pan horizontally (when not holding Ctrl)
        if response.hovered() && !ui.input(|i| i.modifiers.command) {
            let scroll_delta = ui.input(|i| i.raw_scroll_delta);
            let scroll_x = scroll_delta.x + scroll_delta.y;
            if scroll_x != 0.0 {
                state.timeline_pan_x -= scroll_x * 2.0;
            }
        }
    });
}
