use crate::app_state::AppState;
use crate::scene::{move_node, Shape};
use eframe::egui;
use eframe::egui::{Color32, Frame, Id, InnerResponse, LayerId, Order, Sense};
use std::any::Any;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.heading(egui::RichText::new("Scene Graph").strong().size(16.0));
    });
    ui.add_space(8.0);

    // ── Root drop zone ──
    // Always reserve space so items don't shift vertically.
    // Only draw visual content when something is being dragged.
    let is_dragging = ui.memory(|m| m.is_anything_being_dragged());

    let root_payload = {
        let (_response, payload) = ui.dnd_drop_zone::<Vec<usize>>(Frame::none(), |ui| {
            let height = 28.0;
            let (rect, _) =
                ui.allocate_at_least(egui::vec2(ui.available_width(), height), Sense::hover());
            if is_dragging {
                let hovered = ui.rect_contains_pointer(rect);
                let stroke = if hovered {
                    egui::Stroke::new(1.5, Color32::LIGHT_BLUE)
                } else {
                    egui::Stroke::new(1.0, Color32::from_gray(60))
                };
                ui.painter().rect_stroke(rect, 4.0, stroke);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "📂 Drop here to move to root",
                    egui::FontId::proportional(11.0),
                    if hovered {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(160)
                    },
                );
            }
        });
        payload
    };

    if let Some(arc_path) = root_payload {
        let dragged = (*arc_path).clone();
        state.move_request = Some((dragged, vec![], state.scene.len()));
    }

    ui.add_space(4.0);

    // ── Main scroll area ──
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut removals = Vec::new();
            let len = state.scene.len();
            for i in 0..len {
                render_node(ui, state, vec![i], &mut removals);
            }
            for path in removals {
                if state.selected_node_path.as_ref() == Some(&path) {
                    state.selected = None;
                    state.selected_node_path = None;
                }
            }
        });

    // Execute queued move
    if let Some((from, to_parent, to_idx)) = state.move_request.take() {
        if let Some(new_path) = move_node(&mut state.scene, &from, &to_parent, to_idx) {
            state.selected_node_path = Some(new_path);
        }
    }

    // ── Bottom bar ──
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("➕ Elements").clicked() {
                state.show_elements_modal = true;
            }
        });
    });

    // ── Elements Modal ──
    if state.show_elements_modal {
        egui::Window::new("Elements")
            .resizable(false)
            .default_size(egui::vec2(320.0, 160.0))
            .show(ui.ctx(), |ui| {
                ui.set_width(300.0);
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("Elements").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("❌").frame(false)).clicked() {
                            state.show_elements_modal = false;
                        }
                    });
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    if ui.button("📦  Group").clicked() {
                        state.scene.push(Shape::Group {
                            name: format!("Group #{}", state.scene.len()),
                            children: Vec::new(),
                            visible: true,
                        });
                    }
                    if ui.button("⭕   Circle").clicked() {
                        state.scene.push(Shape::Circle {
                            name: format!("Circle #{}", state.scene.len()),
                            x: 0.5,
                            y: 0.5,
                            radius: 0.1,
                            color: [120, 200, 255, 255],
                            spawn_time: 0.0,
                            visible: true,
                        });
                    }
                    if ui.button("⬜  Rectangle").clicked() {
                        state.scene.push(Shape::Rect {
                            name: format!("Rect #{}", state.scene.len()),
                            x: 0.4,
                            y: 0.4,
                            w: 0.2,
                            h: 0.2,
                            color: [200, 120, 120, 255],
                            spawn_time: 0.0,
                            visible: true,
                        });
                    }
                });
            });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Node rendering
// ─────────────────────────────────────────────────────────────────────────────

fn render_node(
    ui: &mut egui::Ui,
    state: &mut AppState,
    path: Vec<usize>,
    removals: &mut Vec<Vec<usize>>,
) {
    let is_selected = state.selected_node_path.as_ref() == Some(&path);
    let is_renaming = state.renaming_path.as_ref() == Some(&path);

    let (is_group, children_count, node_name, icon, icon_color, is_visible) = {
        if let Some(shape) = crate::scene::get_shape(&state.scene, &path) {
            let name = shape.name().to_string();
            let vis = shape.is_visible();
            match shape {
                Shape::Group { children, .. } => (
                    true,
                    children.len(),
                    name,
                    "📦",
                    Color32::from_rgb(255, 200, 100),
                    vis,
                ),
                Shape::Circle { .. } => {
                    (false, 0, name, "⭕", Color32::from_rgb(100, 200, 255), vis)
                }
                Shape::Rect { .. } => (false, 0, name, "⬜", Color32::from_rgb(255, 100, 100), vis),
            }
        } else {
            return;
        }
    };

    let drag_id = Id::new("scene_drag").with(&path);

    // ── Row content ──
    let render_row = |ui: &mut egui::Ui, state: &mut AppState| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // Visibility toggle
            let vis_text = if is_visible { "👁" } else { "🚫" };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(vis_text).small().color(if is_visible {
                        Color32::WHITE
                    } else {
                        Color32::GRAY
                    }))
                    .frame(false),
                )
                .clicked()
            {
                if let Some(shape) = crate::scene::get_shape_mut(&mut state.scene, &path) {
                    shape.set_visible(!is_visible);
                }
            }

            // Name / Rename
            if is_renaming {
                let res = ui.add(
                    egui::TextEdit::singleline(&mut state.rename_buffer)
                        .desired_width(f32::INFINITY),
                );
                if res.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(shape) = crate::scene::get_shape_mut(&mut state.scene, &path) {
                        match shape {
                            Shape::Group { name, .. } => *name = state.rename_buffer.clone(),
                            Shape::Circle { name, .. } => *name = state.rename_buffer.clone(),
                            Shape::Rect { name, .. } => *name = state.rename_buffer.clone(),
                        }
                    }
                    state.renaming_path = None;
                }
            } else {
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    icon,
                    0.0,
                    egui::TextFormat {
                        color: if is_visible {
                            icon_color
                        } else {
                            Color32::from_gray(80)
                        },
                        ..Default::default()
                    },
                );
                job.append(
                    &format!(" {}", node_name),
                    0.0,
                    egui::TextFormat {
                        color: if is_selected {
                            Color32::WHITE
                        } else if is_visible {
                            Color32::from_gray(200)
                        } else {
                            Color32::from_gray(100)
                        },
                        ..Default::default()
                    },
                );

                let label_res = ui.add(
                    egui::Label::new(job)
                        .selectable(false)
                        .sense(Sense::click()),
                );

                if label_res.clicked() {
                    state.selected_node_path = Some(path.clone());
                }
                if label_res.double_clicked() {
                    state.renaming_path = Some(path.clone());
                    state.rename_buffer = node_name.clone();
                }

                // Selection highlight — subtle blue tint
                if is_selected {
                    ui.painter().rect_filled(
                        label_res.rect.expand2(egui::vec2(20.0, 2.0)),
                        2.0,
                        Color32::from_rgba_premultiplied(60, 120, 200, 30),
                    );
                }
            }

            // Settings button
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⚙").clicked() {
                    state.modifier_active_path = Some(path.clone());
                }
            });
        });
    };

    // ── Render node with drag source ──
    if is_group {
        let coll_id = Id::new("group_collapsing").with(&path);
        let coll_state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            coll_id,
            false,
        );

        let (zone_res, payload) = ui.dnd_drop_zone::<Vec<usize>>(Frame::none(), |ui| {
            coll_state
                .show_header(ui, |ui| {
                    dnd_drag_source_transparent(ui, drag_id, path.clone(), |ui| {
                        render_row(ui, state);
                    });
                })
                .body(|ui| {
                    for i in 0..children_count {
                        let mut child_path = path.clone();
                        child_path.push(i);
                        render_node(ui, state, child_path, removals);
                    }
                });
        });

        // Drop-target highlight
        if zone_res.hovered() && ui.memory(|m| m.is_anything_being_dragged()) {
            ui.painter().rect_stroke(
                zone_res.rect,
                2.0,
                (1.5, Color32::from_rgba_premultiplied(100, 200, 255, 100)),
            );
        }

        if let Some(arc_path) = payload {
            let from = (*arc_path).clone();
            if from != path {
                state.move_request = Some((from, path.clone(), children_count));
            }
        }
    } else {
        dnd_drag_source_transparent(ui, drag_id, path.clone(), |ui| {
            render_row(ui, state);
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Custom drag source — same logic as egui's dnd_drag_source but without
// altering the widget's visual style (no white background on source).
// ─────────────────────────────────────────────────────────────────────────────

fn dnd_drag_source_transparent<Payload, R>(
    ui: &mut egui::Ui,
    id: Id,
    payload: Payload,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> InnerResponse<R>
where
    Payload: Any + Send + Sync,
{
    // `is_being_dragged` is true the INSTANT the mouse goes down — too eager.
    // We check if there is already an active drag payload, which only gets set
    // after Response::drag_started() fires (i.e., after the ~6 px threshold).
    let is_being_dragged = ui.memory(|mem| mem.is_being_dragged(id));
    let has_drag_payload = egui::DragAndDrop::has_any_payload(ui.ctx());

    // Only show the floating copy once a real drag is confirmed (payload set).
    if is_being_dragged && has_drag_payload {
        // Paint the floating copy at the cursor (tooltip layer)
        let layer_id = LayerId::new(Order::Tooltip, id);
        let InnerResponse { inner, response } = ui.with_layer_id(layer_id, add_contents);

        if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
            let delta = pointer_pos - response.rect.center();
            ui.ctx().translate_layer(layer_id, delta);
        }

        InnerResponse::new(inner, response)
    } else {
        // Render content normally — no style change
        let InnerResponse { inner, response } = ui.scope(add_contents);

        // Add an invisible drag-sense interaction on top of the content rect.
        // egui's Sense::drag() has a built-in distance threshold (~6 px)
        // so clicks and double-clicks will NOT trigger a drag.
        let dnd_response = ui.interact(response.rect, id, Sense::drag());

        // dnd_set_drag_payload internally gates on drag_started(),
        // which only fires after the threshold is met.
        dnd_response.dnd_set_drag_payload(payload);

        InnerResponse::new(inner, dnd_response | response)
    }
}
