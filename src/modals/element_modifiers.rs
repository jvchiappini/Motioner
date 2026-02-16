use crate::app_state::AppState;
use crate::scene::{get_shape_mut, Shape};
use crate::dsl;
use eframe::egui;

/// Fullscreen, non-draggable Element Modifiers modal.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    // Only render when active
    let path = match &state.modifier_active_path {
        Some(p) => p.clone(),
        None => return,
    };

    // Floating, draggable window centered on the full app window
    let screen_rect = ctx.input(|i| i.screen_rect());

    // Floating, draggable window centered on screen (keep initial position)
    let mut open = true;
    let center = screen_rect.center();
    let default_w = 420.0f32;
    let default_h = 480.0f32;
    let default_pos = egui::pos2(center.x - default_w / 2.0, center.y - default_h / 2.0);

    let window = egui::Window::new("🔧 Element Modifiers")
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .default_width(default_w)
        .default_height(default_h)
        .default_pos(default_pos)
        .movable(true);

    window.show(ctx, |ui| {
        // Body: render the same controls that previously lived in ui::show_modifier_modal
        let mut changed = false;
        if let Some(shape) = get_shape_mut(&mut state.scene, &path) {
            ui.add_space(4.0);

            let earliest_spawn = shape.spawn_time();

            // Stable identifier for this element's modifier UI (used as id_source
            // for collapsing headers so they don't reset when labels change).
            let path_id = state
                .modifier_active_path
                .as_ref()
                .map(|p| p.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("-"))
                .unwrap_or_else(|| "root".to_string());

            match shape {
                                Shape::Circle {
                                    name,
                                    x,
                                    y,
                                    radius,
                                    color,
                                    spawn_time,
                                    visible,
                                    animations,
                                    ..
                                } => {
                                    ui.group(|ui| {
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("⭕").size(18.0));
                                                ui.label(
                                                    egui::RichText::new("Circle Parameters")
                                                        .strong()
                                                        .size(14.0),
                                                );
                                            });
                                            ui.separator();

                                            egui::Grid::new("circle_grid")
                                                .num_columns(2)
                                                .spacing([12.0, 8.0])
                                                .show(ui, |ui| {
                                                    ui.label("Name:");
                                                    if ui.text_edit_singleline(name).changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Visible:");
                                                    if ui.checkbox(visible, "").changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Spawn Time:");
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(
                                                                spawn_time,
                                                                0.0..=state.duration_secs,
                                                            )
                                                            .suffix("s"),
                                                        )
                                                        .changed()
                                                    {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Position X:");
                                                    let mut val_x = *x * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_x, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *x = val_x / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Position Y:");
                                                    let mut val_y = *y * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_y, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *y = val_y / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Radius:");
                                                    let mut val_r = *radius * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_r, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *radius = val_r / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Color:");
                                                    if ui.color_edit_button_srgba_unmultiplied(color).changed()
                                                    {
                                                        changed = true;
                                                    }
                                                    ui.end_row();
                                                });

                                            ui.add_space(4.0);
                                            // --- Animations (Move) ---
                                            ui.collapsing("Animations", |ui| {
                                                // Allow adding a new Move animation
                                                ui.horizontal(|ui| {
                                                    if ui.button("+ Add Move").clicked() {
                                                        // default: start at element spawn (or 0), end at project end
                                                        let start = (*spawn_time).max(0.0);
                                                        let end = state.duration_secs;
                                                        let to_x = (*x + 0.20).min(1.0);
                                                        let to_y = *y;
                                                        animations.push(crate::scene::Animation::Move {
                                                            to_x,
                                                            to_y,
                                                            start,
                                                            end,
                                                            easing: crate::scene::Easing::Linear,
                                                        });
                                                        changed = true;
                                                    }
                                                });

                                                // List existing Move animations
                                                if animations.is_empty() {
                                                    ui.label("No animations");
                                                } else {
                                                    // iterate by index so we can remove safely after
                                                    let mut remove_idx: Option<usize> = None;

                                                    for i in 0..animations.len() {
                                                        // only show Move animations here
                                                        if let crate::scene::Animation::Move {
                                                            to_x,
                                                            to_y,
                                                            start,
                                                            end,
                                                            easing,
                                                        } = &mut animations[i]
                                                        {
                                                            let header_text = format!("Move #{} — {:.2}s → {:.2}s", i + 1, *start, *end);
                                                            let stable_id = format!("element_modifiers::{}::move::{}", path_id, i);
                                                            egui::CollapsingHeader::new(header_text)
                                                                .id_source(stable_id)
                                                                .show(ui, |ui| {
                                                                    ui.horizontal(|ui| {
                                                                        if ui.add(egui::Button::new("Remove").small()).clicked() {
                                                                            remove_idx = Some(i);
                                                                        }
                                                                    });

                                                                    ui.add_space(4.0);

                                                                    // Start / End times
                                                                    ui.horizontal(|ui| {
                                                                        ui.label("Start:");
                                                                        let mut s = *start;
                                                                        if ui.add(egui::Slider::new(&mut s, *spawn_time..=state.duration_secs).suffix("s")).changed() {
                                                                            *start = s.max(*spawn_time);
                                                                            // ensure end is not before start
                                                                            if *end < *start {
                                                                                *end = *start;
                                                                            }
                                                                            changed = true;
                                                                        }

                                                                        ui.label("End:");
                                                                        let mut e = *end;
                                                                        if ui.add(egui::Slider::new(&mut e, *start..=state.duration_secs).suffix("s")).changed() {
                                                                            *end = e.max(*start);
                                                                            changed = true;
                                                                        }
                                                                    });

                                                                    ui.add_space(4.0);

                                                                    // Target position (percent)
                                                                    ui.horizontal(|ui| {
                                                                        ui.label("To X:");
                                                                        let mut tx = *to_x * 100.0;
                                                                        if ui.add(egui::Slider::new(&mut tx, 0.0..=100.0).suffix("%")).changed() {
                                                                            *to_x = (tx / 100.0).clamp(0.0, 1.0);
                                                                            changed = true;
                                                                        }

                                                                        ui.label("To Y:");
                                                                        let mut ty = *to_y * 100.0;
                                                                        if ui.add(egui::Slider::new(&mut ty, 0.0..=100.0).suffix("%")).changed() {
                                                                            *to_y = (ty / 100.0).clamp(0.0, 1.0);
                                                                            changed = true;
                                                                        }
                                                                    });

                                                                    ui.add_space(4.0);

                                                                    // Easing selector (for now only Linear exists)
                                                                    ui.horizontal(|ui| {
                                                                        ui.label("Easing:");
                                                                        egui::ComboBox::from_label("")
                                                                            .selected_text(format!("{:?}", easing))
                                                                            .show_ui(ui, |ui| {
                                                                                if ui.selectable_label(matches!(easing, crate::scene::Easing::Linear), "Linear").clicked() {
                                                                                    *easing = crate::scene::Easing::Linear;
                                                                                    changed = true;
                                                                                }
                                                                            });
                                                                    });
                                                                });

                                                            ui.add_space(6.0);
                                                        }
                                                    }

                                                    // perform removal after iteration to avoid borrow issues
                                                    if let Some(idx) = remove_idx {
                                                        animations.remove(idx);
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        });
                                    });
                                }
                                Shape::Rect {
                                    name,
                                    x,
                                    y,
                                    w,
                                    h,
                                    color,
                                    spawn_time,
                                    visible,
                                    animations,
                                    ..
                                } => {
                                    ui.group(|ui| {
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("⬜").size(18.0));
                                                ui.label(
                                                    egui::RichText::new("Rectangle Parameters")
                                                        .strong()
                                                        .size(14.0),
                                                );
                                            });
                                            ui.separator();

                                            egui::Grid::new("rect_grid")
                                                .num_columns(2)
                                                .spacing([12.0, 8.0])
                                                .show(ui, |ui| {
                                                    ui.label("Name:");
                                                    if ui.text_edit_singleline(name).changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Visible:");
                                                    if ui.checkbox(visible, "").changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Spawn Time:");
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(
                                                                spawn_time,
                                                                0.0..=state.duration_secs,
                                                            )
                                                            .suffix("s"),
                                                        )
                                                        .changed()
                                                    {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Position X:");
                                                    let mut val_x = *x * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_x, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *x = val_x / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Position Y:");
                                                    let mut val_y = *y * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_y, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *y = val_y / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Width:");
                                                    let mut val_w = *w * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_w, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *w = val_w / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Height:");
                                                    let mut val_h = *h * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_h, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *h = val_h / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Color:");
                                                    if ui.color_edit_button_srgba_unmultiplied(color).changed()
                                                    {
                                                        changed = true;
                                                    }
                                                    ui.end_row();
                                                });

                                            ui.add_space(4.0);

                                                // --- Animations (Move) ---
                                                ui.collapsing("Animations", |ui| {
                                                    ui.horizontal(|ui| {
                                                        if ui.button("+ Add Move").clicked() {
                                                                let start = (*spawn_time).max(0.0);
                                                            let end = state.duration_secs;
                                                            let to_x = (*x + 0.20).min(1.0);
                                                            let to_y = *y;
                                                            animations.push(crate::scene::Animation::Move {
                                                                to_x,
                                                                to_y,
                                                                start,
                                                                end,
                                                                easing: crate::scene::Easing::Linear,
                                                            });
                                                            changed = true;
                                                        }
                                                    });

                                                    if animations.is_empty() {
                                                        ui.label("No animations");
                                                    } else {
                                                        let mut remove_idx: Option<usize> = None;
                                                        for i in 0..animations.len() {
                                                            if let crate::scene::Animation::Move {
                                                                to_x,
                                                                to_y,
                                                                start,
                                                                end,
                                                                easing,
                                                            } = &mut animations[i]
                                                            {
                                                                let header_text = format!("Move #{} — {:.2}s → {:.2}s", i + 1, *start, *end);
                                                                let stable_id = format!("element_modifiers::{}::move::{}", path_id, i);
                                                                egui::CollapsingHeader::new(header_text)
                                                                    .id_source(stable_id)
                                                                    .show(ui, |ui| {
                                                                        ui.horizontal(|ui| {
                                                                            if ui.add(egui::Button::new("Remove").small()).clicked() {
                                                                                remove_idx = Some(i);
                                                                            }
                                                                        });

                                                                        ui.add_space(4.0);

                                                                        ui.horizontal(|ui| {
                                                                            ui.label("Start:");
                                                                            let mut s = *start;
                                                                            if ui.add(egui::Slider::new(&mut s, *spawn_time..=state.duration_secs).suffix("s")).changed() {
                                                                                *start = s.max(*spawn_time);
                                                                                if *end < *start {
                                                                                    *end = *start;
                                                                                }
                                                                                changed = true;
                                                                            }

                                                                            ui.label("End:");
                                                                            let mut e = *end;
                                                                            if ui.add(egui::Slider::new(&mut e, *start..=state.duration_secs).suffix("s")).changed() {
                                                                                *end = e.max(*start);
                                                                                changed = true;
                                                                            }
                                                                        });

                                                                        ui.add_space(4.0);

                                                                        ui.horizontal(|ui| {
                                                                            ui.label("To X:");
                                                                            let mut tx = *to_x * 100.0;
                                                                            if ui.add(egui::Slider::new(&mut tx, 0.0..=100.0).suffix("%")).changed() {
                                                                                *to_x = (tx / 100.0).clamp(0.0, 1.0);
                                                                                changed = true;
                                                                            }

                                                                            ui.label("To Y:");
                                                                            let mut ty = *to_y * 100.0;
                                                                            if ui.add(egui::Slider::new(&mut ty, 0.0..=100.0).suffix("%")).changed() {
                                                                                *to_y = (ty / 100.0).clamp(0.0, 1.0);
                                                                                changed = true;
                                                                            }
                                                                        });

                                                                        ui.add_space(4.0);

                                                                        ui.horizontal(|ui| {
                                                                            ui.label("Easing:");
                                                                            egui::ComboBox::from_label("")
                                                                                .selected_text(format!("{:?}", easing))
                                                                                .show_ui(ui, |ui| {
                                                                                    if ui.selectable_label(matches!(easing, crate::scene::Easing::Linear), "Linear").clicked() {
                                                                                        *easing = crate::scene::Easing::Linear;
                                                                                        changed = true;
                                                                                    }
                                                                                });
                                                                        });
                                                                    });

                                                                ui.add_space(6.0);
                                                            }
                                                        }

                                                        if let Some(idx) = remove_idx {
                                                            animations.remove(idx);
                                                            changed = true;
                                                        }
                                                    }
                                                });
                                        });
                                    });
                                }
                                Shape::Group {
                                    name,
                                    children: _,
                                    visible,
                                } => {
                                    ui.group(|ui| {
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("📦").size(18.0));
                                                ui.label(
                                                    egui::RichText::new("Group Parameters")
                                                        .strong()
                                                        .size(14.0),
                                                );
                                            });
                                            ui.separator();

                                            egui::Grid::new("group_grid")
                                                .num_columns(2)
                                                .spacing([12.0, 8.0])
                                                .show(ui, |ui| {
                                                    ui.label("Visible:");
                                                    if ui.checkbox(visible, "").changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Name:");
                                                    if ui.text_edit_singleline(name).changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Earliest Spawn:");
                                                    ui.label(
                                                        egui::RichText::new(format!("{:.2}s", earliest_spawn))
                                                            .weak(),
                                                    );
                                                    ui.end_row();
                                                });

                                            ui.add_space(4.0);
                                        });
                                    });
                                }
                            }
                        } else {
                            ui.label("No element found at this path.");
                            state.modifier_active_path = None;
                        }

            // Persist changes immediately if any
            if changed {
                state.position_cache = None; // invalidate cache
                state.dsl_code = dsl::generate_dsl(
                    &state.scene,
                    state.render_width,
                    state.render_height,
                    state.fps,
                    state.duration_secs,
                );
                crate::events::element_properties_changed_event::on_element_properties_changed(state);
            }
        
    });

    // If the window was closed by the user, clear the active path
    if !open {
        state.modifier_active_path = None;
    }
}
