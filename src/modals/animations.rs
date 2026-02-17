use crate::app_state::AppState;
use eframe::egui;

/// Top-level Animations modal (moved from `scene_graph.rs`).
/// Features: close on Esc, close on click-outside, remember position.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_animations_modal {
        return;
    }

    // Compute default position (centered) unless a remembered position exists
    let screen_rect = ctx.input(|i| i.screen_rect());
    let default_w = 360.0f32;
    let default_h = 180.0f32;
    #[allow(clippy::map_identity)]
    let default_pos = state.animations_modal_pos.map(|p| p).unwrap_or_else(|| {
        egui::pos2(
            screen_rect.center().x - default_w / 2.0,
            screen_rect.center().y - default_h / 2.0,
        )
    });

    // Note: we DO NOT install an interactive full-screen backdrop here because
    // that would intercept pointer/drag events and prevent the window from
    // receiving title-bar drags or clicks. Instead we detect "click-outside"
    // after the window is shown (see below).

    // Window (floating, movable)
    let mut open = state.show_animations_modal;
    let window = egui::Window::new("Animations")
        .open(&mut open)
        .resizable(false)
        .default_size(egui::vec2(default_w, default_h))
        .default_pos(default_pos)
        .movable(true);

    let inner = window.show(ctx, |ui| {
        // Close on Esc when modal is focused
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            state.show_animations_modal = false;
        }

        ui.set_width(340.0);
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Animations").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new("❌").frame(false)).clicked() {
                    state.show_animations_modal = false;
                }
            });
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // Target selection: prefer selected_node_path, fallback to top-level selection
        let mut target_path: Option<Vec<usize>> = None;
        if let Some(p) = &state.selected_node_path {
            target_path = Some(p.clone());
            ui.label(format!(
                "Target: {}",
                crate::scene::get_shape(&state.scene, p)
                    .map(|s| s.name().to_string())
                    .unwrap_or("(unknown)".to_string())
            ));
        } else if let Some(idx) = state.selected {
            target_path = Some(vec![idx]);
            ui.label(format!(
                "Target: {}",
                state
                    .scene
                    .get(idx)
                    .map(|s| s.name().to_string())
                    .unwrap_or("(unknown)".to_string())
            ));
        } else {
            ui.label("Target: (none selected)");
            ui.add_space(4.0);
            // allow user to pick a top-level element (persisted in AppState)
            let mut pick_idx = state.anim_modal_target_idx;
            let names: Vec<String> = state.scene.iter().map(|s| s.name().to_string()).collect();
            if !names.is_empty() {
                egui::ComboBox::from_label("Pick element")
                    .selected_text(names[pick_idx].clone())
                    .show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(pick_idx == i, n).clicked() {
                                pick_idx = i;
                                state.anim_modal_target_idx = pick_idx;
                            }
                        }
                    });
                target_path = Some(vec![pick_idx]);
            }
        }

        ui.add_space(8.0);
        ui.label("Available animations: Move (linear, ease_in_out)")
            .on_hover_text(
                "Move animation — move an element from its position at animation start to a target (To X, To Y) over [Start, End].\n\nDetails:\n• Before Start: element stays at its base position.\n• During: position interpolated from current position at Start toward target.\n• After End: element remains at the target.\n\nEasing: `linear`, `ease_in/out/in_out(power)`, presets `sine`, `expo`, `circ`, or physics-like `spring(damping, stiffness, mass)`, `elastic(amplitude, period)`, `bounce(bounciness)`.\n\nDSL examples: `ease = sine`, `ease = spring(damping = 0.7, stiffness = 120.0)`.",
            );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.add(egui::Button::new("Add Move (linear)")).on_hover_text(
                "Add a Move animation to the selected element.\n\nDefault values: Start = element spawn (or 0), End = project duration, Easing = linear.\n\nAfter adding, edit Start/End, To X/To Y and choose Easing (linear or ease_in_out(power)). Note: Start must be >= element spawn time.",
            ).clicked() {
                if let Some(path) = target_path.clone() {
                    if let Some(shape) = crate::scene::get_shape_mut(&mut state.scene, &path) {
                        // new animation parameters (default: full project duration)
                        let start = 0.0f32;
                        let end = state.duration_secs;
                        // validate: animation start time (seconds) must be >= element.spawn_time()
                        let anim_start_secs = start; // start is stored in seconds
                        let spawn_secs = shape.spawn_time();
                        if anim_start_secs < spawn_secs {
                            state.toast_message = Some(format!(
                                "Cannot add animation: starts at {:.2}s before element spawn at {:.2}s",
                                anim_start_secs, spawn_secs
                            ));
                            state.toast_type = crate::app_state::ToastType::Error;
                            state.toast_deadline = ui.input(|i| i.time) + 3.0;
                        } else {
                            match shape {
                                crate::scene::Shape::Circle { x, y, animations, .. } => {
                                    let to_x = (*x + 0.20).min(1.0);
                                    animations.push(crate::scene::Animation::Move {
                                        to_x,
                                        to_y: *y,
                                        start,
                                        end,
                                        easing: crate::scene::Easing::Linear,
                                    });
                                }
                                crate::scene::Shape::Rect { x, y, animations, .. } => {
                                    let to_x = (*x + 0.20).min(1.0);
                                    animations.push(crate::scene::Animation::Move {
                                        to_x,
                                        to_y: *y,
                                        start,
                                        end,
                                        easing: crate::scene::Easing::Linear,
                                    });
                                }
                                _ => {}
                            }
                            // regenerate DSL and preview frames
                            state.position_cache = None; // scene changed → invalidate position cache
                            state.dsl_code = crate::dsl::generate_dsl(
                                &state.scene,
                                state.render_width,
                                state.render_height,
                                state.fps,
                                state.duration_secs,
                            );
                            // persist DSL to project (if set)
                            crate::events::element_properties_changed_event::on_element_properties_changed(state);
                            crate::canvas::generate_preview_frames(state, state.time, ctx);
                            state.show_animations_modal = false;
                            state.toast_message = Some("Animation added".to_string());
                            state.toast_type = crate::app_state::ToastType::Success;
                            state.toast_deadline = ui.input(|i| i.time) + 2.0;
                        }
                    }
                }
            }
        });
    });

    // If window was closed via the window 'open' control, reflect that in state
    if !open {
        state.show_animations_modal = false;
        return;
    }

    // Save/remember position while open (persist the last known top-left)
    if let Some(inner_resp) = &inner {
        let win_rect = inner_resp.response.rect;
        state.animations_modal_pos = Some(win_rect.min);

        // NOTE: clicking outside no longer closes the modal (user requested).
    }
}
