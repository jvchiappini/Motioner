use super::super::rasterizer::sample_color_at;
use crate::app_state::AppState;
/// Maneja las interacciones del usuario con el canvas: zoom, pan y selección de objetos.
use eframe::egui;

/// Procesa el zoom y pan del canvas según la entrada del ratón.
pub fn handle_pan_zoom(
    ui: &egui::Ui,
    state: &mut AppState,
    rect: egui::Rect,
    response: &egui::Response,
) {
    // 1. Lógica de Paneo (Pan)
    if response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle)
    {
        state.canvas_pan_x += response.drag_delta().x;
        state.canvas_pan_y += response.drag_delta().y;
    }

    // 2. Lógica de Zoom (Restaurada)
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
                state.canvas_pan_x = (state.canvas_pan_x - (mouse_pos.x - center.x)) * actual_delta
                    + (mouse_pos.x - center.x);
                state.canvas_pan_y = (state.canvas_pan_y - (mouse_pos.y - center.y)) * actual_delta
                    + (mouse_pos.y - center.y);
            } else {
                state.canvas_zoom *= zoom_delta;
                state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
            }
        }
    }
}

/// Función auxiliar para actualizar el código DSL de forma robusta
fn patch_dsl_property(code: &mut String, element_name: &str, prop: &str, val: f32) {
    let mut new_lines = Vec::new();
    let mut patched = false;
    let target_name_quoted = format!("\"{}\"", element_name);

    let mut inside_target = false;

    for line in code.lines() {
        let trimmed = line.trim();

        // Detectar entrada al bloque del elemento buscando: keyword "Name" {
        if !inside_target && trimmed.contains('{') {
            let parts: Vec<&str> = trimmed
                .split(|c: char| c.is_whitespace() || c == '(' || c == '"' || c == '{')
                .filter(|s| !s.is_empty())
                .collect();
            if parts.contains(&element_name) {
                inside_target = true;
            }
        }

        // Si estamos dentro del elemento, buscamos la propiedad
        if inside_target {
            // Buscamos una coincidencia exacta de la propiedad al inicio (prop = ...)
            let is_prop_match = trimmed.starts_with(prop) && {
                let after = &trimmed[prop.len()..];
                let after_trimmed = after.trim();
                after_trimmed.starts_with('=') || after_trimmed.is_empty()
            };

            if is_prop_match {
                // Preservamos la indentación (buscando el primer carácter no espacio)
                let first_char_idx = line.find(|c: char| !c.is_whitespace()).unwrap_or(0);
                let indentation = &line[0..first_char_idx];
                new_lines.push(format!("{}{} = {:.3},", indentation, prop, val));
                patched = true;
                continue;
            }

            if trimmed == "}" {
                inside_target = false;
            }
        }
        new_lines.push(line.to_string());
    }

    *code = new_lines.join("\n");
}

/// Maneja los clics en el canvas para seleccionar, mover y redimensionar elementos.
pub fn handle_canvas_clicks(
    ui: &mut egui::Ui,
    state: &mut AppState,
    response: &egui::Response,
    composition_rect: egui::Rect,
    zoom: f32,
) {
    // 1. Actualizar cursor si estamos en modo Resize
    if state.resize_mode {
        if let Some(hover) = ui.input(|i| i.pointer.hover_pos()) {
            if composition_rect.contains(hover) {
                if let Some(path) = &state.selected_node_path {
                    if let Some(elem) = state.scene.get(path[0]) {
                        let frame_idx =
                            crate::shapes::element_store::seconds_to_frame(state.time, state.fps);
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

    // 2. Actualizar cursor si estamos en modo Move
    if state.move_mode {
        if let Some(hover) = ui.input(|i| i.pointer.hover_pos()) {
            if composition_rect.contains(hover) {
                if let Some(path) = &state.selected_node_path {
                    if let Some(elem) = state.scene.get(path[0]) {
                        if let Some(shape) = elem.to_shape_at_frame(
                            crate::shapes::element_store::seconds_to_frame(state.time, state.fps),
                            state.fps,
                        ) {
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
                            if rect.contains(hover) {
                                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Move);
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Manejo de Clics e Interacciones
    if let Some(pos) = response.interact_pointer_pos() {
        if composition_rect.contains(pos) {
            // --- RESIZE MODE ---
            if state.resize_mode {
                if state.resize_info.is_none() && ui.input(|i| i.pointer.primary_pressed()) {
                    if let Some(path) = &state.selected_node_path {
                        if let Some(elem) = state.scene.get(path[0]) {
                            let frame_idx = crate::shapes::element_store::seconds_to_frame(
                                state.time, state.fps,
                            );
                            if let Some(shape) = elem.to_shape_at_frame(frame_idx, state.fps) {
                                let rect = match &shape {
                                    crate::scene::Shape::Rect(r) => {
                                        let centre = composition_rect.left_top()
                                            + egui::vec2(
                                                r.x * composition_rect.width(),
                                                r.y * composition_rect.height(),
                                            );
                                        egui::Rect::from_center_size(
                                            centre,
                                            egui::vec2(
                                                r.w * composition_rect.width(),
                                                r.h * composition_rect.height(),
                                            ),
                                        )
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
                                        egui::Rect::from_center_size(
                                            centre,
                                            egui::vec2(t.value.len() as f32 * h * 0.5, h),
                                        )
                                    }
                                    _ => egui::Rect::EVERYTHING,
                                };

                                let (h_flag, v_flag, left_hit, right_hit, top_hit, bottom_hit) =
                                    match &shape {
                                        crate::scene::Shape::Circle(_) => {
                                            (true, true, true, true, true, true)
                                        }
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

                                if h_flag || v_flag {
                                    let centre = rect.center(); // Simplificación para obtener el centro
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
                                        orig_x: if let crate::scene::Shape::Rect(ref r) = shape {
                                            Some(r.x)
                                        } else {
                                            None
                                        },
                                        orig_y: if let crate::scene::Shape::Rect(ref r) = shape {
                                            Some(r.y)
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

                if let Some(info) = state.resize_info.clone() {
                    if ui.input(|i| i.pointer.primary_down()) {
                        if let Some(cur_pos) = ui.input(|i| i.pointer.hover_pos()) {
                            if let Some(elem) = state.scene.get_mut(info.path[0]) {
                                let frame_idx = crate::shapes::element_store::seconds_to_frame(
                                    state.time, state.fps,
                                );
                                if let Some(shape) = elem.to_shape_at_frame(frame_idx, state.fps) {
                                    match shape {
                                        crate::scene::Shape::Rect(_) => {
                                            let comp_w = composition_rect.width();
                                            let comp_h = composition_rect.height();
                                            let is_shift = ui.input(|i| i.modifiers.shift);

                                            let mut x_frac = None;
                                            let mut y_frac = None;
                                            let mut w_frac = None;
                                            let mut h_frac = None;

                                            if is_shift {
                                                // Symmetrical (mirror) resize from center
                                                if info.horiz {
                                                    let new_w_px =
                                                        (cur_pos.x - info.centre.x).abs() * 2.0;
                                                    w_frac = Some(new_w_px / comp_w);
                                                }
                                                if info.vert {
                                                    let new_h_px =
                                                        (cur_pos.y - info.centre.y).abs() * 2.0;
                                                    h_frac = Some(new_h_px / comp_h);
                                                }
                                            } else {
                                                // Asymmetrical resize (anchors the opposite edge)
                                                if let (Some(orig_x), Some(orig_w)) =
                                                    (info.orig_x, info.orig_w)
                                                {
                                                    if info.right {
                                                        let left_edge_frac = orig_x - orig_w / 2.0;
                                                        let cur_r_frac = (cur_pos.x
                                                            - composition_rect.left())
                                                            / comp_w;
                                                        let w = (cur_r_frac - left_edge_frac)
                                                            .max(0.001);
                                                        w_frac = Some(w);
                                                        x_frac = Some(left_edge_frac + w / 2.0);
                                                    } else if info.left {
                                                        let right_edge_frac = orig_x + orig_w / 2.0;
                                                        let cur_l_frac = (cur_pos.x
                                                            - composition_rect.left())
                                                            / comp_w;
                                                        let w = (right_edge_frac - cur_l_frac)
                                                            .max(0.001);
                                                        w_frac = Some(w);
                                                        x_frac = Some(right_edge_frac - w / 2.0);
                                                    }
                                                }

                                                if let (Some(orig_y), Some(orig_h)) =
                                                    (info.orig_y, info.orig_h)
                                                {
                                                    if info.bottom {
                                                        let top_edge_frac = orig_y - orig_h / 2.0;
                                                        let cur_b_frac = (cur_pos.y
                                                            - composition_rect.top())
                                                            / comp_h;
                                                        let h =
                                                            (cur_b_frac - top_edge_frac).max(0.001);
                                                        h_frac = Some(h);
                                                        y_frac = Some(top_edge_frac + h / 2.0);
                                                    } else if info.top {
                                                        let bottom_edge_frac =
                                                            orig_y + orig_h / 2.0;
                                                        let cur_t_frac = (cur_pos.y
                                                            - composition_rect.top())
                                                            / comp_h;
                                                        let h = (bottom_edge_frac - cur_t_frac)
                                                            .max(0.001);
                                                        h_frac = Some(h);
                                                        y_frac = Some(bottom_edge_frac - h / 2.0);
                                                    }
                                                }
                                            }

                                            elem.insert_frame(
                                                elem.spawn_frame,
                                                crate::shapes::element_store::FrameProps {
                                                    x: x_frac,
                                                    y: y_frac,
                                                    radius: None,
                                                    w: w_frac,
                                                    h: h_frac,
                                                    size: None,
                                                    value: None,
                                                    color: None,
                                                    visible: None,
                                                    z_index: None,
                                                },
                                            );

                                            let elem_name = elem.name.clone();
                                            // Update DSL chain with only the properties that changed
                                            if let Some(w) = w_frac {
                                                patch_dsl_property(
                                                    &mut state.dsl_code,
                                                    &elem_name,
                                                    "width",
                                                    w,
                                                );
                                            }
                                            if let Some(h) = h_frac {
                                                patch_dsl_property(
                                                    &mut state.dsl_code,
                                                    &elem_name,
                                                    "height",
                                                    h,
                                                );
                                            }
                                            if let Some(x) = x_frac {
                                                patch_dsl_property(
                                                    &mut state.dsl_code,
                                                    &elem_name,
                                                    "x",
                                                    x,
                                                );
                                            }
                                            if let Some(y) = y_frac {
                                                patch_dsl_property(
                                                    &mut state.dsl_code,
                                                    &elem_name,
                                                    "y",
                                                    y,
                                                );
                                            }

                                            state.autosave.mark_dirty(ui.input(|i| i.time));
                                        }
                                        crate::scene::Shape::Circle(_) => {
                                            let dx = cur_pos.x - info.centre.x;
                                            let dy = cur_pos.y - info.centre.y;
                                            let r_frac = (dx * dx + dy * dy).sqrt()
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
                                            patch_dsl_property(
                                                &mut state.dsl_code,
                                                &elem_name,
                                                "radius",
                                                r_frac,
                                            );
                                            state.autosave.mark_dirty(ui.input(|i| i.time));
                                        }
                                        crate::scene::Shape::Text(_) => {
                                            let size_frac = (cur_pos.y - info.centre.y).abs()
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
                                            patch_dsl_property(
                                                &mut state.dsl_code,
                                                &elem_name,
                                                "size",
                                                size_frac,
                                            );
                                            state.autosave.mark_dirty(ui.input(|i| i.time));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    } else {
                        // Mouse release - Forzamos guardado final
                        state.request_dsl_update();
                        // force immediate write
                        state.autosave.last_edit_time = Some(0.0);
                        state.resize_info = None;
                    }
                }
                return; // Interceptamos y salimos
            }

            // --- MOVE MODE ---
            if state.move_mode {
                if state.move_info.is_none() && ui.input(|i| i.pointer.primary_pressed()) {
                    let maybe_path = state
                        .selected_node_path
                        .clone()
                        .or_else(|| state.selected.map(|i| vec![i]));
                    if let Some(path) = &maybe_path {
                        if let Some(elem) = state.scene.get(path[0]) {
                            let frame_idx = crate::shapes::element_store::seconds_to_frame(
                                state.time, state.fps,
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
                                    _ => egui::Rect::NOTHING,
                                };

                                if rect.contains(pos) {
                                    // Detectar si el clic fue en una de las flechas de eje para el movimiento restringido.
                                    // Las flechas se dibujan desde el centro del objeto: Rojo (X), Verde (Y)
                                    let c = rect.center();
                                    let arrow_len = 60.0;
                                    let hit_dist = 10.0; // Tolerancia de clic

                                    // Rectángulo de la flecha X (Eje horizontal)
                                    let x_arrow_rect = egui::Rect::from_min_max(
                                        egui::pos2(c.x, c.y - 12.0),
                                        egui::pos2(c.x + arrow_len, c.y + 12.0),
                                    );
                                    // Rectángulo de la flecha Y (Eje vertical)
                                    let y_arrow_rect = egui::Rect::from_min_max(
                                        egui::pos2(c.x - 12.0, c.y),
                                        egui::pos2(c.x + 12.0, c.y + arrow_len),
                                    );

                                    let hit_x = x_arrow_rect.contains(pos);
                                    let hit_y = y_arrow_rect.contains(pos);

                                    state.move_info = Some(crate::app_state::MoveInfo {
                                        path: path.clone(),
                                        centre: rect.center(),
                                        start_pos: pos,
                                        axis_x: hit_x || (!hit_y), // Si no toca ninguna, movemos ambos (comportamiento base) o si toca solo X
                                        axis_y: hit_y || (!hit_x), // Si toca Y o ninguna
                                    });
                                }
                            }
                        }
                    }
                }

                if let Some(info) = state.move_info.clone() {
                    if ui.input(|i| i.pointer.primary_down()) {
                        let dx = if info.axis_x {
                            pos.x - info.start_pos.x
                        } else {
                            0.0
                        };
                        let dy = if info.axis_y {
                            pos.y - info.start_pos.y
                        } else {
                            0.0
                        };
                        let comp_w = composition_rect.width();
                        let comp_h = composition_rect.height();

                        let new_centre = info.centre + egui::vec2(dx, dy);
                        let x_frac = (new_centre.x - composition_rect.left()) / comp_w;
                        let y_frac = (new_centre.y - composition_rect.top()) / comp_h;

                        if let Some(elem) = state.scene.get_mut(info.path[0]) {
                            elem.insert_frame(
                                elem.spawn_frame,
                                crate::shapes::element_store::FrameProps {
                                    x: Some(x_frac),
                                    y: Some(y_frac),
                                    radius: None,
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
                            patch_dsl_property(&mut state.dsl_code, &elem_name, "x", x_frac);
                            patch_dsl_property(&mut state.dsl_code, &elem_name, "y", y_frac);
                            state.autosave.mark_dirty(ui.input(|i| i.time));
                        }
                    } else {
                        // Mouse release - Forzamos guardado final
                        state.request_dsl_update();
                        state.autosave.last_edit_time = Some(0.0);
                        state.move_info = None;
                    }
                }
                return; // Interceptamos y salimos
            }

            // --- COLOR PICKER / SELECCIÓN NORMAL ---
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
                // Hit Test Normal
                let frame_idx =
                    crate::shapes::element_store::seconds_to_frame(state.time, state.fps);
                let mut live_shapes: Vec<crate::scene::Shape> =
                    Vec::with_capacity(state.scene.len());
                for elem in &state.scene {
                    if frame_idx >= elem.spawn_frame
                        && elem.kill_frame.map_or(true, |k| frame_idx < k)
                    {
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
            // Clic fuera del composition rect
            state.selected = None;
        }
    }
}
