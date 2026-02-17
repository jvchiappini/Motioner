use super::spatial_hash::{BoundingBox, SpatialHashGrid};
use crate::app_state::AppState;

/// Cache de posiciones aplanadas por frame con optimización espacial.
/// Almacena posiciones precalculadas para evitar re-interpolación constante.
#[derive(Clone)]
pub struct PositionCache {
    pub fps: u32,
    pub duration_secs: f32,
    pub scene_hash: u64,
    pub frames: Vec<Vec<(f32, f32)>>,
    pub flattened_count: usize,
    /// Bounding boxes para cada primitiva (flat_idx => bbox)
    pub bounding_boxes: Vec<Vec<BoundingBox>>,
    /// Grid espacial por frame para culling rápido.
    pub spatial_grids: Vec<SpatialHashGrid>,
}

pub fn scene_fingerprint(
    scene: &[crate::scene::Shape],
    handlers: &[crate::dsl::runtime::DslHandler],
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_shape<H: Hasher>(s: &crate::scene::Shape, h: &mut H) {
        match s {
            crate::scene::Shape::Circle {
                name,
                x,
                y,
                radius,
                color,
                spawn_time,
                animations,
                visible,
            } => {
                name.hash(h);
                (x.to_bits()).hash(h);
                (y.to_bits()).hash(h);
                (radius.to_bits()).hash(h);
                color.hash(h);
                (spawn_time.to_bits()).hash(h);
                visible.hash(h);
                for a in animations {
                    format!("{:?}", a).hash(h);
                }
            }
            crate::scene::Shape::Rect {
                name,
                x,
                y,
                w,
                h: hh,
                color,
                spawn_time,
                animations,
                visible,
            } => {
                name.hash(h);
                (x.to_bits()).hash(h);
                (y.to_bits()).hash(h);
                (w.to_bits()).hash(h);
                (hh.to_bits()).hash(h);
                color.hash(h);
                (spawn_time.to_bits()).hash(h);
                visible.hash(h);
                for a in animations {
                    format!("{:?}", a).hash(h);
                }
            }
            crate::scene::Shape::Group {
                name,
                children,
                visible,
            } => {
                name.hash(h);
                visible.hash(h);
                for c in children {
                    hash_shape(c, h);
                }
            }
        }
    }

    let mut hasher = DefaultHasher::new();
    for s in scene {
        hash_shape(s, &mut hasher);
    }
    for handler in handlers {
        handler.name.hash(&mut hasher);
        handler.body.hash(&mut hasher);
    }
    hasher.finish()
}

pub fn build_position_cache(state: &AppState) -> Option<PositionCache> {
    let fingerprint = scene_fingerprint(&state.scene, &state.dsl_event_handlers);
    if let Some(mut pc) = build_position_cache_for(
        state.scene.clone(),
        state.fps,
        state.duration_secs,
        &state.dsl_event_handlers,
    ) {
        pc.scene_hash = fingerprint;
        Some(pc)
    } else {
        None
    }
}

pub fn build_position_cache_for(
    scene: Vec<crate::scene::Shape>,
    fps: u32,
    duration_secs: f32,
    handlers: &[crate::dsl::runtime::DslHandler],
) -> Option<PositionCache> {
    const MAX_SAMPLES: usize = 50_000;

    let duration = duration_secs.max(0.001);
    let frame_count = (fps as f32 * duration).ceil() as usize;

    let mut flattened: Vec<crate::scene::Shape> = Vec::new();
    for s in &scene {
        flattened.extend(s.flatten(0.0).into_iter().map(|(sh, _)| sh));
    }

    let prim_count = flattened.len();
    if frame_count == 0 || prim_count == 0 {
        return None;
    }

    if frame_count.saturating_mul(prim_count) > MAX_SAMPLES {
        return None;
    }

    let mut frames: Vec<Vec<(f32, f32)>> = Vec::with_capacity(frame_count);
    let mut bboxes: Vec<Vec<BoundingBox>> = Vec::with_capacity(frame_count);
    let mut grids: Vec<SpatialHashGrid> = Vec::with_capacity(frame_count);

    let tile_size = 64.0;

    for fi in 0..frame_count {
        let t = (fi as f32) / (fps as f32);
        let mut row: Vec<(f32, f32)> = Vec::with_capacity(prim_count);
        let mut bbox_row: Vec<BoundingBox> = Vec::with_capacity(prim_count);
        let mut grid = SpatialHashGrid::new(1280, 720, tile_size);

        // Apply DSL handlers to a working copy of the flattened primitives
        // to ensure dynamic positions are captured in the cache.
        let mut frame_prims = flattened.clone();
        crate::events::time_changed_event::apply_on_time_handlers(
            &mut frame_prims,
            handlers,
            t,
            fi as u32,
        );

        for (idx, prim) in frame_prims.iter().enumerate() {
            let (px, py) =
                crate::animations::animations_manager::animated_xy_for(prim, t, duration);
            row.push((px, py));

            let bbox = match prim {
                crate::scene::Shape::Circle { radius, .. } => {
                    BoundingBox::from_circle(px, py, *radius)
                }
                crate::scene::Shape::Rect { w, h, .. } => BoundingBox::from_rect(px, py, *w, *h),
                _ => BoundingBox {
                    min_x: px,
                    min_y: py,
                    max_x: px,
                    max_y: py,
                },
            };
            bbox_row.push(bbox);
            grid.insert(idx, bbox);
        }
        frames.push(row);
        bboxes.push(bbox_row);
        grids.push(grid);
    }

    Some(PositionCache {
        fps,
        duration_secs: duration,
        scene_hash: scene_fingerprint(&scene, handlers),
        frames,
        flattened_count: prim_count,
        bounding_boxes: bboxes,
        spatial_grids: grids,
    })
}

pub fn cached_frame_for(state: &AppState, time: f32) -> Option<&Vec<(f32, f32)>> {
    if let Some(pc) = &state.position_cache {
        if pc.fps == state.fps
            && (pc.duration_secs - state.duration_secs).abs() < 1e-6
            && pc.scene_hash == scene_fingerprint(&state.scene, &state.dsl_event_handlers)
        {
            let frame_idx = (time * pc.fps as f32).round() as isize;
            let clamped = frame_idx.clamp(0, pc.frames.len() as isize - 1) as usize;
            return pc.frames.get(clamped);
        }
    }
    None
}

pub fn position_cache_bytes(state: &AppState) -> usize {
    if let Some(pc) = &state.position_cache {
        pc.frames.len() * pc.flattened_count * 2 * std::mem::size_of::<f32>()
    } else {
        0
    }
}
