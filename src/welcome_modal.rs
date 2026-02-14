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
            
            // NOTE: We rely on the main UI being disabled to prevent click-through,
            // rather than a blocking rect which might steal focus.

            // Inner Window Content
            // We use a specific ID scope to ensure stable IDs for the text edit
            ui.allocate_ui_at_rect(rect, |ui| {
                ui.push_id("welcome_modal_inner", |ui| {
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
                            let text_edit = egui::TextEdit::singleline(&mut state.project_path_input)
                                .id(egui::Id::new("project_path_input"))
                                .hint_text("Select or type a folder path...")
                                .desired_width(ui.available_width() - 90.0)
                                .margin(egui::vec2(8.0, 8.0))
                                .lock_focus(true); // Keep focus here when interacting
                                
                            let response = ui.add(text_edit);
                            
                            // Auto-focus if empty and just opened (optional, maybe later)
                            // if state.show_welcome && state.project_path_input.is_empty() {
                            //     response.request_focus();
                            // }

                            if response.changed() {
                                state.path_validation_error = None; // clear error on edit
                            }

                            if ui.button("📂 Browse").clicked() {
                                // Try to start at the current input path if it exists, otherwise default
                                let start_dir = if !state.project_path_input.is_empty() {
                                    std::path::PathBuf::from(&state.project_path_input)
                                } else {
                                    std::env::current_dir().unwrap_or_default()
                                };

                                // Open Folder Picker
                                let dialog = rfd::FileDialog::new();
                                // attempt to set start directory to help it load faster or more relevantly
                                let dialog = if start_dir.exists() {
                                    dialog.set_directory(&start_dir)
                                } else {
                                    dialog
                                };

                                if let Some(path) = dialog.pick_folder() {
                                    state.project_path_input = path.to_string_lossy().to_string();
                                    state.path_validation_error = None;
                                }
                            }
                        });

                        ui.add_space(30.0);
                        
                        ui.vertical_centered(|ui| {
                            let btn = egui::Button::new(
                                egui::RichText::new("Start Creating  🚀")
                                    .size(16.0)
                                    .strong()
                            )
                            .min_size(egui::vec2(200.0, 40.0))
                            .fill(if !state.project_path_input.is_empty() { egui::Color32::from_rgb(0, 120, 215) } else { egui::Color32::from_gray(60) });

                            if ui.add_enabled(!state.project_path_input.is_empty(), btn).clicked() {
                                let path = std::path::PathBuf::from(&state.project_path_input);
                                if path.exists() && path.is_dir() {
                                    state.project_path = Some(path);
                                    state.show_welcome = false;
                                } else if !path.exists() {
                                     state.path_validation_error = Some("The specified path does not exist.".to_string());
                                } else {
                                     state.path_validation_error = Some("The specified path is not a directory.".to_string());
                                }
                            }
                            
                            if let Some(err) = &state.path_validation_error {
                                ui.add_space(5.0);
                                ui.label(
                                    egui::RichText::new(err)
                                        .size(12.0)
                                        .color(egui::Color32::from_rgb(255, 100, 100))
                                );
                            } else if state.project_path_input.is_empty() {
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
        });
}
