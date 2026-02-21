/// Maneja las interacciones del usuario con el canvas: zoom, pan y selección de objetos.

use eframe::egui;
use crate::app_state::AppState;
use super::super::rasterizer::sample_color_at;

/// Procesa el zoom y pan del canvas según la entrada del ratón.
pub fn handle_pan_zoom(ui: &egui::Ui, state: &mut AppState, rect: egui::Rect, response: &egui::Response) {
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        state.canvas_pan_x += response.drag_delta().x;
        state.canvas_pan_y += response.drag_delta().y;
    }

    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let zoom_delta = (scroll * 0.002).exp();

            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                let zoom_before = state.canvas_zoom;
                state.canvas_zoom *= zoom_delta;
                state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
                let actual_delta = state.canvas_zoom / zoom_before;

                let center = rect.center();
                state.canvas_pan_x = (state.canvas_pan_x - (mouse_pos.x - center.x))
                    * actual_delta
                    + (mouse_pos.x - center.x);
                state.canvas_pan_y = (state.canvas_pan_y - (mouse_pos.y - center.y))
                    * actual_delta
                    + (mouse_pos.y - center.y);
            } else {
                state.canvas_zoom *= zoom_delta;
                state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
            }
}
    }
}

fn patch_dsl_property(code: &mut String, element_name: &str, prop: &str, val: f32) {
    println!("[DEBUG RESIZE] patch_dsl_property called for elem: '{}', prop: '{}', val: {}", element_name, prop, val);
    let mut in_target = false;
    let mut depth = 0;
    let mut new_lines = Vec::new();
    let mut patched = false;

    for line in code.lines() {
        let trimmed = line.trim();
        if !in_target {
            if trimmed.ends_with('{') {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let parsed_name = parts[1];
                    let expected_name1 = element_name;
                    let expected_name2 = format!("\"{}\"", element_name);
                    if parsed_name == expected_name1 || parsed_name == expected_name2.as_str() {
                        println!("[DEBUG RESIZE] Target block found!");
                        in_target = true;
                        depth = 1;
                    }
                }
            }
            new_lines.push(line.to_string());
        } else {
            let matches_open = trimmed.matches('{').count() as i32;
            let matches_close = trimmed.matches('}').count() as i32;
            let original_depth = depth;

            depth += matches_open;
            depth -= matches_close;

            let prefix1 = format!("{} =", prop);
            let prefix2 = format!("{}=", prop);

            if !patched && original_depth == 1 && (trimmed.starts_with(&prefix1) || trimmed.starts_with(&prefix2)) {
                let leading_ws_len = line.find(trimmed).unwrap_or(0);
                let leading_ws = &line[0..leading_ws_len];
                let suffix = if trimmed.ends_with(',') { "," } else { "" };
                new_lines.push(format!("{}{} = {:.3}{}", leading_ws, prop, val, suffix));
                patched = true;
                println!("[DEBUG RESIZE] property patched (replaced)");
            } else if original_depth == 1 && depth == 0 && !patched {
                new_lines.push(format!("\t{} = {:.3},", prop, val));
                new_lines.push(line.to_string());
                patched = true;
            } else {
                new_lines.push(line.to_string());
            }

            if depth <= 0 {
                in_target = false;
                println!("[DEBUG RESIZE] Exited target block. Patched: {}", patched);
            }
        }
    }

    if !patched {
        println!("[DEBUG RESIZE] Warning: failed to patch property!");
    }

    *code = new_lines.join("\n");
}

/// Maneja los clics en el canvas para seleccionar elementos o usar el cuentagotas.
pub fn handle_canvas_clicks(
    ui: &mut egui::Ui,
    state: &mut AppState,
    response: &egui::Response,
    composition_rect: egui::Rect,
    zoom: f32
) {
    // helper closure for rectangular shapes: returns true if `point` lies
    // within `threshold` pixels of any of the four edges of `rect`.
    // Circles are handled specially below so we keep this separate.
    // (we may not always use it, hence the leading underscore)
    let _rect_near_edge = |rect: egui::Rect, point: egui::Pos2, threshold: f32| {
        let horiz = (point.x - rect.left()).abs() <= threshold
            || (point.x - rect.right()).abs() <= threshold;
        let vert = (point.y - rect.top()).abs() <= threshold
            || (point.y - rect.bottom()).abs() <= threshold;
        (horiz && point.y >= rect.top() - threshold && point.y <= rect.bottom() + threshold)
            || (vert && point.x >= rect.left() - threshold && point.x <= rect.right() + threshold)
    };

    // use hover position to update cursor icon if we're close to a selectable
    // element.  this runs continuously while the pointer moves over the canvas.
    if state.resize_mode {
        if let Some(hover) = ui.input(|i| i.pointer.hover_pos()) {
            if composition_rect.contains(hover) {
                if let Some(path) = &state.selected_node_path {
                    if let Some(elem) = state.scene.get(path[0]) {
                        let frame_idx =
                            crate::shapes::element_store::seconds_to_frame(state.time, state.fps);
                        if let Some(shape) = elem.to_shape_at_frame(frame_idx, state.fps) {
                            // compute the bounding rect once, borrowing the shape
                            let rect = match &shape {
                                crate::scene::Shape::Rect(r) => {
                                    let centre = composition_rect.left_top()
                                        + egui::vec2(
                                            r.x * composition_rect.width(),
                                            r.y * composition_rect.height(),
                                        );
                                    let w = r.w * composition_rect.width();
                                    let h = r.h * composition_rect.height();
                                    egui::Rect::from_center_size(centre, egui::vec2(w, h))
                                }
                                crate::scene::Shape::Circle(c) => {
                                    let centre = composition_rect.left_top()
                                        + egui::vec2(
                                            c.x * composition_rect.width(),
                                            c.y * composition_rect.height(),
                                        );
                                    egui::Rect::from_center_size(
                                        centre,
                                        egui::vec2(
                                            2.0 * c.radius * composition_rect.width(),
                                            2.0 * c.radius * composition_rect.width(),
                                        ),
                                    )
                                }
                                crate::scene::Shape::Text(t) => {
                                    let centre = composition_rect.left_top()
                                        + egui::vec2(
                                            t.x * composition_rect.width(),
                                            t.y * composition_rect.height(),
                                        );
                                    let h = t.size * composition_rect.height();
                                    let w = t.value.len() as f32 * h * 0.5;
                                    egui::Rect::from_center_size(centre, egui::vec2(w, h))
                                }
                                _ => egui::Rect::EVERYTHING,
                            };
                            // determine which axes are near the pointer. circles
                            // always resize both.
                            let (h_flag, v_flag) = match &shape {
                                crate::scene::Shape::Circle(_) => (true, true),
                                _ => {
                                    let near_left = (hover.x - rect.left()).abs() <= 20.0;
                                    let near_right = (hover.x - rect.right()).abs() <= 20.0;
                                    let near_top = (hover.y - rect.top()).abs() <= 20.0;
                                    let near_bottom = (hover.y - rect.bottom()).abs() <= 20.0;
                                    ((near_left || near_right), (near_top || near_bottom))
                                }
                            };
                            let is_edge = h_flag || v_flag;
                            if is_edge {
                                ui.output_mut(|o| {
                                    o.cursor_icon = if h_flag && v_flag {
                                        egui::CursorIcon::ResizeNwSe
                                    } else if h_flag {
                                        egui::CursorIcon::ResizeHorizontal
                                    } else {
                                        egui::CursorIcon::ResizeVertical
                                    };
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(pos) = response.interact_pointer_pos() {
        if composition_rect.contains(pos) {
            // Handle resize mode: if the toggle is active we intercept
            // pointer events and adjust the dimensions of the selected
            // element based on the pointer position relative to the element
            // centre.  We treat the centre as fixed; dragging an edge moves
            // that edge outward/inward symmetrically.
            if state.resize_mode {
                // start a new resize drag if the user just clicked on an
                // edge (threshold band) and we don't already have an active
                // resize operation
                if state.resize_info.is_none() && ui.input(|i| i.pointer.primary_pressed()) {
                    println!("[DEBUG RESIZE] pointer pressed");
                    // ensure click was near the currently selected element's
                    // border before beginning
                    if let Some(path) = &state.selected_node_path {
                        if let Some(elem) = state.scene.get(path[0]) {
                            let frame_idx =
                                crate::shapes::element_store::seconds_to_frame(
                                    state.time,
                                    state.fps,
                                );
                            if let Some(shape) = elem.to_shape_at_frame(frame_idx, state.fps) {
                                let rect = match &shape {
                                    crate::scene::Shape::Rect(r) => {
                                        let centre = composition_rect.left_top()
                                            + egui::vec2(
                                                r.x * composition_rect.width(),
                                                r.y * composition_rect.height(),
                                            );
                                        let w = r.w * composition_rect.width();
                                        let h = r.h * composition_rect.height();
                                        egui::Rect::from_center_size(centre, egui::vec2(w, h))
                                    }
                                    crate::scene::Shape::Circle(c) => {
                                        let centre = composition_rect.left_top()
                                            + egui::vec2(
                                                c.x * composition_rect.width(),
                                                c.y * composition_rect.height(),
                                            );
                                        egui::Rect::from_center_size(
                                            centre,
                                            egui::vec2(
                                                2.0 * c.radius * composition_rect.width(),
                                                2.0 * c.radius * composition_rect.width(),
                                            ),
                                        )
                                    }
                                    crate::scene::Shape::Text(t) => {
                                        let centre = composition_rect.left_top()
                                            + egui::vec2(
                                                t.x * composition_rect.width(),
                                                t.y * composition_rect.height(),
                                            );
                                        let h = t.size * composition_rect.height();
                                        let w = t.value.len() as f32 * h * 0.5;
                                        egui::Rect::from_center_size(centre, egui::vec2(w, h))
                                    }
                                    _ => egui::Rect::EVERYTHING,
                                };
                                // determine which edges were hit; circles always
                                // resize both axes. capture individual side
                                // booleans so we know which edge to anchor.
                                let (h_flag, v_flag, left_hit, right_hit, top_hit, bottom_hit) =
                                    match &shape {
                                        crate::scene::Shape::Circle(_) =>
                                            (true, true, true, true, true, true),
                                        _ => {
                                            let near_left = (pos.x - rect.left()).abs() <= 20.0;
                                            let near_right = (pos.x - rect.right()).abs() <= 20.0;
                                            let near_top = (pos.y - rect.top()).abs() <= 20.0;
                                            let near_bottom = (pos.y - rect.bottom()).abs() <= 20.0;
                                            (
                                                near_left || near_right,
                                                near_top || near_bottom,
                                                near_left,
                                                near_right,
                                                near_top,
                                                near_bottom,
                                            )
                                        }
                                    };
                                let click_on_edge = h_flag || v_flag;
                                println!(
                                    "[DEBUG RESIZE] click_on_edge = {} (h={}, v={}, l={}, r={}, t={}, b={})",
                                    click_on_edge,
                                    h_flag,
                                    v_flag,
                                    left_hit,
                                    right_hit,
                                    top_hit,
                                    bottom_hit,
                                );
                                if click_on_edge {
                                    // compute centre coords now that we know the
                                    // click was valid
                                    let centre_opt = if let crate::scene::Shape::Rect(ref r) = shape {
                                        Some(
                                            composition_rect.left_top()
                                                + egui::vec2(
                                                    r.x * composition_rect.width(),
                                                    r.y * composition_rect.height(),
                                                ),
                                        )
                                    } else if let crate::scene::Shape::Circle(ref c) = shape {
                                        Some(
                                            composition_rect.left_top()
                                                + egui::vec2(
                                                    c.x * composition_rect.width(),
                                                    c.y * composition_rect.height(),
                                                ),
                                        )
                                    } else if let crate::scene::Shape::Text(ref t) = shape {
                                        Some(
                                            composition_rect.left_top()
                                                + egui::vec2(
                                                    t.x * composition_rect.width(),
                                                    t.y * composition_rect.height(),
                                                ),
                                        )
                                    } else {
                                        None
                                    };
                                    if let Some(centre) = centre_opt {
                                        state.resize_info = Some(crate::app_state::ResizeInfo {
                                            path: path.clone(),
                                            centre,
                                            horiz: h_flag,
                                            vert: v_flag,
                                            orig_w: if let crate::scene::Shape::Rect(ref r) = shape {
                                                Some(r.w)
                                            } else {
                                                None
                                            },
                                            orig_h: if let crate::scene::Shape::Rect(ref r) = shape {
                                                Some(r.h)
                                            } else {
                                                None
                                            },
                                            left: left_hit,
                                            right: right_hit,
                                            top: top_hit,
                                            bottom: bottom_hit,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }

                // if we have an active resize operation and the pointer is
                // down, update element dimensions
                if let Some(info) = state.resize_info.clone() {
                    if ui.input(|i| i.pointer.primary_down()) {
                        if let Some(cur_pos) = ui.input(|i| i.pointer.hover_pos()) {
                            // mutate the element based on pointer location
                            if let Some(elem) = state.scene.get_mut(info.path[0]) {
                                let frame_idx =
                                    crate::shapes::element_store::seconds_to_frame(
                                        state.time,
                                        state.fps,
                                    );
                                if let Some(shape) =
                                    elem.to_shape_at_frame(frame_idx, state.fps)
                                {
                                    match shape {
                                        crate::scene::Shape::Rect(_) => {
                                                println!("[DEBUG RESIZE] Dragging Rect");
                                            // recalc width/height based on anchor rather
                                            // than centre; when dragging one side we
                                            // hold the opposite edge fixed.  also
                                            // recompute centre accordingly unless shift
                                            // is pressed.
                                            let shift = ui.input(|i| i.modifiers.shift);
                                            let comp_w = composition_rect.width();
                                            let comp_h = composition_rect.height();
                                            let orig_w_px = info.orig_w.unwrap_or(0.0) * comp_w;
                                            let orig_h_px = info.orig_h.unwrap_or(0.0) * comp_h;
                                            // determine anchor pixel coordinates
                                            let anchor_x = if info.left && !info.right {
                                                // anchor at right edge
                                                info.centre.x + orig_w_px / 2.0
                                            } else if info.right && !info.left {
                                                // anchor at left edge
                                                info.centre.x - orig_w_px / 2.0
                                            } else {
                                                info.centre.x
                                            };
                                            let anchor_y = if info.top && !info.bottom {
                                                info.centre.y + orig_h_px / 2.0
                                            } else if info.bottom && !info.top {
                                                info.centre.y - orig_h_px / 2.0
                                            } else {
                                                info.centre.y
                                            };
                                            // compute new pixel widths/heights
                                            let mut width_px = orig_w_px;
                                            let mut height_px = orig_h_px;
                                            if info.horiz {
                                                if info.right && !info.left {
                                                    width_px = (cur_pos.x - anchor_x).abs();
                                                } else if info.left && !info.right {
                                                    width_px = (anchor_x - cur_pos.x).abs();
                                                } else {
                                                    // both sides or circle
                                                    width_px = 2.0 * (cur_pos.x - info.centre.x).abs();
                                                }
                                            }
                                            if info.vert {
                                                if info.bottom && !info.top {
                                                    height_px = (cur_pos.y - anchor_y).abs();
                                                } else if info.top && !info.bottom {
                                                    height_px = (anchor_y - cur_pos.y).abs();
                                                } else {
                                                    height_px = 2.0 * (cur_pos.y - info.centre.y).abs();
                                                }
                                            }
                                            let w_frac = width_px / comp_w;
                                            let h_frac = height_px / comp_h;
                                            // determine new centre if needed
                                            let mut new_centre = info.centre;
                                            if !shift {
                                                if info.horiz {
                                                    // when only one horizontal edge is
                                                    // being dragged, centre moves toward
                                                    // the cursor but the anchor stays
                                                    // fixed
                                                    if info.right && !info.left {
                                                        new_centre.x = anchor_x + width_px / 2.0;
                                                    } else if info.left && !info.right {
                                                        new_centre.x = anchor_x - width_px / 2.0;
                                                    }
                                                }
                                                if info.vert {
                                                    if info.bottom && !info.top {
                                                        new_centre.y = anchor_y + height_px / 2.0;
                                                    } else if info.top && !info.bottom {
                                                        new_centre.y = anchor_y - height_px / 2.0;
                                                    }
                                                }
                                            }
                                            // compute fractional x/y based on adjusted centre
                                            let mut x_frac: Option<f32> = None;
                                            let mut y_frac: Option<f32> = None;
                                            if !shift {
                                                if info.horiz && (info.left ^ info.right) {
                                                    x_frac = Some((new_centre.x - composition_rect.left()) / comp_w);
                                                }
                                                if info.vert && (info.top ^ info.bottom) {
                                                    y_frac = Some((new_centre.y - composition_rect.top()) / comp_h);
                                                }
                                            }

                                                // update the live element so the preview changes
                                                elem.insert_frame(
                                                    elem.spawn_frame,
                                                    crate::shapes::element_store::FrameProps {
                                                        x: x_frac,
                                                        y: y_frac,
                                                        radius: None,
                                                        w: Some(w_frac),
                                                        h: Some(h_frac),
                                                        size: None,
                                                        value: None,
                                                        color: None,
                                                        visible: None,
                                                        z_index: None,
                                                    },
                                                );
                                                // stash name while borrow lasts
                                                let elem_name = elem.name.clone();
                                                // after this point `elem` is no longer used, so the
                                                // mutable borrow ends and we can call methods that
                                                // borrow `state` again.
                                                state.request_dsl_update();
                                                crate::events::element_properties_changed_event::on_element_properties_changed(
                                                    state,
                                                );

                                                // also attempt to patch DSL text immediately for
                                                // a more responsive feel (fallback only)
                                                patch_dsl_property(&mut state.dsl_code, &elem_name, "width", w_frac);
                                                patch_dsl_property(&mut state.dsl_code, &elem_name, "height", h_frac);
                                                // ensure parser sees the modified code right away
                                                state.last_code_edit_time = Some(0.0);
                                                state.last_scene_parse_time = 0.0;
                                                state.autosave_pending = true;
                                            }
                                        crate::scene::Shape::Circle(_) => {
                                            println!("[DEBUG RESIZE] Dragging Circle");
                                            let dx = cur_pos.x - info.centre.x;
                                            let dy = cur_pos.y - info.centre.y;
                                            let r_frac =
                                                ((dx * dx + dy * dy).sqrt())
                                                    / composition_rect.width();

                                            elem.insert_frame(
                                                elem.spawn_frame,
                                                crate::shapes::element_store::FrameProps {
                                                    x: None,
                                                    y: None,
                                                    radius: Some(r_frac),
                                                    w: None,
                                                    h: None,
                                                    size: None,
                                                    value: None,
                                                    color: None,
                                                    visible: None,
                                                    z_index: None,
                                                },
                                            );
                                            let elem_name = elem.name.clone();
                                            state.request_dsl_update();
                                            crate::events::element_properties_changed_event::on_element_properties_changed(
                                                state,
                                            );
                                            patch_dsl_property(&mut state.dsl_code, &elem_name, "radius", r_frac);
                                            state.last_code_edit_time = Some(0.0);
                                            state.last_scene_parse_time = 0.0;
                                            state.autosave_pending = true;
                                        }
                                        crate::scene::Shape::Text(_) => {
                                            println!("[DEBUG RESIZE] Dragging Text");
                                            // map vertical distance to size fraction
                                            let size_frac =
                                                (cur_pos.y - info.centre.y).abs()
                                                    / composition_rect.height();

                                            elem.insert_frame(
                                                elem.spawn_frame,
                                                crate::shapes::element_store::FrameProps {
                                                    x: None,
                                                    y: None,
                                                    radius: None,
                                                    w: None,
                                                    h: None,
                                                    size: Some(size_frac),
                                                    value: None,
                                                    color: None,
                                                    visible: None,
                                                    z_index: None,
                                                },
                                            );
                                            let elem_name = elem.name.clone();
                                            state.request_dsl_update();
                                            crate::events::element_properties_changed_event::on_element_properties_changed(
                                                state,
                                            );
                                            patch_dsl_property(&mut state.dsl_code, &elem_name, "size", size_frac);
                                            state.last_code_edit_time = Some(0.0);
                                            state.last_scene_parse_time = 0.0;
                                            state.autosave_pending = true;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    } else {
                        // pointer released: end resize
                        state.resize_info = None;
                    }
                }

                // when in resize mode we skip the normal hit/selection path so
                // dragging doesn't also move the selection around
                return;
            }
            let paper_uv = egui::pos2(
                (pos.x - composition_rect.min.x) / composition_rect.width(),
                (pos.y - composition_rect.min.y) / composition_rect.height(),
            );

            if state.picker_active {
                let color = sample_color_at(state, paper_uv, state.time);
                let hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
                ui.output_mut(|o| o.copied_text = hex.clone());
                state.picker_color = color;
                state.toast_message = Some(format!("Color {} copiado al portapapeles!", hex));
                state.toast_type = crate::app_state::ToastType::Success;
                state.toast_deadline = ui.input(|i| i.time) + 3.0;
                state.picker_active = false;
            } else {
                // Realizar Hit Test para seleccionar formas
                let frame_idx = crate::shapes::element_store::seconds_to_frame(state.time, state.fps);
                let mut live_shapes: Vec<crate::scene::Shape> = Vec::with_capacity(state.scene.len());
                for elem in &state.scene {
                    if frame_idx >= elem.spawn_frame && elem.kill_frame.map_or(true, |k| frame_idx < k) {
                        if let Some(s) = elem.to_shape_at_frame(frame_idx, state.fps) {
                            live_shapes.push(s);
                        }
                    }
                }

                let hit_path = crate::shapes::shapes_manager::find_hit_path(
                    &live_shapes,
                    pos,
                    composition_rect,
                    zoom,
                    state.time,
                    0.0,
                    state.render_height,
                );
                if let Some(p) = hit_path {
                    state.selected = Some(p[0]);
                    state.selected_node_path = Some(p);
                } else {
                    state.selected = None;
                    state.selected_node_path = None;
                }
            }
        } else {
            state.selected = None;
        }
    }
}
