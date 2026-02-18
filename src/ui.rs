use crate::app_state::{AppState, PanelTab};
use crate::{canvas, code_panel, dsl, scene_graph, timeline};
use eframe::egui;

pub struct MyApp {
    state: AppState,
}

pub fn create_app(_cc: &eframe::CreationContext<'_>) -> MyApp {
    let mut state = AppState::default();

    // -- Initialize Autocomplete Worker Thread ---------------------
    let (atx, arx) = std::sync::mpsc::channel::<String>();
    let (btx, brx) = std::sync::mpsc::channel::<Vec<crate::app_state::CompletionItem>>();
    state.completion_worker_tx = Some(atx);
    state.completion_worker_rx = Some(brx);

    std::thread::spawn(move || {
        while let Ok(query) = arx.recv() {
            // Catalogue of completion candidates (static for now)
            let candidates = vec![
                crate::app_state::CompletionItem { label: "project".into(), insert_text: "project".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "timeline".into(), insert_text: "timeline".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "layer".into(), insert_text: "layer".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "fps".into(), insert_text: "fps".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "duration".into(), insert_text: "duration".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "size".into(), insert_text: "size".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "fill".into(), insert_text: "fill".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "radius".into(), insert_text: "radius".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "width".into(), insert_text: "width".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "height".into(), insert_text: "height".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "color".into(), insert_text: "color".into(), is_snippet: false },
                crate::app_state::CompletionItem { label: "circle".into(), insert_text: "circle \"Name\" {\n    x = 0.50,\n    y = 0.50,\n    radius = 0.10,\n    fill = \"#78c8ff\",\n    spawn = 0.00\n}\n".into(), is_snippet: true },
                crate::app_state::CompletionItem { label: "rect".into(), insert_text: "rect \"Name\" {\n    x = 0.50,\n    y = 0.50,\n    width = 0.30,\n    height = 0.20,\n    fill = \"#c87878\",\n    spawn = 0.00\n}\n".into(), is_snippet: true },
                crate::app_state::CompletionItem { label: "text".into(), insert_text: "text \"Name\" {\n    x = 0.50,\n    y = 0.50,\n    value = \"Hello\",\n    font = \"System\",\n    size = 24.0,\n    fill = \"#ffffff\",\n    spawn = 0.00\n}\n".into(), is_snippet: true },
                crate::app_state::CompletionItem { label: "move".into(), insert_text: "move {\n    element = \"Name\",\n    to = (0.50, 0.50),\n    during = 0.00 -> 1.00,\n    ease = linear\n}\n".into(), is_snippet: true },
            ];

            let filtered: Vec<_> = candidates
                .into_iter()
                .filter(|c| c.label.starts_with(&query) && c.label != query)
                .collect();

            let _ = btx.send(filtered);
        }
    });

    // VRAM DETECTION: Detectar memoria GPU disponible al iniciar
    if let Some(wgpu_render_state) = _cc.wgpu_render_state.as_ref() {
        let adapter_info = &wgpu_render_state.adapter.get_info();
        state.estimated_vram_bytes = canvas::detect_vram_size(adapter_info);
        println!(
            "[motioner] VRAM detected: {:.1} GB ({} bytes) on {}",
            state.estimated_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            state.estimated_vram_bytes,
            adapter_info.name
        );
        println!(
            "[motioner] VRAM cache limit: {:.1} GB ({:.0}% of total)",
            (state.estimated_vram_bytes as f64 * state.vram_cache_max_percent as f64)
                / (1024.0 * 1024.0 * 1024.0),
            state.vram_cache_max_percent * 100.0
        );
    } else {
        state.estimated_vram_bytes = 512 * 1024 * 1024; // fallback 512 MB
        println!("[motioner] No wgpu adapter, using fallback VRAM estimate: 512 MB");
    }

    // OPTIMIZACIÓN RAM: Limpiar caches al inicio para reducir uso de memoria
    state.preview_frame_cache.clear();
    state.preview_frame_cache.shrink_to_fit();
    state.preview_compressed_cache.clear();
    state.preview_compressed_cache.shrink_to_fit();
    state.preview_texture_cache.clear();
    state.preview_texture_cache.shrink_to_fit();

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

        // Ensure fonts used in scene are loaded in egui
        let mut used_fonts = std::collections::HashSet::new();
        fn collect_fonts(shapes: &[crate::scene::Shape], fonts: &mut std::collections::HashSet<String>) {
            for s in shapes {
                match s {
                    crate::scene::Shape::Text(t) => { 
                        fonts.insert(t.font.clone()); 
                        for span in &t.spans {
                            fonts.insert(span.font.clone());
                        }
                    }
                    crate::scene::Shape::Group { children, .. } => collect_fonts(children, fonts),
                    _ => {}
                }
            }
        }
        collect_fonts(&state.scene, &mut used_fonts);
        let mut fonts_changed = false;
        for font_name in used_fonts {
            if font_name != "System" && !font_name.is_empty() {
                if let Some(path) = state.font_map.get(&font_name) {
                    if crate::shapes::fonts::load_font(&mut state.font_definitions, &font_name, path) {
                        fonts_changed = true;
                    }
                    if !state.font_arc_cache.contains_key(&font_name) {
                        if let Some(font) = crate::shapes::fonts::load_font_arc(path) {
                            state.font_arc_cache.insert(font_name.clone(), font);
                        }
                    }
                }
            }
        }
        if fonts_changed {
            ctx.set_fonts(state.font_definitions.clone());
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
                    state.last_code_edit_time = Some(ctx.input(|i| i.time));
                    state.autosave_pending = true;
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
            state.dsl_code = dsl::generate_dsl(
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

        // Throttle system stats update (e.g. every 1.0s)
        let now = ctx.input(|i| i.time);
        // -- Autosave debounce handling ---------------------------------
        if state.autosave_pending {
            if let Some(last_edit) = state.last_code_edit_time {
                if now - last_edit > state.autosave_cooldown_secs as f64 {
                    // perform the autosave (silent) and set indicator
                    match crate::events::element_properties_changed_event::write_dsl_to_project(
                        state, false,
                    ) {
                        Ok(_) => {
                            state.autosave_pending = false;
                            state.autosave_last_success_time = Some(now);
                            state.autosave_error = None;
                        }
                        Err(e) => {
                            state.autosave_pending = false;
                            state.autosave_error = Some(e.to_string());
                        }
                    }
                }
            }
        }
        // Debounced DSL parsing (separate from autosave). Run a lightweight
        // parse after the user stops typing for a short interval so the
        // UI isn't blocked on every keystroke.
        if let Some(last_edit) = state.last_code_edit_time {
            // parse after ~120ms of inactivity
            let parse_debounce = 0.12_f64;
            if now - last_edit > parse_debounce && now - state.last_scene_parse_time > 0.0 {
                // Try to parse configuration (non-fatal). Do NOT show errors while typing.
                if let Ok(config) = crate::dsl::parse_config(&state.dsl_code) {
                    state.fps = config.fps;
                    state.duration_secs = config.duration;
                    state.render_width = config.width;
                    state.render_height = config.height;
                }

                // Try to parse DSL into scene; only update scene & preview on successful parse
                let parsed = crate::dsl::parse_dsl(&state.dsl_code);
                if !parsed.is_empty() {
                    state.scene = parsed;
                    // collect DSL event handler blocks (e.g. `on_time { ... }`)
                    state.dsl_event_handlers =
                        crate::dsl::extract_event_handlers_structured(&state.dsl_code);
                    // regenerate preview for current playhead (single-frame request)
                    crate::canvas::request_preview_frames(
                        state,
                        state.time,
                        crate::canvas::PreviewMode::Single,
                    );
                }

                state.last_scene_parse_time = now;
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
                            state.dsl_event_handlers =
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

            // Request single-frame preview generation for the new playhead time (non-blocking)
            crate::canvas::request_preview_frames(
                state,
                state.time,
                crate::canvas::PreviewMode::Single,
            );

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
        // Poll for asynchronous position-cache build completion
        if let Some(rx) = &state.position_cache_build_rx {
            if let Ok(pc) = rx.try_recv() {
                state.position_cache = Some(pc);
                state.position_cache_build_in_progress = false;
                state.toast_message = Some("Position cache ready".to_string());
                state.toast_type = crate::app_state::ToastType::Info;
                state.toast_deadline = ctx.input(|i| i.time) + 2.0;
                state.position_cache_build_rx = None;
            }
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
            scene_graph::show(ui, state);
        }
        PanelTab::Code => {
            code_panel::show(ui, state);
        }
    }
}

// Element Modifiers UI moved to `src/modals/element_modifiers.rs` (fullscreen, non-draggable)
