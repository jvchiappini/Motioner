use crate::app_state::{AppState, PanelTab};
use crate::{canvas, code_panel, dsl, scene_graph, timeline};
use eframe::egui;

pub struct MyApp {
    state: AppState,
}

pub fn create_app(_cc: &eframe::CreationContext<'_>) -> MyApp {
    let mut state = AppState::default();

    // Autocomplete worker is a reusable bit of logic; the helper ensures it's
    // only spawned once and the channels are stored in state.
    state.ensure_completion_worker();

    // perform additional initialisation (vram detection, texture loading etc)
    state.initialize_with_context(_cc);

    // Check if we have wgpu access
    #[cfg(feature = "wgpu")]
    if let Some(render_state) = &_cc.wgpu_render_state {
        state.preview_worker_use_gpu = true;
        state.wgpu_render_state = Some(render_state.clone());

        let device = &render_state.device;
        let target_format = render_state.target_format;

        // We'll insert our custom resources into the callback_resources map
        use crate::canvas::GpuResources;
        let mut renderer = render_state.renderer.write();
        renderer
            .callback_resources
            .insert(GpuResources::new(device, target_format));
    }

    MyApp { state }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = &mut self.state;

        // ensure all fonts referenced by the scene are registered with egui
        state.load_scene_fonts(ctx);

        // collect async font refresh results if available
        if let Some(rx) = &state.font_refresh_rx {
            if let Ok((names, map)) = rx.try_recv() {
                state.available_fonts = names;
                state.font_map = map;
            }
        }
        // --- Global Color Picker Window (Top-level, unconstrained) ---
        if let Some(mut data) = state.color_picker_data.clone() {
            let mut open = true;
            let mut changed = false;
            egui::Window::new("Color Picker")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    let alpha = if data.is_alpha {
                        egui::color_picker::Alpha::BlendOrAdditive
                    } else {
                        egui::color_picker::Alpha::Opaque
                    };

                    let mut color = egui::Color32::from_rgba_unmultiplied(
                        data.color[0],
                        data.color[1],
                        data.color[2],
                        data.color[3],
                    );
                    if egui::color_picker::color_picker_color32(ui, &mut color, alpha) {
                        data.color = color.to_srgba_unmultiplied();
                        changed = true;
                    }
                });

            if !open {
                state.color_picker_data = None;
            } else if changed {
                let new_hex = crate::code_panel::utils::format_hex(data.color, data.is_alpha);
                // Safety check: ensure range is still valid (text might have changed)
                if data.range.end <= state.dsl_code.len() {
                    state.dsl_code.replace_range(data.range.clone(), &new_hex);
                    // update autosave state
                    let now_time = ctx.input(|i| i.time);
                    state.autosave.mark_dirty(now_time);
                    // Update range length if it changed
                    data.range.end = data.range.start + new_hex.len();
                    state.color_picker_data = Some(data);
                } else {
                    // Text changed out from under us, close picker to avoid panic
                    state.color_picker_data = None;
                }
            }
        }

        // Auto-sync Code if settings changed while Code tab is active
        let current_settings = (
            state.fps,
            state.duration_secs,
            state.render_width,
            state.render_height,
        );
        if state.active_tab == Some(PanelTab::Code)
            && state.last_synced_settings != current_settings
        {
            state.dsl_code = dsl::generate_dsl_from_elements(
                &state.scene,
                state.render_width,
                state.render_height,
                state.fps,
                state.duration_secs,
            );
            state.last_synced_settings = current_settings;
        } else if state.active_tab != Some(PanelTab::Code) {
            state.last_synced_settings = current_settings;
        }

        // compute current time once and drive state updates.  Previously the
        // autosave/parse calls lived directly in `ui::update`, but that made
        // the UI module responsible for non-UI business logic.  We now expose
        // a single helper on `AppState` so the rendering code can remain
        // lightweight and a caller in tests can tick the state without
        // depending on egui.
        let now = ctx.input(|i| i.time);
        let _ = state.tick(now);

        // If a preview request was deferred while editing code, trigger it once
        // the editor has been idle long enough. This keeps immediate UI feedback
        // via live composition while avoiding frequent texture swaps.
        if state.preview_pending_from_code {
            if let Some(last_edit) = state.autosave.last_edit_time {
                const CODE_PREVIEW_IDLE_SECS: f64 = 0.45;
                if now - last_edit > CODE_PREVIEW_IDLE_SECS {
                    crate::canvas::request_preview_frames(state, state.time);
                    state.preview_pending_from_code = false;
                }
            } else {
                // safety: no last_edit timestamp — request immediately
                crate::canvas::request_preview_frames(state, state.time);
                state.preview_pending_from_code = false;
            }
        }
        if now - (state.last_update as f64) > 1.0 {
            state.system.refresh_process(state.pid);
            state.last_update = now as f32;
        }

        // Define main UI enabled state based on modal visibility
        let main_ui_enabled = !state.show_welcome;

        // Render Element Modifiers as a top-level modal (global — not constrained to CentralPanel)
        if state.modifier_active_path.is_some() {
            crate::modals::element_modifiers::show(ctx, state);
        }

        // Render Animations modal as top-level/global (remembered position, Esc/click-outside)
        if state.show_animations_modal {
            crate::modals::animations::show(ctx, state);
        }

        // Export modal (blocks all interaction behind it)
        if state.show_export_modal {
            crate::modals::export::show(ctx, state);
        }

        // 1. Toolbar Strip (Far Left)
        egui::SidePanel::left("toolbar_panel")
            .resizable(false)
            .exact_width(32.0)
            .show(ctx, |ui| {
                ui.set_enabled(main_ui_enabled); // Disable if modal is open

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
                            state.dsl_code = dsl::generate_dsl_from_elements(
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
                ui.set_enabled(main_ui_enabled);
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
                    if state.code_anim_t > 1.0 {
                        state.code_anim_t = 1.0;
                    }
                    ctx.request_repaint(); // Ensure fluid animation
                }
            } else {
                if state.code_anim_t > 0.0 {
                    state.code_anim_t -= dt / close_duration;
                    if state.code_anim_t < 0.0 {
                        state.code_anim_t = 0.0;
                    }
                    ctx.request_repaint(); // Ensure fluid animation
                }
            }
            let fs_t = state.code_anim_t;

            // Disable panel resizing when renaming to prevent conflicts
            let allow_resize = state.renaming_path.is_none();

            let mut panel = egui::SidePanel::left("multifunction_panel")
                .resizable(allow_resize)
                .width_range(150.0..=600.0)
                .default_width(state.sidebar_width);

            // If animating (opening or closing), force the width with elastic effect
            if t > 0.0 && t < 1.0 {
                // Elastic / BackOut Easing
                let c1 = 1.2; // Slightly less overshoot than before
                let c3 = c1 + 1.0;
                let ease_t = 1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2);

                let width = state.sidebar_width * ease_t;
                panel = panel.exact_width(width.max(0.0)).resizable(false);
            } else if !allow_resize {
                // When renaming, lock the panel to its current width
                panel = panel.exact_width(state.sidebar_width).resizable(false);
            }

            let panel_res = panel.show(ctx, |ui| {
                ui.set_enabled(main_ui_enabled);

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
                        let rect = ui.available_rect_before_wrap();
                        ui.allocate_rect(rect, egui::Sense::hover());

                        // Easing for slide
                        let ease_switch = 1.0 - (1.0 - switch_t).powi(2);

                        let width = rect.width();

                        // Render Old Tab
                        if let Some(source) = state.transition_source_tab {
                            let old_offset = -width * ease_switch;
                            let old_rect = rect.translate(egui::vec2(old_offset, 0.0));

                            let mut child_ui = ui.child_ui(old_rect, *ui.layout());
                            child_ui.visuals_mut().widgets.noninteractive.weak_bg_fill =
                                egui::Color32::from_black_alpha(
                                    ((1.0 - ease_switch) * 255.0) as u8,
                                );

                            render_tab_content(&mut child_ui, source, state);
                        }

                        let new_offset = width * (1.0 - ease_switch);
                        let new_rect = rect.translate(egui::vec2(new_offset, 0.0));

                        let mut child_ui = ui.child_ui(new_rect, *ui.layout());
                        child_ui.set_enabled(main_ui_enabled);

                        render_tab_content(&mut child_ui, tab_to_show, state);
                    } else {
                        // Standard Static Render
                        if fs_t <= 0.0 {
                            render_tab_content(ui, tab_to_show, state);
                        } else {
                            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
                        }
                    }
                }
            });

            // Update stored width only when not animating, fully open, and not renaming
            if t >= 1.0 && !is_fullscreen && state.renaming_path.is_none() {
                state.sidebar_width = panel_res.response.rect.width();
            }

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
                    egui::Rect::from_min_size(
                        egui::pos2(32.0, 0.0),
                        egui::vec2(250.0, screen_rect.height()),
                    )
                };

                // Easing (BackOut or Elastic for expansion)
                let t_anim = fs_t; // 0.0 to 1.0

                // BackOut: overshoot slightly then settle
                // c1 = 1.70158
                // c3 = c1 + 1
                // 1 + c3 * (t-1)^3 + c1 * (t-1)^2
                // Or maybe simple CubicOut/QuintOut for smoothness
                // let ease = 1.0 - (1.0 - t_anim).powi(4); // QuartOut

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
                    .order(egui::Order::Middle)
                    .show(ctx, |ui| {
                        // Background
                        egui::Frame::window(ui.style())
                            .fill(egui::Color32::from_rgb(30, 30, 30))
                            .show(ui, |ui| {
                                ui.set_enabled(main_ui_enabled); // Disable if modal is open

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
            ui.set_enabled(main_ui_enabled); // Disable if modal is open

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
                        // When starting playback, ensure DSL handlers are
                        // registered immediately (don't rely on debounce).
                        if state.playing {
                            state.dsl.event_handlers =
                                crate::dsl::extract_event_handlers_structured(&state.dsl_code);
                            // dispatch initial time event for the current playhead
                            state.set_time(state.time);
                        }
                    }
                    if ui.button("⏹ Reset").clicked() {
                        state.playing = false;
                        state.set_time(0.0);
                    }
                    ui.label(format!("Time: {:.2}s", state.time));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Export").clicked() {
                            // Seed modal with current project settings so user can review/adjust
                            state.export_modal_fps = state.fps;
                            state.export_modal_width = state.render_width;
                            state.export_modal_height = state.render_height;
                            state.export_modal_duration = state.duration_secs;
                            state.export_modal_step = 0;
                            state.export_ffmpeg_log.clear();
                            state.export_ffmpeg_done = false;
                            state.export_ffmpeg_error = None;
                            state.export_ffmpeg_rx = None;
                            state.show_export_modal = true;
                        }
                    });
                });

                ui.separator();

                // The Canvas (Fills the rest of the vertical space upwards)
                canvas::show(ui, state, main_ui_enabled);
            });
        });

        // Frame update for animation
        if state.playing {
            let dt = ctx.input(|i| i.stable_dt);
            // advance time and emit via centralized setter
            let next = state.time + dt;
            if next > state.duration_secs {
                // loop back to start
                state.set_time(0.0);
            } else {
                state.set_time(next);
            }

            // NOTA: Hemos eliminado request_preview_frames aquí porque el CompositionCallback
            // ya renderiza en vivo usando la GPU al 100%. Esto evita trabajo redundante.

            // Ensure maximum fluidity during playback
            ctx.request_repaint();
        }

        // Always control frame rate based on preview_fps (not just when playing)
        // This acts as a fallback or minimum frame rate when not playing,
        // but while playing we prefer the explicit request_repaint() above.
        let frame_duration = 1.0 / (state.preview_fps as f32);
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(frame_duration));

        // Poll background preview worker results and integrate textures (UI thread)
        crate::canvas::poll_preview_results(state, ctx);
        // position-cache background build removed — nothing to poll

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
            scene_graph::show(ui, state);
        }
        PanelTab::Code => {
            code_panel::show(ui, state);
        }
    }
}

// Element Modifiers UI moved to `src/modals/element_modifiers.rs` (fullscreen, non-draggable)
