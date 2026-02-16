use crate::scene::Easing;
use eframe::egui;

/// Renders an interactive easing curve editor with draggable control points.
/// Returns true if the easing was modified.
pub fn render_easing_curve_editor(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    easing: &mut Easing,
    animation_index: usize,
    element_type: &str, // "circle" or "rect"
) -> bool {
    let mut changed = false;

    // Parameter editors for standard easings
    match easing {
        Easing::EaseIn { power } | Easing::EaseOut { power } | Easing::EaseInOut { power } => {
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Slider::new(power, 0.1..=5.0).text("Power"))
                    .changed()
                {
                    changed = true;
                }
            });
        }
        Easing::Bezier { p1, p2 } => {
            ui.horizontal(|ui| {
                ui.label("P1:");
                if ui
                    .add(
                        egui::DragValue::new(&mut p1.0)
                            .speed(0.01)
                            .clamp_range(0.0..=1.0),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .add(egui::DragValue::new(&mut p1.1).speed(0.01))
                    .changed()
                {
                    changed = true;
                }
                ui.label("P2:");
                if ui
                    .add(
                        egui::DragValue::new(&mut p2.0)
                            .speed(0.01)
                            .clamp_range(0.0..=1.0),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .add(egui::DragValue::new(&mut p2.1).speed(0.01))
                    .changed()
                {
                    changed = true;
                }
            });
        }
        Easing::Custom { .. } => {
            ui.label(
                egui::RichText::new("Left-click add/drag, Right-click remove")
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
        _ => {}
    }

    // UNIFIED GRAPH EDITOR
    let size = egui::vec2(ui.available_width(), 200.0);
    let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
    let rect = response.rect;

    // Background & Grid
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(20));
    painter.rect_stroke(
        rect,
        1.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
    );

    let to_screen = |x: f32, y: f32| {
        egui::pos2(
            rect.left() + x * rect.width(),
            rect.bottom() - y * rect.height(),
        )
    };
    let from_screen = |pos: egui::Pos2| {
        (
            (pos.x - rect.left()) / rect.width(),
            (rect.bottom() - pos.y) / rect.height(),
        )
    };

    // Grid lines
    for i in 1..4 {
        let t = i as f32 / 4.0;
        let x = rect.left() + t * rect.width();
        let y = rect.bottom() - t * rect.height();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
        );
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
        );
    }

    // Draw curve
    let mut curve_points = Vec::new();
    for i in 0..=100 {
        let t = i as f32 / 100.0;
        let v = match easing {
            Easing::Linear => t,
            Easing::EaseIn { power } => t.powf(*power),
            Easing::EaseOut { power } => 1.0 - (1.0 - t).powf(*power),
            Easing::EaseInOut { power } => {
                if t < 0.5 {
                    0.5 * (2.0 * t).powf(*power)
                } else {
                    1.0 - 0.5 * (2.0 * (1.0 - t)).powf(*power)
                }
            }
            Easing::Bezier { p1, p2 } => {
                let u = 1.0 - t;
                3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t
            }
            Easing::Custom { points } => {
                if points.is_empty() {
                    t
                } else if points.len() == 1 {
                    points[0].1
                } else {
                    let mut sorted = points.clone();
                    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    if t <= sorted[0].0 {
                        sorted[0].1
                    } else if t >= sorted[sorted.len() - 1].0 {
                        sorted[sorted.len() - 1].1
                    } else {
                        let mut result = t;
                        for i in 0..sorted.len() - 1 {
                            if t >= sorted[i].0 && t <= sorted[i + 1].0 {
                                let alpha = (t - sorted[i].0) / (sorted[i + 1].0 - sorted[i].0);
                                result = sorted[i].1 + alpha * (sorted[i + 1].1 - sorted[i].1);
                                break;
                            }
                        }
                        result
                    }
                }
            }
        };
        curve_points.push(to_screen(t, v));
    }
    painter.add(egui::Shape::line(
        curve_points,
        egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE),
    ));

    // INTERACTION & EDITORS
    match easing {
        Easing::Bezier { p1, p2 } => {
            // Bezier Handles
            let start = to_screen(0.0, 0.0);
            let end = to_screen(1.0, 1.0);
            let cp1 = to_screen(p1.0, p1.1);
            let cp2 = to_screen(p2.0, p2.1);

            painter.line_segment([start, cp1], egui::Stroke::new(1.0, egui::Color32::GRAY));
            painter.line_segment([end, cp2], egui::Stroke::new(1.0, egui::Color32::GRAY));
            painter.circle_filled(cp1, 4.0, egui::Color32::YELLOW);
            painter.circle_filled(cp2, 4.0, egui::Color32::YELLOW);

            let drag_id =
                ui.make_persistent_id(format!("bezier_drag_{}_{}", element_type, animation_index));
            let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let pointer_down = ui.input(|i| i.pointer.primary_down());
            let was_down_id = ui.make_persistent_id(format!(
                "bezier_was_down_{}_{}",
                element_type, animation_index
            ));
            let was_down = ui
                .data(|d| d.get_temp::<bool>(was_down_id))
                .unwrap_or(false);

            // Detectar inicio de drag
            if pointer_down && !was_down && dragging.is_none() {
                if let Some(pos) = pointer_pos {
                    if rect.contains(pos) {
                        if pos.distance(cp1) < 10.0 {
                            dragging = Some(1);
                        } else if pos.distance(cp2) < 10.0 {
                            dragging = Some(2);
                        }
                        if let Some(idx) = dragging {
                            ui.data_mut(|d| d.insert_temp(drag_id, idx));
                        }
                    }
                }
            }

            // Actualizar durante el drag
            if let Some(idx) = dragging {
                if pointer_down {
                    if let Some(pos) = pointer_pos {
                        let (nx, ny) = from_screen(pos);
                        let new_val = (nx.clamp(0.0, 1.0), ny.clamp(-0.5, 1.5));
                        if idx == 1 {
                            if *p1 != new_val {
                                *p1 = new_val;
                                changed = true;
                            }
                        } else {
                            if *p2 != new_val {
                                *p2 = new_val;
                                changed = true;
                            }
                        }
                        ctx.request_repaint();
                    }
                } else {
                    // Mouse soltado
                    ui.data_mut(|d| d.remove::<usize>(drag_id));
                    changed = true;
                }
            }

            // Guardar estado del mouse
            ui.data_mut(|d| d.insert_temp(was_down_id, pointer_down));
        }
        Easing::Custom { points } => {
            // Points Editor
            let drag_id =
                ui.make_persistent_id(format!("custom_drag_{}_{}", element_type, animation_index));
            let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let pointer_down = ui.input(|i| i.pointer.primary_down());
            let was_down_id = ui.make_persistent_id(format!(
                "custom_was_down_{}_{}",
                element_type, animation_index
            ));
            let was_down = ui
                .data(|d| d.get_temp::<bool>(was_down_id))
                .unwrap_or(false);

            // Draw Points
            for (_idx, p) in points.iter().enumerate() {
                painter.circle_filled(to_screen(p.0, p.1), 5.0, egui::Color32::YELLOW);
            }

            // Right click to remove point
            if response.clicked_by(egui::PointerButton::Secondary) {
                if let Some(pos) = response.interact_pointer_pos() {
                    let mut to_remove = None;
                    for (i, p) in points.iter().enumerate() {
                        if to_screen(p.0, p.1).distance(pos) < 10.0 {
                            to_remove = Some(i);
                            break;
                        }
                    }
                    if let Some(idx) = to_remove {
                        points.remove(idx);
                        changed = true;
                    }
                }
            }

            // Detectar inicio de drag o agregar punto
            if pointer_down && !was_down && dragging.is_none() {
                if let Some(pos) = pointer_pos {
                    if rect.contains(pos) {
                        let mut best_dist = f32::MAX;
                        let mut best = None;
                        for (pt_idx, p) in points.iter().enumerate() {
                            let d = to_screen(p.0, p.1).distance(pos);
                            if d < 10.0 && d < best_dist {
                                best_dist = d;
                                best = Some(pt_idx);
                            }
                        }
                        if let Some(pt_idx) = best {
                            dragging = Some(pt_idx);
                            ui.data_mut(|d| d.insert_temp(drag_id, pt_idx));
                        } else {
                            // Agregar punto si no hay ninguno cerca
                            let (nx, ny) = from_screen(pos);
                            points.push((nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0)));
                            points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                            // Opcional: empezar a arrastrar el nuevo punto?
                            // Buscamos el índice del nuevo punto (tras sort)
                            for (idx, p) in points.iter().enumerate() {
                                if (p.0 - nx).abs() < 0.001 && (p.1 - ny).abs() < 0.001 {
                                    dragging = Some(idx);
                                    ui.data_mut(|d| d.insert_temp(drag_id, idx));
                                    break;
                                }
                            }
                            changed = true;
                        }
                    }
                }
            }

            // Actualizar durante el drag
            if let Some(idx) = dragging {
                if pointer_down {
                    if let Some(pos) = pointer_pos {
                        let (nx, ny) = from_screen(pos);
                        let new_val = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
                        if points[idx] != new_val {
                            points[idx] = new_val;
                            changed = true;
                        }
                        ctx.request_repaint();
                    }
                } else {
                    // Mouse soltado
                    ui.data_mut(|d| d.remove::<usize>(drag_id));
                    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    changed = true;
                }
            }

            // Guardar estado del mouse
            ui.data_mut(|d| d.insert_temp(was_down_id, pointer_down));
        }
        _ => {}
    }

    changed
}
