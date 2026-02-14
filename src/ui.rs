use crate::app_state::{AppState, PanelTab};
use crate::{code_panel, dsl, scene_graph, timeline};
use eframe::egui;

pub struct MyApp {
    state: AppState,
}

pub fn create_app() -> MyApp {
    MyApp {
        state: AppState::default(),
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = &mut self.state;

        // Auto-sync Code if settings changed while Code tab is active
        let current_settings = (state.fps, state.duration_secs, state.render_width, state.render_height);
        if state.active_tab == Some(PanelTab::Code) && state.last_synced_settings != current_settings {
             state.dsl_code = dsl::generate_dsl(
                 &state.scene, 
                 state.render_width, 
                 state.render_height, 
                 state.fps, 
                 state.duration_secs
             );
             state.last_synced_settings = current_settings;
        } else if state.active_tab != Some(PanelTab::Code) {
             state.last_synced_settings = current_settings;
        }

        // Throttle system stats update (e.g. every 1.0s)
        let now = ctx.input(|i| i.time);
        if now - (state.last_update as f64) > 1.0 {
            state.system.refresh_process(state.pid);
            state.last_update = now as f32;
        }

        // 1. Toolbar Strip (Far Left)
        egui::SidePanel::left("toolbar_panel")
            .resizable(false)
            .exact_width(32.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui
                        .add(egui::Button::new("⚙").frame(false))
                        .on_hover_text("Settings")
                        .clicked()
                    {
                        state.show_settings = !state.show_settings;
                        if state.show_settings {
                            // Reset animation state when opening
                            state.settings_open_time = None;
                            state.settings_is_closing = false;
                        }
                    }
                    ui.add_space(12.0);

                    // Scene Graph Toggle
                    let scene_btn = egui::Button::new("☰").frame(false);
                    if ui.add(scene_btn).on_hover_text("Scene Graph").clicked() {
                        let target = PanelTab::SceneGraph;
                        if state.active_tab == Some(target) {
                            // Close
                            state.active_tab = None;
                        } else {
                            // Open or Switch
                            if state.active_tab.is_some() {
                                // Switching
                                state.transition_source_tab = state.active_tab;
                                state.tab_switch_time = Some(ui.input(|i| i.time));
                            } else {
                                state.tab_switch_time = None;
                            }
                            state.active_tab = Some(target);
                            state.last_active_tab = target;
                        }
                    }
                    ui.add_space(8.0);

                    // Code / DSL Toggle
                    let code_btn = egui::Button::new("{ }").frame(false);
                    if ui.add(code_btn).on_hover_text("Generate Code").clicked() {
                        let target = PanelTab::Code;

                        // Update DSL code always if switching TO code
                        if state.active_tab != Some(target) {
                            state.dsl_code = dsl::generate_dsl(
                                &state.scene,
                                state.render_width,
                                state.render_height,
                                state.fps,
                                state.duration_secs,
                            );
                        }

                        if state.active_tab == Some(target) {
                            // Close
                            state.active_tab = None;
                        } else {
                            // Open or Switch
                            if state.active_tab.is_some() {
                                // Switching
                                state.transition_source_tab = state.active_tab;
                                state.tab_switch_time = Some(ui.input(|i| i.time));
                            } else {
                                state.tab_switch_time = None;
                            }
                            state.active_tab = Some(target);
                            state.last_active_tab = target;
                        }
                    }
                });

                // Bottom indicators (Resource Usage)
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);

                    let mut ram_mb = 0.0;
                    let mut cpu_usage = 0.0;

                    if let Some(process) = state.system.process(state.pid) {
                        ram_mb = process.memory() as f32 / 1024.0 / 1024.0;
                        cpu_usage = process.cpu_usage();
                    }

                    ui.label(
                        egui::RichText::new("N/A")
                            .size(9.0)
                            .weak()
                            .color(egui::Color32::from_rgb(150, 200, 150)),
                    );
                    ui.label(egui::RichText::new("GPU").size(7.0).weak());
                    ui.add_space(4.0);

                    ui.label(
                        egui::RichText::new(format!("{:.1}%", cpu_usage))
                            .size(9.0)
                            .weak()
                            .color(egui::Color32::from_rgb(150, 180, 220)),
                    );
                    ui.label(egui::RichText::new("App CPU").size(7.0).weak());
                    ui.add_space(4.0);

                    ui.label(
                        egui::RichText::new(format!("{:.1} MB", ram_mb))
                            .size(9.0)
                            .weak()
                            .color(egui::Color32::from_rgb(220, 180, 150)),
                    );
                    ui.label(egui::RichText::new("App RAM").size(7.0).weak());
                });
            });

        // 2. Timeline (Bottom)
        egui::TopBottomPanel::bottom("timeline_panel")
            .resizable(true)
            .min_height(120.0)
            .default_height(200.0)
            .show(ctx, |ui| {
                timeline::show(ui, state);
            });

        // 3. Central Area (Multifunction + Canvas)

        // Multifunction Panel (Animated)
        // Since we've already added SidePanel::left("toolbar"), adding another SidePanel::left("multifunction")
        // WILL behave correctly (stacking left-to-right).

        let panel_open = state.active_tab.is_some();
        let t = ctx.animate_bool("multifunction_panel_anim".into(), panel_open);

        // Capture side panel rect for fullscreen animation
        let mut side_panel_rect = egui::Rect::NOTHING;

        if t > 0.0 {
            // Determine which tab content to show.
            // If active_tab is None (closing), use last_active_tab.
            let tab_to_show = state.active_tab.unwrap_or(state.last_active_tab);

            // Special Case: If fullscreen code is active (or animating), we might need to hide this panel
            // OR we render an empty panel to reserve space? 
            // Better: We Always render the SidePanel, but if Fullscreen is active, we render the content 
            // inside the Overlay, effectively stealing it.
            
            let is_fullscreen = state.code_fullscreen && tab_to_show == PanelTab::Code;
            
            // Manual Animation Logic for Slower Transition
            // Opening: 0.8s, Closing: 0.4s (faster)
            let open_duration = 0.8;
            let close_duration = 0.4;

            let dt = ctx.input(|i| i.stable_dt);
            if is_fullscreen {
                if state.code_anim_t < 1.0 {
                    state.code_anim_t += dt / open_duration;
                    if state.code_anim_t > 1.0 { state.code_anim_t = 1.0; }
                    ctx.request_repaint(); // Keep animating
                }
            } else {
                if state.code_anim_t > 0.0 {
                    state.code_anim_t -= dt / close_duration;
                    if state.code_anim_t < 0.0 { state.code_anim_t = 0.0; }
                    ctx.request_repaint(); // Keep animating
                }
            }
            let fs_t = state.code_anim_t;

            let mut panel = egui::SidePanel::left("multifunction_panel")
                .resizable(true)
                .default_width(250.0);

            // If animating (opening or closing), force the width with elastic effect
            if t < 1.0 {
                // ...existing code...
                // Elastic / BackOut Easing
                // This creates the "Overshoot" when opening and "Anticipation" when closing
                let c1 = 1.7;
                let c3 = c1 + 1.0;
                let ease_t = 1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2);

                // Allow width to go slightly larger than 250.0 (overshoot)
                let width = 250.0 * ease_t;
                // Ensure it doesn't go negative
                let width = width.max(0.0);

                panel = panel.exact_width(width).resizable(false);
            }

            panel.show(ctx, |ui| {
                // While animating, we might want to clip content so it doesn't overflow/squish weirdly
                ui.set_clip_rect(ui.max_rect());
                side_panel_rect = ui.min_rect(); // approximate the panel area

                // Base opacity for panel open/close
                ui.visuals_mut().widgets.noninteractive.weak_bg_fill =
                    egui::Color32::from_black_alpha((255.0 * t) as u8);

                // Handle Tab Switching Animation inside the panel
                let now = ui.input(|i| i.time);
                let switch_duration = 0.25;
                let switch_t = if let Some(stime) = state.tab_switch_time {
                    ((now - stime) as f32 / switch_duration).clamp(0.0, 1.0)
                } else {
                    1.0
                };

                // If NOT fully fullscreen, we render here
                if fs_t < 1.0 {
                     if switch_t < 1.0 {
                        // Animating Switch
                        ui.ctx().request_repaint();

                        // Allocate the entire space so the panel doesn't shrink
                        let panel_rect = ui.available_rect_before_wrap();
                        ui.allocate_rect(panel_rect, egui::Sense::hover());

                        // Easing for slide
                        let ease_switch = 1.0 - (1.0 - switch_t).powi(2);

                        // Old Tab (Source) - Slide Out to Left
                        // New Tab (Target) - Slide In from Right
                        // Or if we want a "Push" effect:
                        // Old moves 0 -> -Width
                        // New moves +Width -> 0

                        let width = panel_rect.width();
                        let _height = panel_rect.height();
                        // Use panel_rect for child UIs to match exact allocated space
                        let rect = panel_rect;

                        // Render Old Tab
                        if let Some(source) = state.transition_source_tab {
                            let old_offset = -width * ease_switch;
                            let old_rect = rect.translate(egui::vec2(old_offset, 0.0));

                            let mut child_ui = ui.child_ui(old_rect, *ui.layout());
                            // child_ui.multiply_opacity(1.0 - ease_switch); // Available in newer egui, but maybe not 0.26 or exposed differently 
                            // Just use visual transparency
                            child_ui.visuals_mut().widgets.noninteractive.weak_bg_fill = 
                                egui::Color32::from_black_alpha(((1.0 - ease_switch) * 255.0) as u8);

                            render_tab_content(&mut child_ui, source, state);
                        }

                        // Render New Tab
                        // Note: Because we use child_ui with translate, they don't block each other's layout space if we are careful,
                        // but normally in egui immediate mode, drawing widgets advances the cursor.
                        // Ideally we render them in overlapping layers.

                        let new_offset = width * (1.0 - ease_switch);
                        let new_rect = rect.translate(egui::vec2(new_offset, 0.0));

                        // We need to create a UI that thinks it is at new_rect but doesn't interact with the previous one layout-wise.
                        // Using allocate_ui_at_rect usually pushes the parent cursor.
                        // Since we are inside a SidePanel, we can just overlay.

                        // Force a new layer/clip for the second one?
                        // Simplest hack: Just draw them. 'child_ui' creates a nested UI region.

                        let mut child_ui = ui.child_ui(new_rect, *ui.layout());
                        // child_ui.set_clip_rect(rect); // Clip to parent so it slides in
                        // child_ui.set_opacity(ease_switch); // Fade in - use multiply_opacity or color tint
                        // This is 0.26, set_opacity doesn't exist.
                        // We rely on the movement mainly.

                        render_tab_content(&mut child_ui, tab_to_show, state);
                    } else {
                        // Standard Static Render
                        // If fully minimized, render normally. If animating, render normally (will be overlaid)
                        if fs_t <= 0.0 {
                            render_tab_content(ui, tab_to_show, state);
                        } else {
                             // Placeholder to keep layout size
                             ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
                        }
                    }
                }
            });

            // 4. Handle Fullscreen Animation / Overlay
            if fs_t > 0.0 {
                let screen_rect = ctx.screen_rect();
                
                // Calculate interpolated rect
                // Start: side_panel_rect (where it was) or a default if not captured yet
                let start_rect = if side_panel_rect.width() > 0.0 {
                     // Ensure it is not zero.
                     side_panel_rect
                } else {
                     // Fallback if panel wasn't rendered yet (edge case)
                     egui::Rect::from_min_size(egui::pos2(32.0, 0.0), egui::vec2(250.0, screen_rect.height()))
                };

                // Easing (BackOut or Elastic for expansion)
                let t_anim = fs_t; // 0.0 to 1.0
                
                // BackOut: overshoot slightly then settle
                // c1 = 1.70158
                // c3 = c1 + 1
                // 1 + c3 * (t-1)^3 + c1 * (t-1)^2
                // Or maybe simple CubicOut/QuintOut for smoothness
                // let ease = 1.0 - (1.0 - t_anim).powi(4); // QuartOut

                // Custom EaseInOut for slower feel
                // Sigmoid-like or Parametric Blend
                let sqt = t_anim * t_anim;
                let ease = sqt / (2.0 * (sqt - t_anim) + 1.0);
                
                // OR nice Cubic Out
                let ease = 1.0 - (1.0 - t_anim).powi(3);

                let current_rect = egui::Rect::from_min_max(
                    egui::pos2(
                        start_rect.left() + (screen_rect.left() - start_rect.left()) * ease,
                        start_rect.top() + (screen_rect.top() - start_rect.top()) * ease,
                    ),
                    egui::pos2(
                        start_rect.right() + (screen_rect.right() - start_rect.right()) * ease,
                        start_rect.bottom() + (screen_rect.bottom() - start_rect.bottom()) * ease,
                    ),
                );

                egui::Area::new("fullscreen_code_overlay")
                    .fixed_pos(current_rect.min)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        // Background
                        egui::Frame::window(ui.style())
                             .fill(egui::Color32::from_rgb(30, 30, 30))
                             .show(ui, |ui| {
                                 ui.set_min_size(current_rect.size());
                                 ui.set_max_size(current_rect.size());
                                 
                                 // We simply call render_tab_content here
                                 // It will render the header and the code editor
                                 // The code editor automatically takes available space
                                 
                                 // We need to make sure we render PanelTab::Code specifically
                                 render_tab_content(ui, PanelTab::Code, state);
                             });
                    });
            }
        }
        // Canvas (Central Panel takes remaining space)
        egui::CentralPanel::default().show(ctx, |ui| {
            // Use bottom_up layout to place tools at the bottom
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                // Tools (Bottom)
                ui.horizontal(|ui| {
                    if ui
                        .button(if state.playing {
                            "⏸ Pause"
                        } else {
                            "▶ Play"
                        })
                        .clicked()
                    {
                        state.playing = !state.playing;
                    }
                    if ui.button("⏹ Reset").clicked() {
                        state.playing = false;
                        state.time = 0.0;
                    }
                    ui.label(format!("Time: {:.2}s", state.time));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Export DSL").clicked() {
                            // rfd is not in dependencies yet
                            // if let Some(path) = rfd::FileDialog::new().save_file() {
                            //    let _ = std::fs::write(path, &state.dsl_code);
                            // }
                            println!("Export DSL clicked (File dialog requires 'rfd' crate)");
                        }
                    });
                });

                ui.separator();

                // The Canvas (Fills the rest of the vertical space upwards)
                egui::Frame::canvas(ui.style()).show(ui, |ui| {
                    let (rect, _resp) =
                        ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());

                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 0.0, egui::Color32::BLACK); // Canvas bg

                    // Draw objects
                    for (i, shape) in state.scene.iter().enumerate() {
                        let is_selected = Some(i) == state.selected;
                        let stroke = if is_selected {
                            egui::Stroke::new(2.0, egui::Color32::YELLOW)
                        } else {
                            egui::Stroke::NONE
                        };

                        match shape {
                            crate::scene::Shape::Circle {
                                x,
                                y,
                                radius,
                                color,
                            } => {
                                painter.circle(
                                    egui::pos2(rect.left() + *x, rect.top() + *y),
                                    *radius,
                                    egui::Color32::from_rgb(color[0], color[1], color[2]),
                                    stroke,
                                );
                            }
                            crate::scene::Shape::Rect { x, y, w, h, color } => {
                                painter.rect(
                                    egui::Rect::from_min_size(
                                        egui::pos2(rect.left() + *x, rect.top() + *y),
                                        egui::vec2(*w, *h),
                                    ),
                                    0.0,
                                    egui::Color32::from_rgb(color[0], color[1], color[2]),
                                    stroke,
                                );
                            }
                        }
                    }

                    // Handle Canvas Interaction
                    if ui.input(|i| i.pointer.primary_clicked()) {
                        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                            let mut hit = None;
                            for (i, shape) in state.scene.iter().enumerate() {
                                match shape {
                                    crate::scene::Shape::Circle { x, y, radius, .. } => {
                                        let center = egui::pos2(rect.left() + *x, rect.top() + *y);
                                        if pos.distance(center) <= *radius {
                                            hit = Some(i);
                                        }
                                    }
                                    crate::scene::Shape::Rect { x, y, w, h, .. } => {
                                        let min = egui::pos2(rect.left() + *x, rect.top() + *y);
                                        let max = min + egui::vec2(*w, *h);
                                        if pos.x >= min.x
                                            && pos.x <= max.x
                                            && pos.y >= min.y
                                            && pos.y <= max.y
                                        {
                                            hit = Some(i);
                                        }
                                    }
                                }
                            }
                            state.selected = hit;
                        }
                    }
                });
            });
        });

        // Frame update for animation
        if state.playing {
            state.time += ctx.input(|i| i.stable_dt);
            if state.time > state.duration_secs {
                state.time = 0.0; // Loop
            }
            ctx.request_repaint();
        }

        if state.show_settings {
            crate::project_settings::show(ctx, state);
        }

        // Welcome/Startup Modal
        crate::welcome_modal::show(ctx, state);

        // Toast Notification
        if let Some(msg) = &state.toast_message {
            let now = ctx.input(|i| i.time);
            if now > state.toast_deadline {
                // Clear toast
                state.toast_message = None;
            } else {
                let bg_color = match state.toast_type {
                    crate::app_state::ToastType::Error => egui::Color32::from_rgb(200, 50, 50),
                    crate::app_state::ToastType::Success => egui::Color32::from_rgb(50, 150, 50),
                    _ => egui::Color32::from_gray(80),
                };

                egui::Area::new("toast_notification")
                    .order(egui::Order::Tooltip)
                    .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -60.0))
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(bg_color)
                            .rounding(8.0)
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(50)))
                            .inner_margin(12.0)
                            .shadow(egui::epaint::Shadow::small_dark())
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(msg)
                                        .color(egui::Color32::WHITE)
                                        .size(16.0),
                                );
                            });
                    });
            }
        }
    }
}

fn render_tab_content(ui: &mut egui::Ui, tab: PanelTab, state: &mut AppState) {
    match tab {
        PanelTab::SceneGraph => {
            ui.heading("Scene Graph");
            ui.separator();
            scene_graph::show(ui, state);
        }
        PanelTab::Code => {
            // ui.heading("Generated Code"); // Header inside the panel now?
            // User requested "En la ventana generated code debemos poder editar el codigo..."
            // The header "Generated Code" is outside the code_panel::show.
            // code_panel::show draws the save button row.
            ui.heading("Generated Code");
            ui.separator();
            code_panel::show(ui, state);
        }
    }
}
