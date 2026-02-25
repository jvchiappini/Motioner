use crate::app_state::AppState;
use eframe::egui;

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    // 1. Initialize animation timer if needed
    let now = ctx.input(|i| i.time);
    if state.settings_open_time.is_none() {
        state.settings_open_time = Some(now);
    }

    // 2. Calculate animation progress (t)
    let start_time = state.settings_open_time.unwrap();
    let duration = if state.settings_is_closing { 0.25 } else { 0.3 };
    let raw_t = ((now - start_time) as f32 / duration as f32).clamp(0.0, 1.0);

    // Check if closing animation is done
    if state.settings_is_closing && raw_t >= 1.0 {
        state.show_settings = false;
        state.settings_open_time = None;
        state.settings_is_closing = false;
        return;
    }

    if raw_t < 1.0 {
        ctx.request_repaint();
    }

    // Animation Easing
    let display_t = if state.settings_is_closing {
        // Linear fade out or simple ease out
        1.0 - raw_t
    } else {
        // Ease Out Cubic
        1.0 - (1.0 - raw_t).powi(3)
    };

    // Animation visual parameters
    let opacity = display_t;
    let _scale = 0.95 + (0.05 * display_t); // Subtle scale up
    let slide_offset = 20.0 * (1.0 - display_t); // Slight slide up

    let fade_color = egui::Color32::from_black_alpha((200.0 * opacity) as u8);
    let screen_rect = ctx.input(|i| i.screen_rect());
    let center = screen_rect.center();

    // 3. Draw Full Screen Overlay
    egui::Area::new("settings_overlay")
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            // Backdrop
            ui.painter().rect_filled(screen_rect, 0.0, fade_color);

            // Close on click outside (Backdrop interaction)
            // We put a large invisible button or sense click on the background
            let backdrop_response = ui.allocate_rect(screen_rect, egui::Sense::click());
            if backdrop_response.clicked() {
                close_settings(state);
            }
        });

    // 4. Draw the Centered Modal Window
    let window_width = 460.0;
    let window_height = 420.0; // Optimized for two sections

    // Calculate centered position
    // We apply the slide offset to Y for the animation
    let window_pos = egui::pos2(
        center.x - window_width / 2.0,
        center.y - window_height / 2.0 + slide_offset,
    );

    // 5. Render Modal Content
    egui::Area::new("settings_content_area")
        .fixed_pos(window_pos)
        .order(egui::Order::Foreground) // Same layer, but drawn after backdrop naturally due to code order, or use Tooltip to be safe
        .show(ctx, |ui| {
            // Apply scale transform if possible?
            // egui::Area doesn't support transform easily. We simulate scale by just fading and sliding.
            // Scale logic is omitted for simplicity as it requires changing rect size which affects layout.

            // Window Shadow & Frame
            // We use a separate frame for the detailed styling
            let frame = egui::Frame::none()
                .fill(egui::Color32::from_rgb(20, 20, 22)) 
                .rounding(20.0)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(15)))
                .shadow(egui::epaint::Shadow {
                    extrusion: 60.0,
                    color: egui::Color32::from_black_alpha(160),
                });

            frame.show(ui, |ui| {
                ui.set_width(window_width);
                ui.set_height(window_height);

                // Use a vertical layout with spacing
                ui.allocate_ui(egui::vec2(window_width, window_height), |ui| {
                    // Header
                    ui.add_space(20.0);
                    render_header(ui, state);
                    ui.add_space(10.0);
                    ui.separator();

                    // Body (Scrollable)
                    egui::ScrollArea::vertical()
                        .max_height(window_height - 100.0) // Leave room for header/footer
                        .show(ui, |ui| {
                            ui.add_space(20.0);
                            render_body(ui, state);
                            ui.add_space(30.0);
                        });

                    // Footer (Floating at bottom or just end of scroll? Let's keep it clean at bottom of scroll usually, or pinned)
                    // For a true modal, pinned footer is nice, but scrollable is safer for small screens.
                    // We'll put it at the end of scroll for now.
                    render_footer(ui, state);
                    ui.add_space(20.0);
                });
            });
        });
}

fn render_header(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.add_space(24.0); // Left padding
        ui.heading(
            egui::RichText::new("Project Settings")
                .size(20.0)
                .strong()
                .color(egui::Color32::from_white_alpha(240)),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(24.0); // Right padding

            // Modern "X" close button
            let close_btn = ui.add(
                egui::Button::new(egui::RichText::new("✕").size(16.0).color(egui::Color32::from_gray(180)))
                    .frame(false)
                    .fill(egui::Color32::TRANSPARENT),
            );

            if close_btn.clicked() {
                close_settings(state);
            }
            if close_btn.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        });
    });
}

fn render_body(ui: &mut egui::Ui, state: &mut AppState) {
    let accent_color = egui::Color32::from_rgb(100, 150, 255);
    let label_color = egui::Color32::from_gray(170);

    let mut section =
        |ui: &mut egui::Ui, title: &str, content: &dyn Fn(&mut egui::Ui, &mut AppState)| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new(title)
                        .strong()
                        .color(egui::Color32::WHITE)
                        .size(16.0),
                );
            });
            ui.add_space(12.0);

            // Indented content inside a nice rounded frame for each section
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(32, 32, 36))
                .rounding(12.0)
                .inner_margin(egui::Margin::symmetric(20.0, 16.0))
                .show(ui, |ui| {
                    content(ui, state);
                });

            ui.add_space(8.0);
        };

    ui.vertical_centered(|ui| {
        ui.set_width(ui.available_width() - 48.0);

        // 1. Animation Section
        section(ui, "Animation Timeline", &|ui, state| {
            egui::Grid::new("anim_grid")
                .num_columns(2)
                .spacing([40.0, 16.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Frame Rate").color(label_color));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut state.fps)
                                .clamp_range(1..=240)
                                .speed(1),
                        );
                        ui.label(egui::RichText::new("FPS").color(label_color));
                    });
                    ui.end_row();

                    ui.label(egui::RichText::new("Duration").color(label_color));

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut state.duration_input_buffer)
                            .desired_width(80.0),
                    );

                    if response.changed() {
                        if let Ok(val) = state.duration_input_buffer.parse::<f32>() {
                            state.duration_secs = val.clamp(0.1, 3600.0);
                        }
                    } else if !response.has_focus() {
                        let current_str = format!("{:.1}", state.duration_secs);
                        if state.duration_input_buffer != current_str {
                            if let Ok(val) = state.duration_input_buffer.parse::<f32>() {
                                if (val - state.duration_secs).abs() > 0.001 {
                                    state.duration_input_buffer = current_str;
                                }
                            } else {
                                state.duration_input_buffer = current_str;
                            }
                        }
                    }

                    ui.label(egui::RichText::new("sec").color(label_color));
                    ui.end_row();
                });
        });

        ui.add_space(12.0);

        // 2. Output & Resolution
        section(ui, "Output Resolution", &|ui, state| {
            egui::Grid::new("res_grid")
                .num_columns(2)
                .spacing([40.0, 16.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Dimensions").color(label_color));
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut state.render_width).prefix("W: "));
                        ui.label(egui::RichText::new("x").color(label_color));
                        ui.add(egui::DragValue::new(&mut state.render_height).prefix("H: "));
                    });
                    ui.end_row();

                    ui.label(egui::RichText::new("Presets").color(label_color));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let presets = [
                            ("720p", 1280, 720),
                            ("1080p", 1920, 1080),
                            ("2K", 2560, 1440),
                            ("4K", 3840, 2160),
                        ];
                        for (name, w, h) in presets {
                            let is_active = state.render_width == w && state.render_height == h;

                            let mut btn = egui::Button::new(
                                egui::RichText::new(name).size(13.0).color(if is_active {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::from_gray(180)
                                }),
                            )
                            .rounding(6.0);

                            if is_active {
                                btn = btn.fill(accent_color).stroke(egui::Stroke::NONE);
                            } else {
                                btn = btn.fill(egui::Color32::from_white_alpha(10));
                            }

                            if ui.add(btn).clicked() {
                                state.render_width = w;
                                state.render_height = h;
                            }
                        }
                    });
                    ui.end_row();
                });
        });
    });
}

fn render_footer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        let btn = egui::Button::new(
            egui::RichText::new("Done")
                .size(15.0)
                .strong()
                .color(egui::Color32::WHITE),
        )
        .min_size(egui::vec2(160.0, 36.0))
        .fill(egui::Color32::from_rgb(100, 150, 255))
        .rounding(8.0)
        .stroke(egui::Stroke::NONE);

        if ui.add(btn).clicked() {
            close_settings(state);
        }
    });
}

fn close_settings(state: &mut AppState) {
    if !state.settings_is_closing {
        state.settings_is_closing = true;
        state.settings_open_time = None;
    }
}
