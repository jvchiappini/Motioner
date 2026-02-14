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
    // Faster closing animation (0.2s) vs opening (0.35s)
    let duration = if state.settings_is_closing { 0.2 } else { 0.35 };
    let raw_t = ((now - start_time) as f32 / duration as f32).clamp(0.0, 1.0);

    // If closing, reverse the direction  <-- REMOVE THIS BLOCK (Lines 18-22 in original context)
    // The previous logic used 't' for both directions. Now we handle it separately.
    // So we just delete the block that computed `let t = ...`

    // Check if closing animation is done
    if state.settings_is_closing && raw_t >= 1.0 {
        state.show_settings = false;
        state.settings_open_time = None;
        state.settings_is_closing = false;
        return;
    }

    // Continue requesting repaints until animation completes
    if raw_t < 1.0 {
        ctx.request_repaint();
    }

    // Calculate Animation Parameters
    let (opacity, slide_offset) = if state.settings_is_closing {
        // Closing: "Anticipation" effect (BackIn)
        // Go up slightly (negative offset), then crash down (positive offset)
        // raw_t goes from 0.0 to 1.0 during closing
        let t = raw_t;

        let opacity = (1.0 - t).powi(2); // Fade out quicker at end

        // BackIn Easing: c3 * t^3 - c1 * t^2
        // c1 determines the "overshoot" (how much it goes up)
        let c1 = 2.5;
        let c3 = c1 + 1.0;
        let back_in = c3 * t.powi(3) - c1 * t.powi(2);

        // Target offset is 100px down.
        let offset = 100.0 * back_in;

        (opacity, offset)
    } else {
        // Opening: Smooth Cubic Ease Out
        let t = raw_t;
        let t_eased = 1.0 - (1.0 - t).powi(3);

        let opacity = t_eased;
        // Starts at 100px (down), moves to 0px
        let offset = 100.0 * (1.0 - t_eased);

        (opacity, offset)
    };

    let fade_color = egui::Color32::from_black_alpha((180.0 * opacity) as u8);

    let screen_rect = ctx.input(|i| i.screen_rect());

    // 5. Draw Full Screen Overlay (Modal Backdrop)
    egui::Area::new("settings_overlay")
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            // Draw the dimmed background
            ui.painter().rect_filled(screen_rect, 0.0, fade_color);

            // Close on click outside (optional, but good UX)
            if ui.input(|i| i.pointer.primary_clicked()) {
                // We can check if the click was outside the window later.
                // For now, simpler to rely on the "Close" button or the X.
            }

            // 6. Draw the Settings Window
            let window_width = 460.0;
            let window_height = 420.0; // Estimate
            let center_pos = screen_rect.center();

            // Apply the slide offset to the Y position
            let window_pos = egui::pos2(
                center_pos.x - window_width / 2.0,
                center_pos.y - window_height / 2.0 + slide_offset,
            );

            // We use a nested Area or just manually paint a window-like frame at the calculated rect.
            // Using a Window widget is easier for content, but we want custom positioning and animation.
            // So we'll use a Frame inside the Area.

            egui::Area::new("settings_content_area")
                .fixed_pos(window_pos) // This moves correctly
                .order(egui::Order::Tooltip)
                .show(ctx, |ui| {
                    // Opacity handling for widgets is limited in this version,
                    // relying on background fade and slide animation.

                    // Main Window Frame
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(32, 32, 32))
                        .rounding(12.0)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(30)))
                        .shadow(egui::epaint::Shadow {
                            extrusion: 25.0,
                            color: egui::Color32::from_black_alpha(100),
                        })
                        .inner_margin(24.0)
                        .show(ui, |ui| {
                            ui.set_width(window_width - 48.0); // account for margin
                                                               // ui.set_height(window_height - 48.0); // let it grow naturally

                            render_header(ui, state);
                            ui.add_space(20.0);
                            ui.separator();
                            ui.add_space(20.0);
                            render_body(ui, state);
                            ui.add_space(30.0);
                            render_footer(ui, state);
                        });
                });
        });
}

fn render_header(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.heading(
            egui::RichText::new("Project Settings")
                .size(20.0)
                .strong()
                .color(egui::Color32::WHITE),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close_btn = ui.add(egui::Button::new("❌").frame(false));
            if close_btn.clicked() {
                close_settings(state);
            }
        });
    });
}

fn render_body(ui: &mut egui::Ui, state: &mut AppState) {
    // Use a Grid for nice alignment
    egui::Grid::new("settings_grid")
        .num_columns(2)
        .spacing([40.0, 16.0])
        .striped(false)
        .show(ui, |ui| {
            // Section: Animation
            ui.label(
                egui::RichText::new("Animation")
                    .strong()
                    .color(egui::Color32::from_gray(200)),
            );
            ui.end_row();

            ui.label("Frame Rate (FPS)");
            ui.add(
                egui::DragValue::new(&mut state.fps)
                    .clamp_range(1..=240)
                    .speed(1),
            );
            ui.end_row();

            ui.label("Duration (seconds)");
            ui.add(
                egui::DragValue::new(&mut state.duration_secs)
                    .clamp_range(0.1..=3600.0)
                    .speed(0.1)
                    .suffix(" s"),
            );
            ui.end_row();

            ui.add_space(10.0);
            ui.end_row();

            // Section: Output
            ui.label(
                egui::RichText::new("Output Resolution")
                    .strong()
                    .color(egui::Color32::from_gray(200)),
            );
            ui.end_row();

            ui.label("Dimensions");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut state.render_width).prefix("W: "));
                ui.label("x");
                ui.add(egui::DragValue::new(&mut state.render_height).prefix("H: "));
            });
            ui.end_row();

            ui.label("Presets");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                selectable_res(ui, state, "720p", 1280, 720);
                selectable_res(ui, state, "1080p", 1920, 1080);
                selectable_res(ui, state, "2K", 2560, 1440);
                selectable_res(ui, state, "4K", 3840, 2160);
            });
            ui.end_row();
        });
}

fn selectable_res(ui: &mut egui::Ui, state: &mut AppState, name: &str, w: u32, h: u32) {
    let is_selected = state.render_width == w && state.render_height == h;
    let btn = ui.add_enabled(
        !is_selected,
        egui::Button::new(name).min_size(egui::vec2(40.0, 20.0)),
    );
    if btn.clicked() {
        state.render_width = w;
        state.render_height = h;
    }
}

fn render_footer(ui: &mut egui::Ui, state: &mut AppState) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        let btn_size = egui::vec2(200.0, 32.0);
        if ui
            .add_sized(btn_size, egui::Button::new("Close Settings"))
            .clicked()
        {
            close_settings(state);
        }
    });
}

fn close_settings(state: &mut AppState) {
    if !state.settings_is_closing {
        state.settings_is_closing = true;
        state.settings_open_time = None; // Force show() to reset 'start_time' to 'now'
    }
}
