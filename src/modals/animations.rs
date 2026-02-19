use crate::app_state::AppState;
use eframe::egui;

/// Top-level Animations modal (moved from `scene_graph.rs`).

/// Animations modal — lightweight UI to add basic animations to elements.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_animations_modal {
        return;
    }

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

    let mut open = state.show_animations_modal;
    let window = egui::Window::new("Animations")
        .open(&mut open)
        .resizable(false)
        .default_size(egui::vec2(default_w, default_h))
        .default_pos(default_pos)
        .movable(true);

    let inner = window.show(ctx, |ui| {
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

        // Target selection
        let mut target_path: Option<Vec<usize>> = None;
        if let Some(p) = &state.selected_node_path {
            target_path = Some(p.clone());
            let label = state.scene.get(p[0]).map(|e| e.name.clone()).unwrap_or("(unknown)".into());
            ui.label(format!("Target: {}", label));
        } else if let Some(idx) = state.selected {
            target_path = Some(vec![idx]);
            let label = state.scene.get(idx).map(|e| e.name.clone()).unwrap_or("(unknown)".into());
            ui.label(format!("Target: {}", label));
        } else {
            ui.label("Target: (none selected)");
            ui.add_space(4.0);
            let mut pick_idx = state.anim_modal_target_idx;
            let names: Vec<String> = state.scene.iter().map(|e| e.name.clone()).collect();
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
            .on_hover_text("Move animation — move an element from its position at animation start to a target (To X, To Y) over [Start, End].");

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("Add Move (linear)").min_size(egui::vec2(160.0, 28.0)))
                .on_hover_text("Add a Move animation to the selected element. Default: Start=spawn or 0, End=project duration, Easing=linear.")
                .clicked()
            {
                if let Some(path) = target_path.clone() {
                    if path.len() == 1 {
                        if let Some(elem) = state.scene.get_mut(path[0]) {
                            let start = 0.0f32;
                            let end = state.duration_secs;
                            let spawn_secs = elem.spawn_frame as f32 / elem.fps as f32;

                            if start < spawn_secs {
                                state.toast_message = Some(format!(
                                    "Cannot add animation: starts at {:.2}s before element spawn at {:.2}s",
                                    start, spawn_secs
                                ));
                                state.toast_type = crate::app_state::ToastType::Error;
                                state.toast_deadline = ui.input(|i| i.time) + 3.0;
                            } else {
                                let base_x = elem.sample(elem.spawn_frame).and_then(|p| p.x).unwrap_or(0.5);
                                let base_y = elem.sample(elem.spawn_frame).and_then(|p| p.y).unwrap_or(0.5);
                                let to_x = (base_x + 0.20).min(1.0);

                                elem.animations.push(crate::scene::Animation::Move {
                                    to_x,
                                    to_y: base_y,
                                    start,
                                    end,
                                    easing: crate::scene::Easing::Linear,
                                });
                                // position cache removed — no-op
                                state.dsl_code = crate::dsl::generate_dsl(
                                    &crate::shapes::element_store::to_legacy_shapes(&state.scene),
                                    state.render_width,
                                    state.render_height,
                                    state.fps,
                                    state.duration_secs,
                                );
                                crate::events::element_properties_changed_event::on_element_properties_changed(state);
                                crate::canvas::generate_preview_frames(state, state.time, ctx);

                                state.show_animations_modal = false;
                                state.toast_message = Some("Animation added".to_string());
                                state.toast_type = crate::app_state::ToastType::Success;
                                state.toast_deadline = ui.input(|i| i.time) + 2.0;
                            }
                        }
                    } else {
                        state.toast_message = Some("Nested element selection not supported yet".into());
                        state.toast_type = crate::app_state::ToastType::Error;
                        state.toast_deadline = ui.input(|i| i.time) + 2.0;
                    }
                }
            }
        });
    });

    if !open {
        state.show_animations_modal = false;
        return;
    }

    if let Some(inner_resp) = &inner {
        let win_rect = inner_resp.response.rect;
        state.animations_modal_pos = Some(win_rect.min);
    }
}
