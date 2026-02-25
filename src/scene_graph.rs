use crate::app_state::AppState;
use crate::dsl;
use crate::shapes::element_store::ElementKeyframes;
use eframe::egui;
use eframe::egui::{Color32, Frame, Id, InnerResponse, LayerId, Order, Sense};
use std::any::Any;

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.heading(egui::RichText::new("Scene Graph").strong().size(16.0));
    });
    ui.add_space(8.0);

    // Root drop zone — visible only when scene is empty or something is being dragged.
    let root_payload = render_root_drop_zone(ui, state);
    if let Some(arc_path) = root_payload {
        if state.move_request.is_none() {
            state.move_request = Some(((*arc_path).clone(), vec![], 0));
        }
    }

    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let len = state.scene.len();
            for i in 0..len {
                render_node(ui, state, vec![i]);
            }
        });

    // Execute the queued move exactly once per frame.
    if let Some((from, _to_parent, to_idx)) = state.move_request.take() {
        // simple top-level move for flat ElementKeyframes list
        if !from.is_empty() {
            let src = from[0];
            if src < state.scene.len() {
                let node = state.scene.remove(src);
                let insert_at = to_idx.min(state.scene.len());
                state.scene.insert(insert_at, node);
                state.selected_node_path = Some(vec![insert_at]);
                // position cache removed — no-op
                state.dsl_code = dsl::generate_dsl_from_elements(
                    &state.scene,
                    state.render_width,
                    state.render_height,
                    state.fps,
                    state.duration_secs,
                );
                crate::events::element_properties_changed_event::on_element_properties_changed(
                    state,
                );
            }
        }
    }

    // Bottom bar.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("➕ Elements").clicked() {
                state.show_elements_modal = true;
            }
            ui.add_space(8.0);
            if ui.button("➕ Animations").clicked() {
                state.show_animations_modal = true;
            }
        });
    });

    show_elements_modal(ui, state);
}

// ─────────────────────────────────────────────────────────────────────────────
// Root drop zone
// ─────────────────────────────────────────────────────────────────────────────

fn render_root_drop_zone(
    ui: &mut egui::Ui,
    state: &AppState,
) -> Option<std::sync::Arc<Vec<usize>>> {
    let is_dragging = ui.memory(|m| m.is_anything_being_dragged());
    let height = if state.scene.is_empty() { 28.0 } else { 4.0 };

    let (_response, payload) = ui.dnd_drop_zone::<Vec<usize>>(Frame::none(), |ui| {
        let (rect, _) =
            ui.allocate_at_least(egui::vec2(ui.available_width(), height), Sense::hover());

        if state.scene.is_empty() {
            let hovered = is_dragging && ui.rect_contains_pointer(rect);
            let stroke_color = if hovered {
                Color32::LIGHT_BLUE
            } else {
                Color32::from_gray(60)
            };
            let text_color = if hovered {
                Color32::WHITE
            } else {
                Color32::from_gray(120)
            };

            ui.painter()
                .rect_stroke(rect, 4.0, egui::Stroke::new(1.5, stroke_color));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "📂 Drag & Drop elements here",
                egui::FontId::proportional(11.0),
                text_color,
            );
        }
    });

    payload
}

// ─────────────────────────────────────────────────────────────────────────────
// Node dispatch — decides between group and leaf rendering
// ─────────────────────────────────────────────────────────────────────────────

fn render_node(ui: &mut egui::Ui, state: &mut AppState, path: Vec<usize>) {
    let element = match state.scene.get(path[0]) {
        Some(e) => e,
        None => return,
    };

    // ElementKeyframes are a flat list for now; groups are represented by
    // elements with kind == "group" but child nesting isn't supported yet.
    let is_group = element.kind == "group";
    let children_count = 0usize;
    let node_name = element.name.clone();
    let is_visible = element.visible.first().map(|kf| kf.value).unwrap_or(true);
    let (icon, icon_color) = element_icon(element);

    let is_selected = state.selected_node_path.as_ref() == Some(&path);
    let is_renaming = state.renaming_path.as_ref() == Some(&path);
    let drag_id = Id::new("scene_drag").with(&path);

    let (zone_res, drop_payload) = ui.dnd_drop_zone::<Vec<usize>>(Frame::none(), |ui| {
        if is_group {
            render_group_node(
                ui,
                state,
                &path,
                drag_id,
                &node_name,
                icon,
                icon_color,
                is_visible,
                is_selected,
                is_renaming,
                children_count,
            );
        } else {
            render_leaf_node(
                ui,
                state,
                &path,
                drag_id,
                &node_name,
                icon,
                icon_color,
                is_visible,
                is_selected,
                is_renaming,
            );
        }
    });

    handle_drop(ui, state, &path, drop_payload, &zone_res);
    draw_drop_indicator(ui, &zone_res);
}

// ─────────────────────────────────────────────────────────────────────────────
// Group node
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn render_group_node(
    ui: &mut egui::Ui,
    state: &mut AppState,
    path: &[usize],
    drag_id: Id,
    node_name: &str,
    icon: &str,
    icon_color: Color32,
    is_visible: bool,
    is_selected: bool,
    is_renaming: bool,
    children_count: usize,
) {
    let coll_id = Id::new("group_collapsing").with(path);
    let coll_state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), coll_id, false);

    let mut clicked = false;
    let mut double_clicked = false;

    coll_state
        .show_header(ui, |ui| {
            ui.horizontal(|ui| {
                let drag_res = drag_source(ui, drag_id, path.to_vec(), |ui| {
                    render_row(
                        ui,
                        state,
                        path,
                        node_name,
                        icon,
                        icon_color,
                        is_visible,
                        is_selected,
                        is_renaming,
                    );
                });
                clicked = drag_res.response.clicked();
                double_clicked = drag_res.response.double_clicked();

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⚙").clicked() {
                        state.modifier_active_path = Some(path.to_vec());
                    }
                });
            });
        })
        .body(|ui| {
            for i in 0..children_count {
                let mut child_path = path.to_vec();
                child_path.push(i);
                render_node(ui, state, child_path);
            }

            // Empty-group drop zone so items can still be dropped into it.
            if children_count == 0 {
                let (_r, payload) = ui.dnd_drop_zone::<Vec<usize>>(Frame::none(), |ui| {
                    let (rect, _) = ui
                        .allocate_at_least(egui::vec2(ui.available_width(), 20.0), Sense::hover());
                    if ui.memory(|m| m.is_anything_being_dragged())
                        && ui.rect_contains_pointer(rect)
                    {
                        ui.painter().rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(1.5, Color32::LIGHT_BLUE),
                        );
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Drop inside group",
                            egui::FontId::proportional(10.0),
                            Color32::WHITE,
                        );
                    }
                });
                if let Some(arc) = payload {
                    if state.move_request.is_none() {
                        state.move_request = Some(((*arc).clone(), path.to_vec(), 0));
                    }
                }
            }
        });

    if double_clicked {
        state.renaming_path = Some(path.to_vec());
        state.rename_buffer = node_name.to_string();
    } else if clicked {
        state.selected_node_path = Some(path.to_vec());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Leaf node
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn render_leaf_node(
    ui: &mut egui::Ui,
    state: &mut AppState,
    path: &[usize],
    drag_id: Id,
    node_name: &str,
    icon: &str,
    icon_color: Color32,
    is_visible: bool,
    is_selected: bool,
    is_renaming: bool,
) {
    ui.horizontal(|ui| {
        let drag_res = drag_source(ui, drag_id, path.to_vec(), |ui| {
            render_row(
                ui,
                state,
                path,
                node_name,
                icon,
                icon_color,
                is_visible,
                is_selected,
                is_renaming,
            );
        });

        if drag_res.response.double_clicked() {
            state.renaming_path = Some(path.to_vec());
            state.rename_buffer = node_name.to_string();
        } else if drag_res.response.clicked() {
            state.selected_node_path = Some(path.to_vec());
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("⚙").clicked() {
                state.modifier_active_path = Some(path.to_vec());
            }
        });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Row content — visibility toggle + name label / rename field
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn render_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    path: &[usize],
    node_name: &str,
    icon: &str,
    icon_color: Color32,
    is_visible: bool,
    is_selected: bool,
    is_renaming: bool,
) {
    ui.spacing_mut().item_spacing.x = 4.0;

    // Visibility toggle.
    let vis_icon = if is_visible { "👁" } else { "🚫" };
    let vis_color = if is_visible {
        Color32::WHITE
    } else {
        Color32::GRAY
    };
    if ui
        .add(egui::Button::new(egui::RichText::new(vis_icon).small().color(vis_color)).frame(false))
        .clicked()
    {
        if let Some(elem) = state.scene.get_mut(path[0]) {
            // toggle visibility by inserting a hold keyframe at spawn
            let new_vis = !is_visible;
            elem.visible.clear();
            elem.visible.push(crate::shapes::element_store::Keyframe {
                frame: elem.spawn_frame,
                value: new_vis,
                easing: crate::animations::easing::Easing::Linear,
            });
        }
        state.dsl_code = dsl::generate_dsl_from_elements(
            &state.scene,
            state.render_width,
            state.render_height,
            state.fps,
            state.duration_secs,
        );
        crate::events::element_properties_changed_event::on_element_properties_changed(state);
    }

    // Name display or inline rename field.
    if is_renaming {
        let text_id = ui.make_persistent_id(("rename_text", path));
        let res = ui.add(
            egui::TextEdit::singleline(&mut state.rename_buffer)
                .id(text_id)
                .desired_width(ui.available_width().max(50.0))
                .lock_focus(true),
        );
        if !res.has_focus() {
            res.request_focus();
        }
        if res.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(elem) = state.scene.get_mut(path[0]) {
                elem.name = state.rename_buffer.clone();
            }
            state.renaming_path = None;
            state.dsl_code = dsl::generate_dsl_from_elements(
                &state.scene,
                state.render_width,
                state.render_height,
                state.fps,
                state.duration_secs,
            );
            crate::events::element_properties_changed_event::on_element_properties_changed(state);
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
            &format!(" {node_name}"),
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
        // append spawn/kill range (subtle) and ephemeral badge — use ElementKeyframes
        if let Some(elem) = state.scene.get(path[0]) {
            if elem.ephemeral {
                job.append(
                    " ⚡",
                    0.0,
                    egui::TextFormat {
                        color: Color32::from_rgb(220, 200, 80),
                        ..Default::default()
                    },
                );
            }

            let sp = elem.spawn_frame as f32 / state.fps as f32;
            let range = if let Some(kf) = elem.kill_frame {
                format!("  ({:.2}–{:.2})", sp, kf as f32 / state.fps as f32)
            } else {
                format!("  ({:.2}– )", sp)
            };
            job.append(
                &range,
                0.0,
                egui::TextFormat {
                    color: Color32::from_gray(120),
                    ..Default::default()
                },
            );
        }
        let label_res = ui.add(egui::Label::new(job).selectable(false));
        if is_selected {
            ui.painter().rect_filled(
                label_res.rect.expand2(egui::vec2(20.0, 2.0)),
                2.0,
                Color32::from_rgba_premultiplied(60, 120, 200, 30),
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Drop handling — only the innermost (first-queued) drop wins per frame
// ─────────────────────────────────────────────────────────────────────────────

fn handle_drop(
    ui: &egui::Ui,
    state: &mut AppState,
    path: &[usize],
    drop_payload: Option<std::sync::Arc<Vec<usize>>>,
    zone_res: &egui::Response,
) {
    let Some(arc) = drop_payload else { return };
    if state.move_request.is_some() {
        return; // Another zone already claimed this frame's move.
    }

    let from = (*arc).clone();
    let my_idx = path.last().copied().unwrap_or(0);
    let parent_path = path[..path.len().saturating_sub(1)].to_vec();

    let insert_before = ui
        .ctx()
        .pointer_hover_pos()
        .map(|p| p.y < zone_res.rect.center().y)
        .unwrap_or(false);

    let to_idx = if insert_before { my_idx } else { my_idx + 1 };
    state.move_request = Some((from, parent_path, to_idx));
}

// ─────────────────────────────────────────────────────────────────────────────
// Drop indicator line
// ─────────────────────────────────────────────────────────────────────────────

fn draw_drop_indicator(ui: &egui::Ui, zone_res: &egui::Response) {
    if !ui.memory(|m| m.is_anything_being_dragged()) || !zone_res.contains_pointer() {
        return;
    }
    let rect = zone_res.rect;
    let Some(pointer) = ui.ctx().pointer_hover_pos() else {
        return;
    };
    // Split at the top-row boundary (~20 px) so the indicator stays near the header
    // even when the zone wraps an expanded group body.
    let row_bottom = rect.top() + 20.0;
    let line_y = if pointer.y < rect.top() + 10.0 {
        rect.top()
    } else {
        row_bottom
    };

    ui.painter().line_segment(
        [
            egui::pos2(rect.left(), line_y),
            egui::pos2(rect.right(), line_y),
        ],
        egui::Stroke::new(2.0, Color32::from_rgb(100, 180, 255)),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Elements modal
// ─────────────────────────────────────────────────────────────────────────────

fn show_elements_modal(ui: &mut egui::Ui, state: &mut AppState) {
    if !state.show_elements_modal {
        return;
    }
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

            let mut added = false;
            if ui.button("📦  Group").clicked() {
                let mut ek =
                    ElementKeyframes::new(format!("Group #{}", state.scene.len()), "group".into());
                ek.spawn_frame = 0;
                ek.visible.push(crate::shapes::element_store::Keyframe {
                    frame: 0,
                    value: true,
                    easing: crate::animations::easing::Easing::Linear,
                });
                state.scene.push(ek);
                added = true;
            }
            if ui.button("🔤  Text").clicked() {
                if let Some(ek) = crate::shapes::shapes_manager::create_default_by_keyword(
                    "text",
                    format!("Text #{}", state.scene.len()),
                )
                .and_then(|s| {
                    crate::shapes::element_store::ElementKeyframes::from_shape_at_spawn(
                        &s, state.fps,
                    )
                }) {
                    state.scene.push(ek);
                }
                added = true;
            }
            if added {
                state.dsl_code = dsl::generate_dsl_from_elements(
                    &state.scene,
                    state.render_width,
                    state.render_height,
                    state.fps,
                    state.duration_secs,
                );
                crate::events::element_properties_changed_event::on_element_properties_changed(
                    state,
                );
                state.show_elements_modal = false;
            }
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn element_icon(elem: &ElementKeyframes) -> (&'static str, Color32) {
    match elem.kind.as_str() {
        "group" => ("📦", Color32::from_rgb(255, 200, 100)),
        "text" => ("🔤", Color32::from_rgb(200, 255, 100)),
        _ => ("❓", Color32::from_gray(180)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Drag source — ghost follows cursor, no white-background override
// ─────────────────────────────────────────────────────────────────────────────

fn drag_source<Payload, R>(
    ui: &mut egui::Ui,
    id: Id,
    payload: Payload,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> InnerResponse<R>
where
    Payload: Any + Send + Sync,
{
    let is_being_dragged = ui.memory(|mem| mem.is_being_dragged(id));

    if is_being_dragged && egui::DragAndDrop::has_any_payload(ui.ctx()) {
        // Render floating ghost at cursor.
        let layer_id = LayerId::new(Order::Tooltip, id);
        let InnerResponse { inner, response } = ui.with_layer_id(layer_id, add_contents);
        if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
            let delta = pointer_pos - response.rect.center();
            ui.ctx().translate_layer(layer_id, delta);
        }
        InnerResponse::new(inner, response)
    } else {
        let InnerResponse { inner, response } = ui.scope(add_contents);
        let dnd_response = ui.interact(response.rect, id, Sense::click_and_drag());
        dnd_response.dnd_set_drag_payload(payload);
        InnerResponse::new(inner, dnd_response | response)
    }
}
