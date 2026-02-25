use crate::app_state::AppState;
use eframe::egui;

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_welcome {
        return;
    }

    // handle folder picking ...
    if let Some(rx) = &state.folder_dialog_rx {
        if let Ok(path) = rx.try_recv() {
            state.project_path_input = path.to_string_lossy().to_string();
            state.path_validation_error = None;
        }
    }

    let screen_rect = ctx.input(|i| i.screen_rect());
    
    // 1. Backdrop layer: foreground
    // Use Foreground order so the backdrop covers panels instead of being rendered at the same level as the modal
    let backdrop_painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("welcome_backdrop")));
    backdrop_painter.rect_filled(
        screen_rect,
        0.0,
        egui::Color32::from_black_alpha(200),
    );

    // 2. Modal - order Tooltip (always on top of Foreground)
    egui::Area::new("welcome_modal_area")
        .fixed_pos(screen_rect.center())
        .pivot(egui::Align2::CENTER_CENTER)
        .order(egui::Order::Tooltip)
        .interactable(true)
        .show(ctx, |ui| {
            let width = 680.0;
            let height = 460.0;
            
            // Outer glow / shadow effect done via shadow property
            let frame = egui::Frame::none()
                .fill(egui::Color32::from_rgb(18, 18, 22))
                .rounding(24.0)
                .inner_margin(0.0) // No margin here so we can draw custom background to the edge
                .shadow(egui::epaint::Shadow {
                    extrusion: 80.0,
                    color: egui::Color32::from_black_alpha(200),
                })
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(15)));

            // Outer Frame Scope
            frame.show(ui, |ui| {
                ui.set_width(width);
                ui.set_height(height);
                
                // Get the rect for our window to draw animations
                let bg_rect = ui.min_rect();
                let time = ui.input(|i| i.time);
                
                // Draw abstract animated glowing inside the modal background
                // Clip it to the modal bounds so it acts like a frosted glass window
                let abstract_painter = ui.painter().with_clip_rect(bg_rect);
                
                // Orb 1: Violet
                let center1 = bg_rect.center() + egui::vec2((time as f32 * 0.3).sin() * 80.0, -100.0 + (time as f32 * 0.45).cos() * 50.0);
                abstract_painter.circle_filled(
                    center1,
                    200.0,
                    egui::Color32::from_rgba_unmultiplied(140, 60, 255, 12) 
                );

                // Orb 2: Deep Blue
                let center2 = bg_rect.center() + egui::vec2(180.0 + (time as f32 * 0.25).cos() * 60.0, 150.0 + (time as f32 * 0.4).sin() * 70.0);
                abstract_painter.circle_filled(
                    center2,
                    240.0,
                    egui::Color32::from_rgba_unmultiplied(40, 140, 255, 12)
                );

                // Inner content with proper margins
                egui::Frame::none()
                    .inner_margin(egui::Margin {
                        left: 56.0,
                        right: 56.0,
                        top: 48.0,
                        bottom: 32.0,
                    })
                    .show(ui, |ui| {
                        
                        // Content Layout
                        ui.vertical_centered(|ui| {
                            
                            // Logo 
                            let logo_size = 80.0;
                            let logo_scale = 1.0 + (time.sin() as f32 * 0.02); // Subtle breathing
                            
                            // Let's allocate exact space for the image to stay perfectly centered
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(logo_size, logo_size), egui::Sense::hover());
                            let img_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(logo_size, logo_size) * logo_scale);
                            
                            if let Some(handle) = &state.logo_texture {
                                ui.painter().image(
                                    handle.id(),
                                    img_rect,
                                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                    egui::Color32::WHITE
                                );
                            }
                            
                            ui.add_space(24.0);
                            
                            // Title
                            ui.heading(
                                egui::RichText::new("MOTIONER")
                                    .size(42.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(255, 255, 255))
                                    .extra_letter_spacing(4.0)
                            );
                            
                            ui.add_space(6.0);
                            
                            // Subtitle
                            ui.label(
                                egui::RichText::new("The ultimate motion graphics DSL editor")
                                    .size(16.0)
                                    .color(egui::Color32::from_rgb(170, 170, 190))
                            );
                            
                            ui.add_space(48.0);
                            
                            // Form Group Background
                            let group_frame = egui::Frame::none()
                                .fill(egui::Color32::from_black_alpha(100))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(15)))
                                .rounding(16.0)
                                .inner_margin(egui::Margin::symmetric(24.0, 20.0));
                                
                            group_frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("PROJECT WORKSPACE")
                                            .size(11.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(140, 160, 255))
                                            .extra_letter_spacing(1.0)
                                    );
                                });
                                
                                ui.add_space(12.0);
                                
                                ui.horizontal(|ui| {
                                    let btn_rect = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new("📁 Browse")
                                                .strong()
                                                .size(14.0)
                                        )
                                        .min_size(egui::vec2(110.0, 42.0))
                                        .rounding(10.0)
                                        .fill(egui::Color32::from_white_alpha(12))
                                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(20)))
                                    );
                                    
                                    if btn_rect.clicked() {
                                        let start_dir = if !state.project_path_input.is_empty() {
                                            std::path::PathBuf::from(&state.project_path_input)
                                        } else {
                                            std::env::current_dir().unwrap_or_default()
                                        };

                                        if let Some(tx) = state.folder_dialog_tx.clone() {
                                            std::thread::spawn(move || {
                                                if let Some(path) = rfd::FileDialog::new().set_directory(&start_dir).pick_folder() {
                                                    let _ = tx.send(path);
                                                }
                                            });
                                        }
                                    }

                                    ui.add_space(10.0);

                                    // Custom input field
                                    ui.style_mut().visuals.extreme_bg_color = egui::Color32::from_black_alpha(120);
                                    ui.style_mut().visuals.widgets.inactive.rounding = 10.0.into();
                                    ui.style_mut().visuals.widgets.hovered.rounding = 10.0.into();
                                    ui.style_mut().visuals.widgets.active.rounding = 10.0.into();
                                    ui.style_mut().visuals.selection.stroke.color = egui::Color32::from_rgb(140, 160, 255);
                                    
                                    let text_edit = egui::TextEdit::singleline(&mut state.project_path_input)
                                        .hint_text("Choose a folder to start creating...")
                                        .desired_width(ui.available_width())
                                        .margin(egui::vec2(16.0, 12.0))
                                        .text_color(egui::Color32::WHITE);
                                    
                                    let response = ui.add(text_edit);
                                    if response.changed() {
                                        state.path_validation_error = None;
                                    }
                                });
                            });
                            
                            ui.add_space(36.0);
                            
                            // Launch Button
                            let is_ready = !state.project_path_input.is_empty();
                            let btn_color = if is_ready {
                                egui::Color32::from_rgb(95, 65, 255)
                            } else {
                                egui::Color32::from_rgb(45, 45, 52)
                            };
                            
                            let launch_btn = egui::Button::new(
                                egui::RichText::new("LAUNCH ENGINE")
                                    .size(16.0)
                                    .strong()
                                    .color(if is_ready { egui::Color32::WHITE } else { egui::Color32::from_gray(120) })
                                    .extra_letter_spacing(1.5)
                            )
                            .min_size(egui::vec2(ui.available_width(), 54.0)) // FIX here: no more u32::MAX
                            .rounding(12.0)
                            .fill(btn_color);
                            
                            if ui.add_enabled(is_ready, launch_btn).clicked() {
                                let path = std::path::PathBuf::from(&state.project_path_input);
                                if path.exists() && path.is_dir() {
                                    state.project_path = Some(path);
                                    state.refresh_fonts_async();
                                    state.show_welcome = false;
                                } else if !path.exists() {
                                    state.path_validation_error = Some("Path does not exist.".into());
                                } else {
                                    state.path_validation_error = Some("Not a directory.".into());
                                }
                            }
                            
                            if let Some(err) = &state.path_validation_error {
                                ui.add_space(14.0);
                                ui.label(egui::RichText::new(format!("⚠ {}", err)).color(egui::Color32::from_rgb(255, 120, 120)));
                            }
                        });
                        
                        // Bottom text expanding to the bottom margin
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new("v0.1.0-alpha • Made for Creators")
                                    .size(11.0)
                                    .color(egui::Color32::from_gray(100))
                            );
                        });
                    });
            });
        });
}

