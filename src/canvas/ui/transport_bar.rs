use crate::app_state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState, canvas_rect: egui::Rect) {
    let bar_width = 420.0;
    let bar_height = 48.0;

    // Use egui::Area for a truly floating, clickable element that stays in front
    let area = egui::Area::new("transport_area_id")
        .order(egui::Order::Foreground)
        .interactable(true);

    // Initial position or current one
    let mut pos = state.transport_pos.unwrap_or_else(|| {
        egui::pos2(
            canvas_rect.center().x - bar_width / 2.0,
            canvas_rect.bottom() - bar_height - 25.0
        )
    });

    // Clamp strictly within the canvas
    pos.x = pos.x.clamp(canvas_rect.left() + 10.0, canvas_rect.right() - bar_width - 10.0);
    pos.y = pos.y.clamp(canvas_rect.top() + 10.0, canvas_rect.bottom() - bar_height - 10.0);

    area.fixed_pos(pos).show(ui.ctx(), |ui| {
        // Frame for the glassmorphism pill effect
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(18, 18, 20)) // Slightly darker for premium feel
            .rounding(24.0)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(35)))
            .shadow(egui::epaint::Shadow {
                extrusion: 15.0,
                color: egui::Color32::from_black_alpha(160),
            })
            .inner_margin(egui::Margin::symmetric(16.0, 0.0))
            .show(ui, |ui| {
                ui.set_height(bar_height);
                ui.set_width(bar_width);

                // Use a centered horizontal layout to ensure all items are on the same line
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;

                    // Drag Handle
                    let (handle_rect, handle_resp) = ui.allocate_exact_size(egui::vec2(24.0, bar_height), egui::Sense::drag());
                    if handle_resp.dragged() {
                        let delta = ui.input(|i| i.pointer.delta());
                        state.transport_pos = Some(pos + delta);
                    }
                    
                    // Grip visual - perfectly centered
                    let painter = ui.painter();
                    let gx = handle_rect.center().x;
                    let gy = handle_rect.center().y;
                    for i in 0..3 {
                        let y_off = (i as f32 - 1.0) * 5.0;
                        painter.circle_filled(egui::pos2(gx - 2.5, gy + y_off), 1.2, egui::Color32::from_gray(80));
                        painter.circle_filled(egui::pos2(gx + 2.5, gy + y_off), 1.2, egui::Color32::from_gray(80));
                    }

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Transport controls
                    if transport_btn(ui, "⏮", "Reset").clicked() {
                        state.set_time(0.0);
                        state.playing = false;
                    }
                    
                    if transport_btn(ui, "⏪", "Prev Frame").clicked() {
                        state.step_backward();
                        state.playing = false;
                    }

                    // Play/Pause Center Pill - Perfectly circular and centered
                    let (icon, color, bg) = if state.playing {
                        ("⏸", egui::Color32::WHITE, egui::Color32::from_rgb(210, 50, 50))
                    } else {
                        ("▶", egui::Color32::WHITE, egui::Color32::from_rgb(45, 190, 100))
                    };

                    let (play_rect, play_resp) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::click());
                    let p_bg = if play_resp.hovered() { bg.linear_multiply(1.15) } else { bg };
                    
                    ui.painter().circle_filled(play_rect.center(), 18.0, p_bg);
                    
                    // Play icon alignment fix
                    let icon_pos = play_rect.center();
                    // Some icons have slightly off-center visual weights (like the play triangle)
                    let visual_offset = if !state.playing { egui::vec2(1.5, 0.0) } else { egui::vec2(0.0, 0.0) };
                    
                    ui.painter().text(
                        icon_pos + visual_offset,
                        egui::Align2::CENTER_CENTER,
                        icon,
                        egui::FontId::proportional(20.0),
                        color
                    );

                    if play_resp.clicked() {
                        state.playing = !state.playing;
                    }

                    if transport_btn(ui, "⏩", "Next Frame").clicked() {
                        state.step_forward();
                        state.playing = false;
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Time Display - Vertically centered by the layout
                    ui.add(egui::Label::new(
                        egui::RichText::new(format!("{:05.2}s", state.time))
                            .color(egui::Color32::WHITE)
                            .monospace()
                            .size(16.0)
                    ));

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Export Button
                    let export_btn = egui::Button::new(egui::RichText::new("Export").strong())
                        .fill(egui::Color32::from_white_alpha(15))
                        .rounding(8.0);
                    
                    if ui.add(export_btn).clicked() {
                        // Export logic
                    }
                });
            });
    });
}

fn transport_btn(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    let button = egui::Button::new(egui::RichText::new(icon).size(18.0))
        .frame(false)
        .fill(egui::Color32::TRANSPARENT);
    
    ui.add(button).on_hover_text(tooltip)
}
