use crate::app_state::AppState;
use crate::dsl;
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
            // Only visible if dragging and empty scene or specific intention
            let height = if state.scene.is_empty() { 28.0 } else { 4.0 };
            let (rect, _) =
                ui.allocate_at_least(egui::vec2(ui.available_width(), height), Sense::hover());

            if state.scene.is_empty() {
                let hovered = is_dragging && ui.rect_contains_pointer(rect);
                let stroke = if hovered {
                    egui::Stroke::new(1.5, Color32::LIGHT_BLUE)
                } else {
                    egui::Stroke::new(1.0, Color32::from_gray(60))
                };
                ui.painter().rect_stroke(rect, 4.0, stroke);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "📂 Drag & Drop elements here",
                    egui::FontId::proportional(11.0),
                    if hovered {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(120)
                    },
                );
            }
        });
        payload
    };

    if let Some(arc_path) = root_payload {
        let dragged = (*arc_path).clone();
        // If scene is empty, append. Otherwise, this small zone acts as "insert at 0"
        state.move_request = Some((dragged, vec![], 0));
    }

    // ── Animations Modal ──
    if state.show_animations_modal {
        egui::Window::new("Animations")
            .resizable(false)
            .default_size(egui::vec2(360.0, 180.0))
            .show(ui.ctx(), |ui| {
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
                    ui.label(format!("Target: {}", crate::scene::get_shape(&state.scene, p).map(|s| s.name().to_string()).unwrap_or("(unknown)".to_string())));
                } else if let Some(idx) = state.selected {
                    target_path = Some(vec![idx]);
                    ui.label(format!("Target: {}", state.scene.get(idx).map(|s| s.name().to_string()).unwrap_or("(unknown)".to_string())));
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
                ui.label("Available animations: Move (linear)");
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui.button("Add Move (linear)").clicked() {
                        if let Some(path) = target_path.clone() {
                            if let Some(shape) = crate::scene::get_shape_mut(&mut state.scene, &path) {
                                // new animation parameters (default: full project duration)
                                let start = 0.0f32;
                                let end = state.duration_secs;
                                // validate: animation start time (seconds) must be >= element.spawn_time()
                                let anim_start_secs = start; // start is stored in seconds
                                let spawn_secs = shape.spawn_time();
                                if anim_start_secs < spawn_secs {
                                    state.toast_message = Some(format!("Cannot add animation: starts at {:.2}s before element spawn at {:.2}s", anim_start_secs, spawn_secs));
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
                                    state.dsl_code = dsl::generate_dsl(
                                        &state.scene,
                                        state.render_width,
                                        state.render_height,
                                        state.fps,
                                        state.duration_secs,
                                    );
                                    crate::canvas::generate_preview_frames(state, state.time, ui.ctx());
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
    }

    ui.add_space(4.0);

    // ── Main scroll area ──
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut removals = Vec::new();
            let len = state.scene.len();

            // We no longer inject explicit spacer drop zones between items.
            // Instead, each render_node call handles its own drop logic (before/after/inside).
            for i in 0..len {
                render_node(ui, state, vec![i], &mut removals);
            }

            // We no longer need a dedicated append zone at the end,
            // because dropping on the bottom half of the last element handles appending.
            if len == 0 {
                // If the scene is empty, we might need a catch-all if the top "Root drop zone" isn't enough,
                // but the top zone handles empty scene specifically.
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
            state.position_cache = None; // scene mutated by move
            state.dsl_code = dsl::generate_dsl(
                &state.scene,
                state.render_width,
                state.render_height,
                state.fps,
                state.duration_secs,
            );
        }
    }

    // ── Bottom bar ──
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
                    let mut added = false;
                    if ui.button("📦  Group").clicked() {
                        state.scene.push(Shape::Group {
                            name: format!("Group #{}", state.scene.len()),
                            children: Vec::new(),
                            visible: true,
                        });
                        added = true;
                    }
                    if ui.button("⭕   Circle").clicked() {
                        state.scene.push(Shape::Circle {
                            name: format!("Circle #{}", state.scene.len()),
                            x: 0.5,
                            y: 0.5,
                            radius: 0.1,
                            color: [120, 200, 255, 255],
                            spawn_time: 0.0,
                            animations: Vec::new(),
                            visible: true,
                        });
                        state.position_cache = None; // scene changed
                        added = true;
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
                            animations: Vec::new(),
                            visible: true,
                        });
                        added = true;
                    }

                    if added {
                        state.dsl_code = dsl::generate_dsl(
                            &state.scene,
                            state.render_width,
                            state.render_height,
                            state.fps,
                            state.duration_secs,
                        );
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

    // ── Row content (without settings button) ──
    let render_row_content = |ui: &mut egui::Ui, state: &mut AppState| {
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
            state.dsl_code = dsl::generate_dsl(
                &state.scene,
                state.render_width,
                state.render_height,
                state.fps,
                state.duration_secs,
            );
        }

        // Name / Rename
        if is_renaming {
            let text_id = ui.make_persistent_id(("rename_text", &path));
            let available_width = ui.available_width().max(50.0);
            let res = ui.add(
                egui::TextEdit::singleline(&mut state.rename_buffer)
                    .id(text_id)
                    .desired_width(available_width)
                    .lock_focus(true),
            );
            // Request focus on first render when entering rename mode
            if !res.has_focus() {
                res.request_focus();
            }
            if res.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(shape) = crate::scene::get_shape_mut(&mut state.scene, &path) {
                    match shape {
                        Shape::Group { name, .. } => *name = state.rename_buffer.clone(),
                        Shape::Circle { name, .. } => *name = state.rename_buffer.clone(),
                        Shape::Rect { name, .. } => *name = state.rename_buffer.clone(),
                    }
                }
                state.renaming_path = None;
                state.dsl_code = dsl::generate_dsl(
                    &state.scene,
                    state.render_width,
                    state.render_height,
                    state.fps,
                    state.duration_secs,
                );
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

            let label_res = ui.add(egui::Label::new(job).selectable(false));

            // Selection highlight — subtle blue tint
            if is_selected {
                ui.painter().rect_filled(
                    label_res.rect.expand2(egui::vec2(20.0, 2.0)),
                    2.0,
                    Color32::from_rgba_premultiplied(60, 120, 200, 30),
                );
            }
        }
    };

    // ── Render node with drop zone support ──
    // Instead of separate spacer drop zones, we make the whole row a drop zone.
    // However, since we also want to be able to drag *this* row, we need a nested approach or
    // simply detect where the pointer is within the row (top half vs bottom half).

    // We wrap the entire block in a drop zone to detect "insert before/after" or "insert into group"
    let (zone_res, payload) = ui.dnd_drop_zone::<Vec<usize>>(Frame::none(), |ui| {
        if is_group {
            let coll_id = Id::new("group_collapsing").with(&path);
            let coll_state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                coll_id,
                false,
            );

            // We need to capture click/double-click from within the closure
            let group_clicked = std::cell::Cell::new(false);
            let group_double_clicked = std::cell::Cell::new(false);

            coll_state
                .show_header(ui, |ui| {
                    ui.horizontal(|ui| {
                        let drag_res =
                            dnd_drag_source_transparent(ui, drag_id, path.clone(), |ui| {
                                render_row_content(ui, state);
                            });
                        group_clicked.set(drag_res.response.clicked());
                        group_double_clicked.set(drag_res.response.double_clicked());

                        // Settings button OUTSIDE drag source
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("⚙").clicked() {
                                state.modifier_active_path = Some(path.clone());
                            }
                        });
                    });
                })
                .body(|ui| {
                    for i in 0..children_count {
                        let mut child_path = path.clone();
                        child_path.push(i);
                        render_node(ui, state, child_path, removals);
                    }

                    // Final append zone inside group (only needed if empty)
                    if children_count == 0 {
                        let (_, payload) = ui.dnd_drop_zone::<Vec<usize>>(Frame::none(), |ui| {
                            let (rect, _) = ui.allocate_at_least(
                                egui::vec2(ui.available_width(), 20.0),
                                Sense::hover(),
                            );
                            let is_dragging = ui.memory(|m| m.is_anything_being_dragged());
                            if is_dragging && ui.rect_contains_pointer(rect) {
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
                            } else if is_dragging {
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Empty Group",
                                    egui::FontId::proportional(10.0),
                                    Color32::from_gray(100),
                                );
                            }
                        });

                        if let Some(arc_path) = payload {
                            let from = (*arc_path).clone();
                            state.move_request = Some((from, path.clone(), 0));
                        }
                    }
                });

            if group_double_clicked.get() {
                state.renaming_path = Some(path.clone());
                state.rename_buffer = node_name.clone();
            } else if group_clicked.get() {
                state.selected_node_path = Some(path.clone());
            }
        } else {
            // Non-group node
            ui.horizontal(|ui| {
                let drag_res = dnd_drag_source_transparent(ui, drag_id, path.clone(), |ui| {
                    render_row_content(ui, state);
                });

                if drag_res.response.double_clicked() {
                    state.renaming_path = Some(path.clone());
                    state.rename_buffer = node_name.clone();
                } else if drag_res.response.clicked() {
                    state.selected_node_path = Some(path.clone());
                }

                // Settings button OUTSIDE drag source
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⚙").clicked() {
                        state.modifier_active_path = Some(path.clone());
                    }
                });
            });
        }
    });

    // Handle Drop Logic: Insert Before vs Insert After
    if let Some(arc_path) = payload {
        let from = (*arc_path).clone();

        // Calculate where in the rect the pointer is
        if let Some(pointer) = ui.ctx().pointer_hover_pos() {
            let rect = zone_res.rect;
            // If pointer is in top half, insert BEFORE this node.
            // If pointer is in bottom half, insert AFTER this node.

            // For groups, if we are hovering the header, we might want to insert INTO it (if it's empty?)
            // But simple rule: top 50% -> before, bottom 50% -> after.
            // CAREFUL: groups have bodies. We likely only want the logic to apply to the HEADER height if open.
            // For simplicity, let's treat the entire zone rect. If closed, fine. If open, rect covers children too?
            // ui.dnd_drop_zone wraps the entire block (header + body).

            // To fix "dragging over body triggers insert around group", we might want to only allow
            // drop-on-row logic if we are strictly over the header area or if it's a leaf.
            // But `zone_res.rect` for a group *includes* the open body.
            // That's why previously we used explicit spacers.

            // Refined Logic:
            // Insert-Before: top edge of rect
            // Insert-After: bottom edge of rect (or into body?)

            // Actually, explicit spacers ARE cleaner for strict tree ordering, but "horrible" visually.
            // Let's draw the indicator line overlayed on top of the node instead of allocating space.

            // We use the full `zone_res.rect` for detection, but we need to know if we are "in the list slot".
            // If we are over a group body, we might be over a child drop zone.
            // EGul's DND uses the innermost drop zone. So if we wrap the whole group, and children also have zones,
            // the children zones should take precedence.

            // Let's see: We are inside render_node, wrapping everything in `drop_zone`.
            // Inside `.body()`, we recurse `render_node`, which creates *another* `drop_zone`.
            // The deeper one should trigger first.

            // So, if this payload trigger matches *this* node's drop zone, we assume it bubbled up or matched here.

            // Determine index and parent
            if let Some(parent_path) = get_parent_path(&path) {
                // Sibling index
                let my_idx = path.last().copied().unwrap_or(0);

                // If dragging ONTO myself, ignore?
                // `from` != `path` check usually handled by move_node but good to skip logic.

                // Visual Indicator
                // Top 25% height -> Insert Before
                // Middle -> Make Child (if group)? Or just standard list behavior.
                // Bottom 25% -> Insert After

                // Since `zone_res.rect` might be huge (open group), we might want to restrict this logic
                // to the top 24px (header height) approximately.
                let header_height = 24.0; // approx
                let relative_y = pointer.y - rect.top();

                if is_group && relative_y > header_height {
                    // We are over the body but not over a specific child (padding area maybe).
                    // Let's default to "Append to this group"?
                    // `move_node` (from, path, children_count)
                    if from != path {
                        // state.move_request = Some((from, path.clone(), children_count));
                    }
                } else {
                    // We are over the header or it's a leaf.
                    // Split at % height
                    // Let's say if y < height/2 -> before, else -> after.
                    // Actually for open groups, "after" is ambiguous (after header = first child?).
                    // Let's stick to: Top half = Before (idx), Bottom Half = After (idx+1).
                    // If open group header top half -> before group. Bottom half -> insert as first child?

                    // Simplification:
                    // Top 50% of the *Header/Item Height* -> Insert Before (idx)
                    // Bottom 50% -> Insert After (idx + 1)

                    // We need the rect of just the content item, not the whole tree.
                    // Since we wrapped `coll_state.show_header` + body, we don't easily get the header rect alone from `zone_res`.
                    // But we can approximate.

                    let item_rect =
                        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 20.0)); // Assume ~20px row

                    let split_y = item_rect.center().y;

                    if pointer.y < split_y {
                        // Insert Before
                        state.move_request = Some((from, parent_path, my_idx));
                    } else {
                        // Insert After
                        // If it's a group and open, "Insert After" visually looks like "Insert as first child" usually
                        // BUT if we strictly mean sibling reordering:
                        state.move_request = Some((from, parent_path, my_idx + 1));
                    }
                }
            } else {
                // Is Root child
                let my_idx = path.last().copied().unwrap_or(0);
                let item_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 20.0));
                let split_y = item_rect.center().y;
                if pointer.y < split_y {
                    state.move_request = Some((from, vec![], my_idx));
                } else {
                    state.move_request = Some((from, vec![], my_idx + 1));
                }
            }
        }
    }

    // Draw Indicator if hovered
    if zone_res.contains_pointer() && ui.memory(|m| m.is_anything_being_dragged()) {
        let rect = zone_res.rect;
        // Same logic for position
        let item_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 20.0));
        if let Some(pointer) = ui.ctx().pointer_hover_pos() {
            let split_y = item_rect.center().y;
            let line_y = if pointer.y < split_y {
                item_rect.top()
            } else {
                item_rect.bottom()
            };

            ui.painter().line_segment(
                [
                    egui::pos2(rect.left(), line_y),
                    egui::pos2(rect.right(), line_y),
                ],
                egui::Stroke::new(2.0, Color32::from_rgb(100, 180, 255)),
            );
        }
    }
}

fn get_parent_path(path: &[usize]) -> Option<Vec<usize>> {
    if path.is_empty() {
        None
    } else {
        Some(path[0..path.len() - 1].to_vec())
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
    let is_being_dragged = ui.memory(|mem| mem.is_being_dragged(id));
    let has_drag_payload = egui::DragAndDrop::has_any_payload(ui.ctx());

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

        // Use click_and_drag so that clicks and double-clicks are NOT
        // swallowed by the drag interaction layer.
        let dnd_response = ui.interact(response.rect, id, Sense::click_and_drag());

        // Only set drag payload when an actual drag is started (after ~6 px threshold).
        dnd_response.dnd_set_drag_payload(payload);

        InnerResponse::new(inner, dnd_response | response)
    }
}
