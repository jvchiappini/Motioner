use crate::app_state::AppState;
use eframe::egui;

/// Muestrea el color en una coordenada normalizada (0..1) del papel.
/// Respeta la resolución de vista previa y el orden de las formas.
pub fn sample_color_at(state: &AppState, paper_uv: egui::Pos2, time: f32) -> [u8; 4] {
    let preview_res = egui::vec2(
        state.render_width as f32 * state.preview_multiplier,
        state.render_height as f32 * state.preview_multiplier,
    );

    let snapped_uv = egui::pos2(
        (paper_uv.x * preview_res.x).floor() + 0.5,
        (paper_uv.y * preview_res.y).floor() + 0.5,
    );

    let pixel_pos = egui::pos2(
        snapped_uv.x * (state.render_width as f32 / preview_res.x),
        snapped_uv.y * (state.render_height as f32 / preview_res.y),
    );

    let mut final_color = [255.0f32, 255.0, 255.0, 255.0];

    fn collect_primitives(
        shapes: &[crate::scene::Shape],
        parent_spawn: f32,
        out: &mut Vec<(crate::scene::Shape, f32)>,
    ) {
        for shape in shapes {
            let my_spawn = shape.spawn_time().max(parent_spawn);
            match shape {
                crate::scene::Shape::Group { children, .. } => {
                    collect_primitives(children, my_spawn, out);
                }
                _ => out.push((shape.clone(), my_spawn)),
            }
        }
    }

    let mut all_primitives = Vec::new();
    collect_primitives(&state.scene, 0.0, &mut all_primitives);
    // Reverse so that scene index 0 (top of scene graph) wins = sampled last.
    all_primitives.reverse();

    // Búsqueda simple de formas para el color picker (CPU)
    for (shape, actual_spawn) in &all_primitives {
        if time < *actual_spawn {
            continue;
        }
        // honor kill_time if present (don't rasterize after kill)
        if let Some(k) = shape.kill_time() {
            if time >= k {
                continue;
            }
        }
        
        // Obtenemos la posición animada actual
        let (eval_x, eval_y) = crate::animations::animations_manager::animated_xy_for(shape, time, state.duration_secs);
        
        match shape {
            crate::scene::Shape::Circle(c) => {
                let width = state.render_width as f32;
                let height = state.render_height as f32;
                let shape_pos = egui::pos2(eval_x * width, eval_y * height);
                let radius_px = c.radius * width;
                let shape_color = [c.color[0] as f32, c.color[1] as f32, c.color[2] as f32, c.color[3] as f32];
                
                let dist = pixel_pos.distance(shape_pos);
                if dist <= radius_px {
                    let src_a = shape_color[3] / 255.0;
                    final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                    final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                    final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;
                }
            }
            crate::scene::Shape::Rect(r) => {
                let width = state.render_width as f32;
                let height = state.render_height as f32;
                let half_w = (r.w * width) / 2.0;
                let half_h = (r.h * height) / 2.0;
                let center_x = eval_x * width + half_w;
                let center_y = eval_y * height + half_h;
                let shape_pos = egui::pos2(center_x, center_y);
                let shape_size = egui::vec2(half_w, half_h);
                let shape_color = [r.color[0] as f32, r.color[1] as f32, r.color[2] as f32, r.color[3] as f32];
                
                let d_vec = egui::vec2(
                    (pixel_pos.x - shape_pos.x).abs() - shape_size.x,
                    (pixel_pos.y - shape_pos.y).abs() - shape_size.y,
                );
                if d_vec.x <= 0.0 && d_vec.y <= 0.0 {
                    let src_a = shape_color[3] / 255.0;
                    final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                    final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                    final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;
                }
            }
            _ => {} // Otros tipos (Text) por ahora se ignoran en el picker rápido o usan el bounding box simple
        }
    }

    [
        final_color[0].round() as u8,
        final_color[1].round() as u8,
        final_color[2].round() as u8,
        255,
    ]
}
