use eframe::egui;
use crate::scene::{Animation, Easing};
use crate::modals::element_modifiers::easing_curve_editor::render_easing_curve_editor;

pub fn render_move_animation_modifiers(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    animations: &mut Vec<Animation>,
    spawn_time: f32,
    duration_secs: f32,
    base_x: f32,
    base_y: f32,
    path_id: &str,
    changed: &mut bool,
) {
    ui.collapsing("Animations", |ui| {
        // Allow adding a new Move animation
        ui.horizontal(|ui| {
            // Add Move button with descriptive tooltip
            if ui
                .add(egui::Button::new("+ Add Move"))
                .on_hover_text("Add a Move animation to this element.\n\nCreates a position animation that interpolates the element from its current position at Start toward (To X, To Y) over [Start, End]. Defaults: Start = element spawn (or 0), End = project duration, Easing = linear.")
                .clicked()
            {
                // default: start at element spawn (or 0), end at project end
                let start = (spawn_time).max(0.0);
                let end = duration_secs;
                let to_x = (base_x + 0.20).min(1.0);
                let to_y = base_y;
                animations.push(Animation::Move {
                    to_x,
                    to_y,
                    start,
                    end,
                    easing: Easing::Linear,
                });
                *changed = true;
            }

            // tooltip describing the Move animation and available easing types (info icon)
            ui.add_space(6.0);
            ui.label(egui::RichText::new("ⓘ").weak()).on_hover_text(
                "Move animation — moves an element from its position at the animation Start to the specified target (To X, To Y) over [Start, End].\n\nBehavior:\n• Before Start: element stays at its base position.\n• During: interpolates from the element's position at Start toward the target.\n• After End: element remains at the target.\n\nParameters:\n• Start / End (seconds), To X / To Y (0.0..1.0).\n• Easing: `linear` = constant speed; `ease_in_out(power)` = symmetric ease-in/out (power controls curvature; 1.0 = linear).\n\nDSL example: `type = ease_in_out(power = 2.0)`.",
            );
        });

        // List existing Move animations
        if animations.is_empty() {
            ui.label("No animations");
        } else {
            // iterate by index so we can remove safely after
            let mut remove_idx: Option<usize> = None;

            for i in 0..animations.len() {
                // only show Move animations here
                if let Animation::Move {
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
                                ui.add_space(6.0);
                                ui.label(egui::RichText::new("ⓘ").weak()).on_hover_text(
                                    "Move animation — moves the element toward `To X, To Y` between `Start` and `End`.\n\nMultiple Move animations are applied in chronological order; each animation interpolates from the element's position at that animation's Start.\n\nEasing options: `linear` (constant speed) or `ease_in_out(power)` (symmetric ease-in/out). Use the `power` slider to control the curve (default 1.0).",
                                );
                            });

                            ui.add_space(4.0);

                            // Start / End times
                            ui.horizontal(|ui| {
                                ui.label("Start:");
                                let mut s = *start;
                                if ui.add(egui::Slider::new(&mut s, spawn_time..=duration_secs).suffix("s")).changed() {
                                    *start = s.max(spawn_time);
                                    // ensure end is not before start
                                    if *end < *start {
                                        *end = *start;
                                    }
                                    *changed = true;
                                }

                                ui.label("End:");
                                let mut e = *end;
                                if ui.add(egui::Slider::new(&mut e, *start..=duration_secs).suffix("s")).changed() {
                                    *end = e.max(*start);
                                    *changed = true;
                                }
                            });

                            ui.add_space(4.0);

                            // Target position (percent)
                            ui.horizontal(|ui| {
                                ui.label("To X:");
                                let mut tx = *to_x * 100.0;
                                if ui.add(egui::Slider::new(&mut tx, 0.0..=100.0).suffix("%")).changed() {
                                    *to_x = (tx / 100.0).clamp(0.0, 1.0);
                                    *changed = true;
                                }

                                ui.label("To Y:");
                                let mut ty = *to_y * 100.0;
                                if ui.add(egui::Slider::new(&mut ty, 0.0..=100.0).suffix("%")).changed() {
                                    *to_y = (ty / 100.0).clamp(0.0, 1.0);
                                    *changed = true;
                                }
                            });

                            ui.add_space(4.0);

                            // Easing selector
                            ui.horizontal(|ui| {
                                ui.label("Easing:");
                                egui::ComboBox::from_label("")
                                    .selected_text(format!("{:?}", easing))
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(matches!(easing, Easing::Linear), "Linear").on_hover_text("Linear — constant speed (uniform velocity)").clicked() {
                                            *easing = Easing::Linear;
                                            *changed = true;
                                        }

                                        if ui.selectable_label(matches!(easing, Easing::EaseIn { .. }), "EaseIn").on_hover_text("EaseIn(power) — accelerate from zero; progress = t^power").clicked() {
                                            *easing = Easing::EaseIn { power: 1.0 };
                                            *changed = true;
                                        }

                                        if ui.selectable_label(matches!(easing, Easing::EaseOut { .. }), "EaseOut").on_hover_text("EaseOut(power) — decelerate to stop; progress = 1 - (1-t)^power").clicked() {
                                            *easing = Easing::EaseOut { power: 1.0 };
                                            *changed = true;
                                        }

                                        if ui.selectable_label(matches!(easing, Easing::EaseInOut { .. }), "EaseInOut").on_hover_text("EaseInOut(power) — symmetric ease-in/out (use for smooth start+end)").clicked() {
                                            *easing = Easing::EaseInOut { power: 1.0 };
                                            *changed = true;
                                        }

                                        if ui.selectable_label(matches!(easing, Easing::Custom { .. }), "Custom").on_hover_text("Custom — define your own curve by adding points").clicked() {
                                            *easing = Easing::Custom { points: vec![(0.0, 0.0), (1.0, 1.0)] };
                                            *changed = true;
                                        }
                                        if ui.selectable_label(matches!(easing, Easing::Bezier { .. }), "Bezier").on_hover_text("Bezier — smooth curve with 2 control points").clicked() {
                                            *easing = Easing::Bezier { p1: (0.42, 0.0), p2: (0.58, 1.0) };
                                            *changed = true;
                                        }
                                    });
                                });

                            ui.label("Easing Curve:");
                            
                            // Use unified curve editor
                            if render_easing_curve_editor(ui, ctx, easing, i, "move") {
                                *changed = true;
                            }
                        });

                    ui.add_space(6.0);
                }
            }

            // perform removal after iteration to avoid borrow issues
            if let Some(idx) = remove_idx {
                animations.remove(idx);
                *changed = true;
            }
        }
    });
}
