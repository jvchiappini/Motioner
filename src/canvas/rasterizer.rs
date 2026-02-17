use super::position_cache::{cached_frame_for, scene_fingerprint};
use super::spatial_hash::BoundingBox;
use crate::animations::animations_manager::animated_xy_for;
use crate::app_state::AppState;
use crate::events::time_changed_event::apply_on_time_handlers;
use eframe::egui;
use rayon::prelude::*;

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

    let mut final_color = [255.0, 255.0, 255.0, 255.0];

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

    if let Some(pc) = &state.position_cache {
        if pc.fps == state.fps
            && (pc.duration_secs - state.duration_secs).abs() < 1e-6
            && pc.scene_hash == scene_fingerprint(&state.scene, &state.dsl_event_handlers)
        {
            let frame_idx = (time * pc.fps as f32).round() as isize;
            let frame_idx = frame_idx.clamp(0, pc.frames.len() as isize - 1) as usize;

            if let (Some(frame), Some(grid), Some(bboxes)) = (
                pc.frames.get(frame_idx),
                pc.spatial_grids.get(frame_idx),
                pc.bounding_boxes.get(frame_idx),
            ) {
                let candidate_indices = grid.query(paper_uv.x, paper_uv.y);

                for &shape_idx in candidate_indices {
                    if shape_idx >= all_primitives.len() {
                        continue;
                    }

                    let (shape, actual_spawn) = &all_primitives[shape_idx];
                    if time < *actual_spawn {
                        continue;
                    }

                    if let Some(bbox) = bboxes.get(shape_idx) {
                        if !bbox.contains(paper_uv.x, paper_uv.y) {
                            continue;
                        }
                    }

                    let (eval_x, eval_y) = frame.get(shape_idx).copied().unwrap_or((0.0, 0.0));

                    match shape {
                        crate::scene::Shape::Circle { radius, color, .. } => {
                            let width = state.render_width as f32;
                            let height = state.render_height as f32;
                            let shape_pos = egui::pos2(eval_x * width, eval_y * height);
                            let radius_px = radius * width;
                            let shape_color = [
                                color[0] as f32,
                                color[1] as f32,
                                color[2] as f32,
                                color[3] as f32,
                            ];
                            let dist = pixel_pos.distance(shape_pos);
                            if dist <= radius_px {
                                let src_a = (shape_color[3]) / 255.0;
                                final_color[0] =
                                    final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                                final_color[1] =
                                    final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                                final_color[2] =
                                    final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;

                                if src_a >= 0.999 {
                                    return [
                                        final_color[0].round() as u8,
                                        final_color[1].round() as u8,
                                        final_color[2].round() as u8,
                                        255,
                                    ];
                                }
                            }
                        }
                        crate::scene::Shape::Rect { w, h, color, .. } => {
                            let width = state.render_width as f32;
                            let height = state.render_height as f32;
                            let half_w = (w * width) / 2.0;
                            let half_h = (h * height) / 2.0;
                            let center_x = eval_x * width + half_w;
                            let center_y = eval_y * height + half_h;
                            let shape_pos = egui::pos2(center_x, center_y);
                            let shape_size = egui::vec2(half_w, half_h);
                            let shape_color = [
                                color[0] as f32,
                                color[1] as f32,
                                color[2] as f32,
                                color[3] as f32,
                            ];
                            let d_vec = egui::vec2(
                                (pixel_pos.x - shape_pos.x).abs() - shape_size.x,
                                (pixel_pos.y - shape_pos.y).abs() - shape_size.y,
                            );
                            if d_vec.x <= 0.0 && d_vec.y <= 0.0 {
                                let src_a = (shape_color[3]) / 255.0;
                                final_color[0] =
                                    final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                                final_color[1] =
                                    final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                                final_color[2] =
                                    final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;

                                if src_a >= 0.999 {
                                    return [
                                        final_color[0].round() as u8,
                                        final_color[1].round() as u8,
                                        final_color[2].round() as u8,
                                        255,
                                    ];
                                }
                            }
                        }
                        _ => {}
                    }
                }

                return [
                    final_color[0].round() as u8,
                    final_color[1].round() as u8,
                    final_color[2].round() as u8,
                    255,
                ];
            }
        }
    }

    if let Some(frame) = cached_frame_for(state, time) {
        for (i, (shape, actual_spawn)) in all_primitives.into_iter().enumerate() {
            if time < actual_spawn {
                continue;
            }
            let (eval_x, eval_y) = frame.get(i).copied().unwrap_or((0.0, 0.0));
            match shape {
                crate::scene::Shape::Circle { radius, color, .. } => {
                    let width = state.render_width as f32;
                    let height = state.render_height as f32;
                    let shape_pos = egui::pos2(eval_x * width, eval_y * height);
                    let radius_px = radius * width;
                    let shape_color = [
                        color[0] as f32,
                        color[1] as f32,
                        color[2] as f32,
                        color[3] as f32,
                    ];
                    let dist = pixel_pos.distance(shape_pos);
                    if dist <= radius_px {
                        let src_a = (shape_color[3]) / 255.0;
                        final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                        final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                        final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;
                    }
                }
                crate::scene::Shape::Rect { w, h, color, .. } => {
                    let width = state.render_width as f32;
                    let height = state.render_height as f32;
                    let half_w = (w * width) / 2.0;
                    let half_h = (h * height) / 2.0;
                    let center_x = eval_x * width + half_w;
                    let center_y = eval_y * height + half_h;
                    let shape_pos = egui::pos2(center_x, center_y);
                    let shape_size = egui::vec2(half_w, half_h);
                    let shape_color = [
                        color[0] as f32,
                        color[1] as f32,
                        color[2] as f32,
                        color[3] as f32,
                    ];
                    let d_vec = egui::vec2(
                        (pixel_pos.x - shape_pos.x).abs() - shape_size.x,
                        (pixel_pos.y - shape_pos.y).abs() - shape_size.y,
                    );
                    if d_vec.x <= 0.0 && d_vec.y <= 0.0 {
                        let src_a = (shape_color[3]) / 255.0;
                        final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                        final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                        final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;
                    }
                }
                _ => {}
            }
        }
    } else {
        for (shape, actual_spawn) in all_primitives {
            if time < actual_spawn {
                continue;
            }
            match shape {
                crate::scene::Shape::Circle {
                    x: _x,
                    y: _y,
                    radius,
                    color,
                    ..
                } => {
                    let width = state.render_width as f32;
                    let height = state.render_height as f32;
                    let (eval_x, eval_y) = animated_xy_for(&shape, time, state.duration_secs);
                    let shape_pos = egui::pos2(eval_x * width, eval_y * height);
                    let radius_px = radius * width;
                    let shape_color = [
                        color[0] as f32,
                        color[1] as f32,
                        color[2] as f32,
                        color[3] as f32,
                    ];
                    let dist = pixel_pos.distance(shape_pos);
                    if dist <= radius_px {
                        let src_a = (shape_color[3]) / 255.0;
                        final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                        final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                        final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;
                    }
                }
                crate::scene::Shape::Rect {
                    x: _x,
                    y: _y,
                    w,
                    h,
                    color,
                    ..
                } => {
                    let width = state.render_width as f32;
                    let height = state.render_height as f32;
                    let (eval_x, eval_y) = animated_xy_for(&shape, time, state.duration_secs);
                    let half_w = (w * width) / 2.0;
                    let half_h = (h * height) / 2.0;
                    let center_x = eval_x * width + half_w;
                    let center_y = eval_y * height + half_h;
                    let shape_pos = egui::pos2(center_x, center_y);
                    let shape_size = egui::vec2(half_w, half_h);
                    let shape_color = [
                        color[0] as f32,
                        color[1] as f32,
                        color[2] as f32,
                        color[3] as f32,
                    ];
                    let d_vec = egui::vec2(
                        (pixel_pos.x - shape_pos.x).abs() - shape_size.x,
                        (pixel_pos.y - shape_pos.y).abs() - shape_size.y,
                    );
                    if d_vec.x <= 0.0 && d_vec.y <= 0.0 {
                        let src_a = (shape_color[3]) / 255.0;
                        final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                        final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                        final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;
                    }
                }
                _ => {}
            }
        }
    }

    [
        final_color[0].round() as u8,
        final_color[1].round() as u8,
        final_color[2].round() as u8,
        255,
    ]
}

#[allow(dead_code)]
pub fn render_frame_color_image(state: &AppState, time: f32) -> egui::ColorImage {
    let preview_w = (state.render_width as f32 * state.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let preview_h = (state.render_height as f32 * state.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let mut pixels: Vec<u8> = Vec::with_capacity(preview_w * preview_h * 4);

    for y in 0..preview_h {
        for x in 0..preview_w {
            let uv = egui::pos2(
                (x as f32 + 0.5) / (preview_w as f32),
                (y as f32 + 0.5) / (preview_h as f32),
            );
            let col = sample_color_at(state, uv, time);
            pixels.push(col[0]);
            pixels.push(col[1]);
            pixels.push(col[2]);
            pixels.push(col[3]);
        }
    }

    egui::ColorImage::from_rgba_unmultiplied([preview_w, preview_h], &pixels)
}

pub fn render_frame_color_image_snapshot(
    snap: &crate::canvas::preview_worker::RenderSnapshot,
    time: f32,
) -> egui::ColorImage {
    let mut working_scene = snap.scene.clone();
    let frame_idx = (time * snap.preview_fps as f32).round() as u32;
    apply_on_time_handlers(
        &mut working_scene,
        &snap.dsl_event_handlers,
        time,
        frame_idx,
    );

    let mut preview_w = (snap.render_width as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let mut preview_h = (snap.render_height as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as usize;

    const MAX_CPU_PREVIEW_SIZE: usize = 4096;
    if preview_w > MAX_CPU_PREVIEW_SIZE || preview_h > MAX_CPU_PREVIEW_SIZE {
        let scale_w = if preview_w > MAX_CPU_PREVIEW_SIZE {
            MAX_CPU_PREVIEW_SIZE as f32 / preview_w as f32
        } else {
            1.0
        };
        let scale_h = if preview_h > MAX_CPU_PREVIEW_SIZE {
            MAX_CPU_PREVIEW_SIZE as f32 / preview_h as f32
        } else {
            1.0
        };
        let scale = scale_w.min(scale_h);
        preview_w = (preview_w as f32 * scale).round() as usize;
        preview_h = (preview_h as f32 * scale).round() as usize;
    }

    fn collect_primitives(
        shapes: &[crate::scene::Shape],
        parent_spawn: f32,
        out: &mut Vec<(crate::scene::Shape, f32)>,
    ) {
        for shape in shapes {
            let my_spawn = shape.spawn_time().max(parent_spawn);
            match shape {
                crate::scene::Shape::Group { children, .. } => {
                    collect_primitives(children, my_spawn, out)
                }
                _ => out.push((shape.clone(), my_spawn)),
            }
        }
    }

    let mut all_prims = Vec::new();
    collect_primitives(&working_scene, 0.0, &mut all_prims);

    let mut prim_data: Vec<(f32, f32, BoundingBox, [u8; 4], bool)> =
        Vec::with_capacity(all_prims.len());

    let viewport_bbox = BoundingBox {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 1.0,
        max_y: 1.0,
    };

    for (shape, actual_spawn) in &all_prims {
        if time < *actual_spawn {
            continue;
        }

        let (eval_x, eval_y) = animated_xy_for(shape, time, snap.duration_secs);

        match shape {
            crate::scene::Shape::Circle { radius, color, .. } => {
                let bbox = BoundingBox::from_circle(eval_x, eval_y, *radius);
                if bbox.max_x < viewport_bbox.min_x
                    || bbox.min_x > viewport_bbox.max_x
                    || bbox.max_y < viewport_bbox.min_y
                    || bbox.min_y > viewport_bbox.max_y
                {
                    continue;
                }
                prim_data.push((eval_x, eval_y, bbox, *color, true));
            }
            crate::scene::Shape::Rect { w, h, color, .. } => {
                let bbox = BoundingBox::from_rect(eval_x, eval_y, *w, *h);
                if bbox.max_x < viewport_bbox.min_x
                    || bbox.min_x > viewport_bbox.max_x
                    || bbox.max_y < viewport_bbox.min_y
                    || bbox.min_y > viewport_bbox.max_y
                {
                    continue;
                }
                prim_data.push((eval_x, eval_y, bbox, *color, false));
            }
            _ => {}
        }
    }

    let pixels: Vec<u8> = (0..preview_h)
        .into_par_iter()
        .flat_map(|y| {
            let mut row_pixels = Vec::with_capacity(preview_w * 4);

            for x in 0..preview_w {
                let uv = egui::pos2(
                    (x as f32 + 0.5) / (preview_w as f32),
                    (y as f32 + 0.5) / (preview_h as f32),
                );

                let mut final_color = [255.0, 255.0, 255.0, 255.0];

                let pixel_pos = egui::pos2(
                    uv.x * snap.render_width as f32,
                    uv.y * snap.render_height as f32,
                );

                for (eval_x, eval_y, bbox, color, is_circle) in &prim_data {
                    if !bbox.contains(uv.x, uv.y) {
                        continue;
                    }

                    let shape_color = [
                        color[0] as f32,
                        color[1] as f32,
                        color[2] as f32,
                        color[3] as f32,
                    ];

                    let hit = if *is_circle {
                        let dx = pixel_pos.x - eval_x * snap.render_width as f32;
                        let dy = pixel_pos.y - eval_y * snap.render_height as f32;
                        let radius_px = (bbox.max_x - bbox.min_x) * snap.render_width as f32 / 2.0;
                        let dist_sq = dx * dx + dy * dy;
                        let radius_sq = radius_px * radius_px;
                        dist_sq <= radius_sq
                    } else {
                        let center_x = (bbox.min_x + bbox.max_x) / 2.0 * snap.render_width as f32;
                        let center_y = (bbox.min_y + bbox.max_y) / 2.0 * snap.render_height as f32;
                        let half_w = (bbox.max_x - bbox.min_x) * snap.render_width as f32 / 2.0;
                        let half_h = (bbox.max_y - bbox.min_y) * snap.render_height as f32 / 2.0;
                        let dx = (pixel_pos.x - center_x).abs();
                        let dy = (pixel_pos.y - center_y).abs();
                        dx <= half_w && dy <= half_h
                    };

                    if hit {
                        let src_a = (shape_color[3]) / 255.0;
                        final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                        final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                        final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;

                        if src_a >= 0.999 {
                            break;
                        }
                    }
                }

                row_pixels.push(final_color[0].round() as u8);
                row_pixels.push(final_color[1].round() as u8);
                row_pixels.push(final_color[2].round() as u8);
                row_pixels.push(255);
            }
            row_pixels
        })
        .collect();

    egui::ColorImage::from_rgba_unmultiplied([preview_w, preview_h], &pixels)
}
