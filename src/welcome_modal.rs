use crate::app_state::AppState;
use eframe::egui;

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_welcome {
        return;
    }
    
    // Dimmed background - High opacity for modal focus
    let screen_rect = ctx.input(|i| i.screen_rect());
    let fade_color = egui::Color32::from_black_alpha(220);

    // Fullscreen Overlay
    egui::Area::new("welcome_modal_overlay")
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(true)
        .order(egui::Order::Tooltip) // On top of everything
        .show(ctx, |ui| {
            // Paint Backdrop
            ui.painter().rect_filled(screen_rect, 0.0, fade_color);

            // Calculate Centered Window
            let width = 500.0;
            let height = 280.0;
            let center = screen_rect.center();
            let rect = egui::Rect::from_center_size(center, egui::vec2(width, height));

            // Inner Window Content
            ui.allocate_ui_at_rect(rect, |ui| {
                egui::Frame::window(ui.style())
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .inner_margin(30.0)
                    .rounding(12.0)
                    .shadow(egui::epaint::Shadow { extrusion: 40.0, color: egui::Color32::BLACK })
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(30)))
                    .show(ui, |ui| {
                        
                        ui.vertical_centered(|ui| {
                            ui.add_space(5.0);
                            ui.heading(
                                egui::RichText::new("Motioner")
                                    .size(32.0)
                                    .strong()
                                    .color(egui::Color32::WHITE)
                            );
                            ui.label(egui::RichText::new("Motion Graphics DSL Editor").italics().weak());
                            ui.add_space(25.0);
                        });

                        ui.label(egui::RichText::new("Project Location").strong());
                        ui.add_space(5.0);
                        
                        ui.horizontal(|ui| {
                            // Display current path or placeholder
                            let mut path_display = state.project_path.as_ref()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_default();
                            
                            let text_edit = egui::TextEdit::singleline(&mut path_display)
                                .hint_text("Select a folder to store your project files...")
                                .desired_width(ui.available_width() - 90.0)
                                .margin(egui::vec2(8.0, 8.0));
                                
                            ui.add_enabled(false, text_edit); // Read-only

                            if ui.button("📂 Browse").clicked() {
                                // Open Folder Picker
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    state.project_path = Some(path);
                                }
                            }
                        });

                        ui.add_space(30.0);
                        
                        ui.vertical_centered(|ui| {
                            let has_path = state.project_path.is_some();
                            let btn = egui::Button::new(
                                egui::RichText::new("Start Creating  🚀")
                                    .size(16.0)
                                    .strong()
                            )
                            .min_size(egui::vec2(200.0, 40.0))
                            .fill(if has_path { egui::Color32::from_rgb(0, 120, 215) } else { egui::Color32::from_gray(60) });

                            if ui.add_enabled(has_path, btn).clicked() {
                                state.show_welcome = false;
                            }
                            
                            if !has_path {
                                ui.add_space(5.0);
                                ui.label(
                                    egui::RichText::new("Please select a project folder to continue")
                                        .size(12.0)
                                        .color(egui::Color32::from_rgb(255, 100, 100))
                                );
                            }
                        });
                    });
            });
        });
}
