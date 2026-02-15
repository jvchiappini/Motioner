use crate::animations::animations_manager::animated_xy_for;
use crate::app_state::AppState;
use eframe::egui;
use image::codecs::png::PngEncoder;
use image::ColorType;
use image::ImageEncoder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

/// Cache de tiles renderizados para evitar re-renderizar tiles sin cambios
#[derive(Clone)]
pub struct TileCache {
    pub tile_size: usize,
    pub tiles: HashMap<(usize, usize, u64), Vec<u8>>, // (x, y, scene_hash) -> rgba data
    pub max_tiles: usize,
}

impl TileCache {
    pub fn new(tile_size: usize, max_tiles: usize) -> Self {
        Self {
            tile_size,
            tiles: HashMap::new(),
            max_tiles,
        }
    }

    pub fn get(&self, x: usize, y: usize, hash: u64) -> Option<&[u8]> {
        self.tiles.get(&(x, y, hash)).map(|v| v.as_slice())
    }

    pub fn insert(&mut self, x: usize, y: usize, hash: u64, data: Vec<u8>) {
        if self.tiles.len() >= self.max_tiles {
            // LRU simple: remover primer elemento
            if let Some(key) = self.tiles.keys().next().cloned() {
                self.tiles.remove(&key);
            }
        }
        self.tiles.insert((x, y, hash), data);
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }
}

/// Spatial hash grid for efficient shape culling
#[derive(Clone)]
pub struct SpatialHashGrid {
    pub tile_size: f32,
    pub width: u32,
    pub height: u32,
    /// Maps tile coordinate to list of shape indices
    pub grid: HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialHashGrid {
    pub fn new(width: u32, height: u32, tile_size: f32) -> Self {
        Self {
            tile_size,
            width,
            height,
            grid: HashMap::new(),
        }
    }

    /// Insert a shape into the grid based on its bounding box
    pub fn insert(&mut self, shape_idx: usize, bbox: BoundingBox) {
        let min_x = ((bbox.min_x * self.width as f32) / self.tile_size).floor() as i32;
        let max_x = ((bbox.max_x * self.width as f32) / self.tile_size).ceil() as i32;
        let min_y = ((bbox.min_y * self.height as f32) / self.tile_size).floor() as i32;
        let max_y = ((bbox.max_y * self.height as f32) / self.tile_size).ceil() as i32;

        for tx in min_x..=max_x {
            for ty in min_y..=max_y {
                self.grid
                    .entry((tx, ty))
                    .or_insert_with(Vec::new)
                    .push(shape_idx);
            }
        }
    }

    /// Query shapes that might intersect a pixel position (normalized 0..1)
    pub fn query(&self, x: f32, y: f32) -> &[usize] {
        let tx = ((x * self.width as f32) / self.tile_size).floor() as i32;
        let ty = ((y * self.height as f32) / self.tile_size).floor() as i32;
        self.grid
            .get(&(tx, ty))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn clear(&mut self) {
        self.grid.clear();
    }
}

/// Bounding box in normalized coordinates (0..1)
#[derive(Clone, Copy, Debug)]
pub struct BoundingBox {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl BoundingBox {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    pub fn from_circle(x: f32, y: f32, radius: f32) -> Self {
        Self {
            min_x: (x - radius).max(0.0),
            min_y: (y - radius).max(0.0),
            max_x: (x + radius).min(1.0),
            max_y: (y + radius).min(1.0),
        }
    }

    pub fn from_rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            min_x: x.max(0.0),
            min_y: y.max(0.0),
            max_x: (x + w).min(1.0),
            max_y: (y + h).min(1.0),
        }
    }
}

/// Pool de buffers para reutilizar y evitar allocaciones
pub struct BufferPool {
    buffers: Vec<Vec<u8>>,
}

impl BufferPool {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
        }
    }

    pub fn acquire(&mut self, capacity: usize) -> Vec<u8> {
        if let Some(mut buf) = self.buffers.pop() {
            buf.clear();
            buf.reserve(capacity.saturating_sub(buf.capacity()));
            buf
        } else {
            Vec::with_capacity(capacity)
        }
    }

    pub fn release(&mut self, buf: Vec<u8>) {
        if self.buffers.len() < 8 {
            self.buffers.push(buf);
        }
    }
}

/// Per-frame flattened position cache with spatial optimization
/// frames[frame_idx][flat_idx] => (x, y, bbox) in normalized project coords (0..1)
#[derive(Clone)]
pub struct PositionCache {
    pub fps: u32,
    pub duration_secs: f32,
    pub scene_hash: u64,
    pub frames: Vec<Vec<(f32, f32)>>,
    pub flattened_count: usize,
    /// Bounding boxes for each primitive (flat_idx => bbox)
    pub bounding_boxes: Vec<Vec<BoundingBox>>,
    /// Spatial grid per frame for fast culling
    pub spatial_grids: Vec<SpatialHashGrid>,
}

fn scene_fingerprint(scene: &[crate::scene::Shape]) -> u64 {
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
                    // rely on Debug/Serialize stable fields — lightweight hashing
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
    hasher.finish()
}

/// Build a per-frame flattened position cache for the current `state.scene`.
/// Returns None if cache would be too large.
pub fn build_position_cache(state: &AppState) -> Option<PositionCache> {
    build_position_cache_for(state.scene.clone(), state.fps, state.duration_secs)
}

/// Build position cache from explicit parameters — useful from worker thread.
pub fn build_position_cache_for(
    scene: Vec<crate::scene::Shape>,
    fps: u32,
    duration_secs: f32,
) -> Option<PositionCache> {
    // limit total samples to avoid runaway memory usage (frames * primitives)
    const MAX_SAMPLES: usize = 50_000;

    let duration = duration_secs.max(0.001);
    let frame_count = (fps as f32 * duration).ceil() as usize;

    // flatten scene once to get primitive order
    let mut flattened: Vec<crate::scene::Shape> = Vec::new();
    for s in &scene {
        flattened.extend(s.flatten(0.0).into_iter().map(|(sh, _)| sh));
    }

    let prim_count = flattened.len();
    if frame_count == 0 || prim_count == 0 {
        return None;
    }

    if frame_count.checked_mul(prim_count).unwrap_or(usize::MAX) > MAX_SAMPLES {
        // too large to precompute
        return None;
    }

    let mut frames: Vec<Vec<(f32, f32)>> = Vec::with_capacity(frame_count);
    let mut bboxes: Vec<Vec<BoundingBox>> = Vec::with_capacity(frame_count);
    let mut grids: Vec<SpatialHashGrid> = Vec::with_capacity(frame_count);

    // Precomputar dimensiones para spatial grid (tiles de 64px aprox)
    let tile_size = 64.0;

    for fi in 0..frame_count {
        let t = (fi as f32) / (fps as f32);
        let mut row: Vec<(f32, f32)> = Vec::with_capacity(prim_count);
        let mut bbox_row: Vec<BoundingBox> = Vec::with_capacity(prim_count);
        let mut grid = SpatialHashGrid::new(1280, 720, tile_size);

        for (idx, prim) in flattened.iter().enumerate() {
            let (px, py) =
                crate::animations::animations_manager::animated_xy_for(prim, t, duration);
            row.push((px, py));

            // Calcular bounding box según tipo de shape
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
        scene_hash: scene_fingerprint(&scene),
        frames,
        flattened_count: prim_count,
        bounding_boxes: bboxes,
        spatial_grids: grids,
    })
}

/// Try to get a cached frame (nearest) for `time`.
fn cached_frame_for(state: &AppState, time: f32) -> Option<&Vec<(f32, f32)>> {
    if let Some(pc) = &state.position_cache {
        if pc.fps == state.fps
            && (pc.duration_secs - state.duration_secs).abs() < 1e-6
            && pc.scene_hash == scene_fingerprint(&state.scene)
        {
            let frame_idx = (time * pc.fps as f32).round() as isize;
            let clamped = frame_idx.clamp(0, (pc.frames.len() as isize - 1)) as usize;
            return pc.frames.get(clamped);
        }
    }
    None
}

/// Samples the color at a specific normalized (0..1) paper coordinate,
/// respecting the preview resolution and shape order. `time` is project time in seconds.
/// OPTIMIZADO: usa spatial grid y early exit
fn sample_color_at(state: &crate::app_state::AppState, paper_uv: egui::Pos2, time: f32) -> [u8; 4] {
    let preview_res = egui::vec2(
        state.render_width as f32 * state.preview_multiplier,
        state.render_height as f32 * state.preview_multiplier,
    );

    // Snap UV to the center of the preview pixel (same as shader floor(...) + 0.5)
    let snapped_uv = egui::pos2(
        (paper_uv.x * preview_res.x).floor() + 0.5,
        (paper_uv.y * preview_res.y).floor() + 0.5,
    );

    // Convert snapped logical pixel back to project pixel coordinates
    let pixel_pos = egui::pos2(
        snapped_uv.x * (state.render_width as f32 / preview_res.x),
        snapped_uv.y * (state.render_height as f32 / preview_res.y),
    );

    let mut final_color = [255.0, 255.0, 255.0, 255.0]; // Paper background

    // Recursively traverse the scene graph to find all visual primitives
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

    // OPTIMIZACIÓN: usar spatial grid si está disponible en el cache
    if let Some(pc) = &state.position_cache {
        if pc.fps == state.fps
            && (pc.duration_secs - state.duration_secs).abs() < 1e-6
            && pc.scene_hash == scene_fingerprint(&state.scene)
        {
            let frame_idx = (time * pc.fps as f32).round() as isize;
            let frame_idx = frame_idx.clamp(0, (pc.frames.len() as isize - 1)) as usize;

            if let (Some(frame), Some(grid), Some(bboxes)) = (
                pc.frames.get(frame_idx),
                pc.spatial_grids.get(frame_idx),
                pc.bounding_boxes.get(frame_idx),
            ) {
                // Query spatial grid para obtener solo shapes relevantes
                let candidate_indices = grid.query(paper_uv.x, paper_uv.y);

                // Solo iterar sobre candidatos relevantes (MUCHO más rápido)
                for &shape_idx in candidate_indices {
                    if shape_idx >= all_primitives.len() {
                        continue;
                    }

                    let (shape, actual_spawn) = &all_primitives[shape_idx];
                    if time < *actual_spawn {
                        continue;
                    }

                    // Early exit con bounding box check
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

                                // Early exit si es completamente opaco
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

                                // Early exit si es completamente opaco
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

    // Fallback a path viejo si no hay cache disponible
    // Try to use precomputed positional cache when available — the order
    // produced by `collect_primitives` matches `flatten()` used by the cache.
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
                    let radius_px = radius * width; // Use width as reference
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
                    let radius_px = radius * width; // Use width as reference
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

// animation sampling is implemented in `src/animations/animations_manager.rs`

/// Render a single preview frame into an egui::ColorImage at the current preview resolution.
fn render_frame_color_image(state: &crate::app_state::AppState, time: f32) -> egui::ColorImage {
    let preview_w = (state.render_width as f32 * state.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let preview_h = (state.render_height as f32 * state.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let mut pixels: Vec<u8> = Vec::with_capacity(preview_w * preview_h * 4);

    for y in 0..preview_h {
        for x in 0..preview_w {
            // sample at pixel center in normalized paper coords
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

/// Generate cached preview frames around `center_time` (10 before + 10 after) and
/// store them in `state.preview_frame_cache`. Also update `state.preview_texture` to the center frame.
pub fn generate_preview_frames(state: &mut AppState, center_time: f32, ctx: &egui::Context) {
    // Backward-compat / direct-call fallback: delegate to request_preview_frames (buffered)
    request_preview_frames(state, center_time, PreviewMode::Buffered);
    // poll results immediately so UI updates if worker already finished (non-blocking)
    poll_preview_results(state, ctx);
}

/// Modes for preview generation requests
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewMode {
    Buffered, // 10 frames before/after center
    Single,   // single center frame
}

/// Job sent to the background preview worker
pub enum PreviewJob {
    Generate {
        center_time: f32,
        mode: PreviewMode,
        snapshot: RenderSnapshot,
    },
}

/// Result returned from background worker
pub enum PreviewResult {
    Buffered(Vec<(f32, egui::ColorImage)>),
    Single(f32, egui::ColorImage),
}

// OPTIMIZADO: Reducir cache a 5 frames para ahorrar RAM
// Mantener solo frames cercanos al playhead actual
const MAX_PREVIEW_CACHE_FRAMES: usize = 5;

// Límite seguro de textura GPU (muchas GPUs tienen límite de 2048 o 4096)
// Usamos 2048 para compatibilidad con GPUs más antiguas
const MAX_GPU_TEXTURE_SIZE: u32 = 2048;

/// Detectar VRAM disponible (aproximado) usando adaptador wgpu existente
pub fn detect_vram_size(adapter_info: &wgpu::AdapterInfo) -> usize {
    // Estimar VRAM basado en el tipo de GPU
    // Nota: wgpu no expone directamente la VRAM, así que estimamos
    let estimated_vram = match adapter_info.device_type {
        wgpu::DeviceType::DiscreteGpu => {
            // GPU dedicada moderna (RTX 3050+, RX 6600+): 6-8GB típico
            // RTX 4050 Laptop = 6GB, estimamos conservador 6GB
            6 * 1024 * 1024 * 1024 // 6GB para GPUs dedicadas modernas
        }
        wgpu::DeviceType::IntegratedGpu => {
            // GPU integrada: compartida con RAM, asumir 2GB
            2 * 1024 * 1024 * 1024 // 2GB
        }
        wgpu::DeviceType::VirtualGpu => {
            512 * 1024 * 1024 // 512MB
        }
        _ => {
            1024 * 1024 * 1024 // 1GB por defecto
        }
    };

    eprintln!(
        "[VRAM] Detected GPU: {} ({:?}) - Estimated VRAM: {} MB",
        adapter_info.name,
        adapter_info.device_type,
        estimated_vram / (1024 * 1024)
    );

    estimated_vram
}

fn preview_cache_ram_bytes(state: &AppState) -> usize {
    let mut bytes: usize = 0;
    for (_t, img) in &state.preview_frame_cache {
        let [w, h] = img.size;
        bytes += w * h * 4;
    }
    for (_t, data, _size) in &state.preview_compressed_cache {
        bytes += data.len();
    }
    bytes
}

fn preview_cache_vram_bytes(state: &AppState) -> usize {
    state
        .preview_texture_cache
        .iter()
        .map(|(_, _h, s)| *s)
        .sum()
}

fn color_image_to_rgba_bytes(img: &egui::ColorImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(img.size[0] * img.size[1] * 4);
    for c in &img.pixels {
        let arr = c.to_array();
        out.push(arr[0]);
        out.push(arr[1]);
        out.push(arr[2]);
        out.push(arr[3]);
    }
    out
}

fn compress_color_image_to_png(img: &egui::ColorImage) -> Option<Vec<u8>> {
    let raw = color_image_to_rgba_bytes(img);
    let mut buf: Vec<u8> = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    // encode as 8-bit RGBA
    if encoder
        .write_image(
            &raw,
            img.size[0] as u32,
            img.size[1] as u32,
            ColorType::Rgba8,
        )
        .is_ok()
    {
        Some(buf)
    } else {
        None
    }
}

pub fn position_cache_bytes(state: &AppState) -> usize {
    if let Some(pc) = &state.position_cache {
        // frames * flattened * 2 floats * 4 bytes
        pc.frames.len() * pc.flattened_count * 2 * std::mem::size_of::<f32>()
    } else {
        0
    }
}

fn total_preview_cache_bytes(state: &AppState) -> usize {
    preview_cache_ram_bytes(state) + preview_cache_vram_bytes(state) + position_cache_bytes(state)
}

/// Trim preview caches until total size <= max_bytes. Strategy: prefer
/// trimming RAM cached frames/compressed first, then texture cache.
pub fn enforce_preview_cache_limits(state: &mut AppState, ctx: &egui::Context) {
    let mut total = total_preview_cache_bytes(state);
    let max_bytes = state.preview_cache_max_mb.saturating_mul(1024 * 1024);
    if max_bytes == 0 || total <= max_bytes {
        return;
    }

    // Helper: remove farthest-from-playhead frames first
    let now_time = state.time;

    // Trim uncompressed RAM frames first
    if !state.preview_frame_cache.is_empty() {
        // sort by distance to now_time, keep nearest frames
        state.preview_frame_cache.sort_by(|a, b| {
            let da = (a.0 - now_time).abs();
            let db = (b.0 - now_time).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        while total > max_bytes && state.preview_frame_cache.len() > 1 {
            if let Some((t, img)) = state.preview_frame_cache.pop() {
                let [w, h] = img.size;
                total = total.saturating_sub(w * h * 4);
            } else {
                break;
            }
        }
        // restore chronological order
        state
            .preview_frame_cache
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    }

    // Trim compressed cache next
    if total > max_bytes && !state.preview_compressed_cache.is_empty() {
        state.preview_compressed_cache.sort_by(|a, b| {
            let da = (a.0 - now_time).abs();
            let db = (b.0 - now_time).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        while total > max_bytes && state.preview_compressed_cache.len() > 1 {
            if let Some((t, data, _)) = state.preview_compressed_cache.pop() {
                total = total.saturating_sub(data.len());
            } else {
                break;
            }
        }
    }

    // Trim texture (VRAM) cache last
    if total > max_bytes && !state.preview_texture_cache.is_empty() {
        state.preview_texture_cache.sort_by(|a, b| {
            let da = (a.0 - now_time).abs();
            let db = (b.0 - now_time).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        while total > max_bytes && state.preview_texture_cache.len() > 1 {
            if let Some((_t, _handle, size)) = state.preview_texture_cache.pop() {
                total = total.saturating_sub(size);
            } else {
                break;
            }
        }
    }

    // If we trimmed anything, ensure UI receives an updated center texture
    if !state.preview_frame_cache.is_empty() {
        let center_idx = state.preview_frame_cache.len() / 2;
        if let Some((_t, center_img)) = state.preview_frame_cache.get(center_idx) {
            let handle = ctx.load_texture(
                "preview_center",
                center_img.clone(),
                egui::TextureOptions::NEAREST,
            );
            state.preview_texture = Some(handle);
        }
    } else if !state.preview_texture_cache.is_empty() {
        let center_idx = state.preview_texture_cache.len() / 2;
        if let Some((_t, handle, _s)) = state.preview_texture_cache.get(center_idx) {
            state.preview_texture = Some(handle.clone());
        }
    } else {
        state.preview_texture = None;
        state.preview_cache_center_time = None;
    }

    // notify user if auto-clean is enabled
    if state.preview_cache_auto_clean {
        state.toast_message = Some("Preview cache exceeded limit — auto-cleaned".to_string());
        state.toast_type = crate::app_state::ToastType::Info;
        state.toast_deadline = ctx.input(|i| i.time) + 2.0;
    } else {
        state.toast_message = Some(format!(
            "Preview cache > {} MB — consider clearing or enabling Auto-clean",
            state.preview_cache_max_mb
        ));
        state.toast_type = crate::app_state::ToastType::Info;
        state.toast_deadline = ctx.input(|i| i.time) + 4.0;
    }
}

/// Lightweight snapshot of rendering inputs that can be sent to worker threads.
#[derive(Clone)]
pub struct RenderSnapshot {
    pub scene: Vec<crate::scene::Shape>,
    pub render_width: u32,
    pub render_height: u32,
    pub preview_multiplier: f32,
    pub duration_secs: f32,
    pub preview_fps: u32,
    /// Whether the worker should attempt to use the headless GPU path for
    /// this snapshot. This mirrors `AppState::preview_worker_use_gpu` so the
    /// worker can decide per-job (no need to restart the thread).
    pub use_gpu: bool,
}

fn render_frame_color_image_snapshot(snap: &RenderSnapshot, time: f32) -> egui::ColorImage {
    let mut preview_w = (snap.render_width as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let mut preview_h = (snap.render_height as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as usize;

    // Limitar tamaño máximo para evitar consumo excesivo de RAM
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

    // OPTIMIZACIÓN: collect primitives ONCE, no en cada pixel
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
    collect_primitives(&snap.scene, 0.0, &mut all_prims);

    // Pre-calcular posiciones y bboxes para evitar calcular en cada pixel
    // OPTIMIZACIÓN: Filtrar shapes fuera del viewport ANTES de renderizar
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

        let (eval_x, eval_y) = animated_xy_for(&shape, time, snap.duration_secs);

        match shape {
            crate::scene::Shape::Circle { radius, color, .. } => {
                let bbox = BoundingBox::from_circle(eval_x, eval_y, *radius);
                // Frustum culling: skip si está completamente fuera del viewport
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
                // Frustum culling: skip si está completamente fuera del viewport
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

    // NUEVA OPTIMIZACIÓN: Renderizado paralelo por scanlines
    // Procesar múltiples filas simultáneamente usando rayon
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

                // Iterar solo sobre shapes relevantes (con bbox check)
                for (eval_x, eval_y, bbox, color, is_circle) in &prim_data {
                    // Early exit con bounding box
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
                        // Circle - versión ultra optimizada
                        let dx = pixel_pos.x - eval_x * snap.render_width as f32;
                        let dy = pixel_pos.y - eval_y * snap.render_height as f32;
                        let radius_px = (bbox.max_x - bbox.min_x) * snap.render_width as f32 / 2.0;
                        let dist_sq = dx * dx + dy * dy;
                        let radius_sq = radius_px * radius_px;
                        dist_sq <= radius_sq
                    } else {
                        // Rect
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

                        // Early exit si opaco
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

// GPU-based offscreen renderer used by the preview worker. Returns Err on any wgpu failure so
// the worker can fall back to CPU rasterizer.
#[cfg(feature = "wgpu")]
fn render_frame_color_image_gpu_snapshot(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut GpuResources,
    snap: &RenderSnapshot,
    time: f32,
) -> Result<egui::ColorImage, String> {
    use std::num::NonZeroU32;

    let mut preview_w = (snap.render_width as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as u32;
    let mut preview_h = (snap.render_height as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as u32;

    // CRÍTICO: Validar límites de GPU para evitar crash
    // Si el preview excede MAX_GPU_TEXTURE_SIZE, reducir proporcionalmente
    if preview_w > MAX_GPU_TEXTURE_SIZE || preview_h > MAX_GPU_TEXTURE_SIZE {
        let scale_w = if preview_w > MAX_GPU_TEXTURE_SIZE {
            MAX_GPU_TEXTURE_SIZE as f32 / preview_w as f32
        } else {
            1.0
        };
        let scale_h = if preview_h > MAX_GPU_TEXTURE_SIZE {
            MAX_GPU_TEXTURE_SIZE as f32 / preview_h as f32
        } else {
            1.0
        };
        let scale = scale_w.min(scale_h);
        preview_w = (preview_w as f32 * scale).round() as u32;
        preview_h = (preview_h as f32 * scale).round() as u32;

        eprintln!(
            "[WARN] Preview size exceeded GPU limit ({}x{}). Reduced to {}x{}",
            snap.render_width as f32 * snap.preview_multiplier,
            snap.render_height as f32 * snap.preview_multiplier,
            preview_w,
            preview_h
        );
    }

    // Build GpuShape list (same layout used by on-screen GPU path)
    let mut gpu_shapes: Vec<GpuShape> = Vec::new();

    fn collect_prims(
        shapes: &[crate::scene::Shape],
        parent_spawn: f32,
        out: &mut Vec<(crate::scene::Shape, f32)>,
    ) {
        for s in shapes {
            let my_spawn = s.spawn_time().max(parent_spawn);
            match s {
                crate::scene::Shape::Group { children, .. } => {
                    collect_prims(children, my_spawn, out)
                }
                _ => out.push((s.clone(), my_spawn)),
            }
        }
    }

    let mut all = Vec::new();
    collect_prims(&snap.scene, 0.0, &mut all);

    for (shape, actual_spawn) in all.iter() {
        if time < *actual_spawn {
            continue;
        }
        match shape {
            crate::scene::Shape::Circle {
                x: _,
                y: _,
                radius,
                color,
                ..
            } => {
                let (eval_x, eval_y) = crate::animations::animations_manager::animated_xy_for(
                    shape,
                    time,
                    snap.duration_secs,
                );
                gpu_shapes.push(GpuShape {
                    pos: [eval_x, eval_y],
                    size: [*radius, 0.0],
                    color: [
                        color[0] as f32 / 255.0,
                        color[1] as f32 / 255.0,
                        color[2] as f32 / 255.0,
                        color[3] as f32 / 255.0,
                    ],
                    shape_type: 0,
                    spawn_time: *actual_spawn,
                    p1: 0,
                    p2: 0,
                });
            }
            crate::scene::Shape::Rect {
                x: _,
                y: _,
                w,
                h,
                color,
                ..
            } => {
                let (eval_x, eval_y) = crate::animations::animations_manager::animated_xy_for(
                    shape,
                    time,
                    snap.duration_secs,
                );
                gpu_shapes.push(GpuShape {
                    pos: [eval_x + *w / 2.0, eval_y + *h / 2.0],
                    size: [*w / 2.0, *h / 2.0],
                    color: [
                        color[0] as f32 / 255.0,
                        color[1] as f32 / 255.0,
                        color[2] as f32 / 255.0,
                        color[3] as f32 / 255.0,
                    ],
                    shape_type: 1,
                    spawn_time: *actual_spawn,
                    p1: 0,
                    p2: 0,
                });
            }
            _ => {}
        }
    }

    // Upload shape buffer (resize if necessary)
    let shape_data = bytemuck::cast_slice(&gpu_shapes);
    if shape_data.len() > resources.shape_buffer.size() as usize {
        resources.shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape_buffer_worker"),
            size: (shape_data.len() * 2 + 1024) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        resources.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composition_bind_group_worker"),
            layout: &resources.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: resources.shape_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: resources.uniform_buffer.as_entire_binding(),
                },
            ],
        });
    }

    if !gpu_shapes.is_empty() {
        queue.write_buffer(&resources.shape_buffer, 0, shape_data);
    }

    // Uniforms (match composition.wgsl layout)
    let mut uniforms: [f32; 20] = [0.0; 20];
    uniforms[0] = snap.render_width as f32;
    uniforms[1] = snap.render_height as f32;
    uniforms[2] = snap.render_width as f32 * snap.preview_multiplier;
    uniforms[3] = snap.render_height as f32 * snap.preview_multiplier;
    uniforms[4] = 0.0;
    uniforms[5] = 0.0;
    uniforms[6] = snap.render_width as f32;
    uniforms[7] = snap.render_height as f32;
    uniforms[8] = 0.0;
    uniforms[9] = 0.0;
    uniforms[10] = snap.render_width as f32;
    uniforms[11] = snap.render_height as f32;
    uniforms[12] = gpu_shapes.len() as f32;
    uniforms[13] = 0.0;
    uniforms[14] = 0.0;
    uniforms[15] = 0.0;
    uniforms[16] = time;
    queue.write_buffer(
        &resources.uniform_buffer,
        0,
        bytemuck::cast_slice(&uniforms),
    );

    // Offscreen texture -> render
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview_offscreen"),
        size: wgpu::Extent3d {
            width: preview_w,
            height: preview_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_encoder"),
    });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("preview_renderpass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(&resources.pipeline);
        rpass.set_bind_group(0, &resources.bind_group, &[]);
        rpass.draw(0..6, 0..1);
    }

    // Readback (padded rows)
    let bytes_per_pixel = 4u32;
    let bytes_per_row_unpadded = bytes_per_pixel * preview_w;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = ((bytes_per_row_unpadded + align - 1) / align) * align;
    let output_buffer_size = padded_bytes_per_row as u64 * preview_h as u64;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("preview_readback_buffer"),
        size: output_buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &output_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(preview_h),
            },
        },
        wgpu::Extent3d {
            width: preview_w,
            height: preview_h,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    let buffer_slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    device.poll(wgpu::Maintain::Wait);
    match rx.recv() {
        Ok(Ok(())) => {
            let data = buffer_slice.get_mapped_range();
            let mut pixels: Vec<u8> = Vec::with_capacity((preview_w * preview_h * 4) as usize);
            for row in 0..preview_h as usize {
                let start = row * padded_bytes_per_row as usize;
                let end = start + bytes_per_row_unpadded as usize;
                pixels.extend_from_slice(&data[start..end]);
            }
            drop(data);
            output_buffer.unmap();
            Ok(egui::ColorImage::from_rgba_unmultiplied(
                [preview_w as usize, preview_h as usize],
                &pixels,
            ))
        }
        _ => Err("wgpu readback failed".to_string()),
    }
}

/// Ensure background preview worker is running; if not, spawn it and store channels in `state`.
fn ensure_preview_worker(state: &mut AppState) {
    if state.preview_worker_tx.is_some() && state.preview_worker_rx.is_some() {
        return;
    }

    let (job_tx, job_rx) = mpsc::channel::<PreviewJob>();
    let (res_tx, res_rx) = mpsc::channel::<PreviewResult>();

    // Spawn worker
    thread::spawn(move || {
        // GPU renderer is initialised lazily per-job based on the snapshot.use_gpu
        // flag so changing the setting takes effect immediately.
        #[cfg(feature = "wgpu")]
        let mut gpu_renderer: Option<(wgpu::Device, wgpu::Queue, GpuResources)> = None;

        while let Ok(job) = job_rx.recv() {
            match job {
                PreviewJob::Generate {
                    center_time,
                    mode,
                    snapshot,
                } => {
                    match mode {
                        PreviewMode::Single => {
                            // Decide per-job whether to use GPU (snapshot.use_gpu mirrors the
                            // user's setting). Initialise GPU lazily and drop it if the
                            // job requests CPU only.
                            #[cfg(feature = "wgpu")]
                            {
                                if snapshot.use_gpu {
                                    if gpu_renderer.is_none() {
                                        // init device on-demand
                                        let instance =
                                            wgpu::Instance::new(wgpu::InstanceDescriptor {
                                                backends: wgpu::Backends::PRIMARY,
                                                dx12_shader_compiler: Default::default(),
                                                flags: wgpu::InstanceFlags::empty(),
                                                gles_minor_version:
                                                    wgpu::Gles3MinorVersion::default(),
                                            });
                                        if let Some(adapter) =
                                            pollster::block_on(instance.request_adapter(
                                                &wgpu::RequestAdapterOptions {
                                                    power_preference:
                                                        wgpu::PowerPreference::HighPerformance,
                                                    compatible_surface: None,
                                                    force_fallback_adapter: false,
                                                },
                                            ))
                                        {
                                            if let Ok((device, queue)) =
                                                pollster::block_on(adapter.request_device(
                                                    &wgpu::DeviceDescriptor {
                                                        label: Some("preview-worker-device"),
                                                        required_features: wgpu::Features::empty(),
                                                        required_limits:
                                                            wgpu::Limits::downlevel_defaults(),
                                                    },
                                                    None,
                                                ))
                                            {
                                                let target_format =
                                                    wgpu::TextureFormat::Rgba8UnormSrgb;
                                                let resources =
                                                    GpuResources::new(&device, target_format);
                                                gpu_renderer = Some((device, queue, resources));
                                            }
                                        }
                                    }

                                    if let Some((ref device, ref queue, ref mut resources)) =
                                        gpu_renderer
                                    {
                                        let img = render_frame_color_image_gpu_snapshot(
                                            device,
                                            queue,
                                            resources,
                                            &snapshot,
                                            center_time,
                                        )
                                        .unwrap_or_else(|_| {
                                            render_frame_color_image_snapshot(
                                                &snapshot,
                                                center_time,
                                            )
                                        });
                                        let _ =
                                            res_tx.send(PreviewResult::Single(center_time, img));
                                        continue;
                                    }
                                } else {
                                    // If user disabled GPU for previews, drop any cached GPU renderer
                                    gpu_renderer = None;
                                }
                            }

                            // CPU fallback or when wgpu feature not compiled
                            let img = render_frame_color_image_snapshot(&snapshot, center_time);
                            let _ = res_tx.send(PreviewResult::Single(center_time, img));
                        }
                        PreviewMode::Buffered => {
                            // OPTIMIZADO: Reducir aún más los frames pre-cacheados
                            // Solo mantener 2-3 frames antes/después del actual
                            let frames_each_side = if snapshot.preview_multiplier > 1.0 {
                                2i32 // Hi-res: solo +/-2 frames
                            } else {
                                3i32 // Low-res: +/-3 frames
                            };
                            let frame_step = 1.0 / (snapshot.preview_fps as f32);
                            let mut frames =
                                Vec::with_capacity((frames_each_side * 2 + 1) as usize);
                            for i in -frames_each_side..=frames_each_side {
                                let t = (center_time + (i as f32) * frame_step)
                                    .clamp(0.0, snapshot.duration_secs);

                                #[cfg(feature = "wgpu")]
                                let img = if snapshot.use_gpu {
                                    // init device lazily if needed
                                    if gpu_renderer.is_none() {
                                        let instance =
                                            wgpu::Instance::new(wgpu::InstanceDescriptor {
                                                backends: wgpu::Backends::PRIMARY,
                                                dx12_shader_compiler: Default::default(),
                                                flags: wgpu::InstanceFlags::empty(),
                                                gles_minor_version:
                                                    wgpu::Gles3MinorVersion::default(),
                                            });
                                        if let Some(adapter) =
                                            pollster::block_on(instance.request_adapter(
                                                &wgpu::RequestAdapterOptions {
                                                    power_preference:
                                                        wgpu::PowerPreference::HighPerformance,
                                                    compatible_surface: None,
                                                    force_fallback_adapter: false,
                                                },
                                            ))
                                        {
                                            if let Ok((device, queue)) =
                                                pollster::block_on(adapter.request_device(
                                                    &wgpu::DeviceDescriptor {
                                                        label: Some("preview-worker-device"),
                                                        required_features: wgpu::Features::empty(),
                                                        required_limits:
                                                            wgpu::Limits::downlevel_defaults(),
                                                    },
                                                    None,
                                                ))
                                            {
                                                let target_format =
                                                    wgpu::TextureFormat::Rgba8UnormSrgb;
                                                let resources =
                                                    GpuResources::new(&device, target_format);
                                                gpu_renderer = Some((device, queue, resources));
                                            }
                                        }
                                    }

                                    if let Some((ref device, ref queue, ref mut resources)) =
                                        gpu_renderer
                                    {
                                        render_frame_color_image_gpu_snapshot(
                                            device, queue, resources, &snapshot, t,
                                        )
                                        .unwrap_or_else(
                                            |_| render_frame_color_image_snapshot(&snapshot, t),
                                        )
                                    } else {
                                        render_frame_color_image_snapshot(&snapshot, t)
                                    }
                                } else {
                                    // forced CPU path
                                    render_frame_color_image_snapshot(&snapshot, t)
                                };

                                frames.push((t, img));
                                // send intermediate single-frame updates for smoother UX
                                let _ = res_tx.send(PreviewResult::Single(
                                    t,
                                    frames.last().unwrap().1.clone(),
                                ));
                            }
                            let _ = res_tx.send(PreviewResult::Buffered(frames));
                        }
                    }
                }
            }
        }
    });

    state.preview_worker_tx = Some(job_tx);
    state.preview_worker_rx = Some(res_rx);
}

/// Request preview frames (delegates to background worker). Non-blocking.
pub fn request_preview_frames(state: &mut AppState, center_time: f32, mode: PreviewMode) {
    ensure_preview_worker(state);
    // Throttle: don't enqueue a new single-frame job if one is already pending.
    // This prevents the background worker from being flooded when the user
    // scrubs or drags the playhead rapidly. Buffered requests (cache fill)
    // are still allowed to queue.
    if mode == PreviewMode::Single && state.preview_job_pending {
        return;
    }

    if let Some(tx) = &state.preview_worker_tx {
        let snap = RenderSnapshot {
            scene: state.scene.clone(),
            render_width: state.render_width,
            render_height: state.render_height,
            preview_multiplier: state.preview_multiplier,
            duration_secs: state.duration_secs,
            preview_fps: state.preview_fps,
            use_gpu: state.preview_worker_use_gpu,
        };
        let job = PreviewJob::Generate {
            center_time,
            mode,
            snapshot: snap,
        };
        // mark pending for single-frame interactive jobs so subsequent scrubs
        // don't enqueue more work until we see a result
        if mode == PreviewMode::Single {
            state.preview_job_pending = true;
        }
        let _ = tx.send(job);
    }
}

/// Poll for preview results from the worker and integrate them into `state` (must be called on UI thread).
/// OPTIMIZADO v2: Detecta VRAM y prioriza cache GPU agresivamente
pub fn poll_preview_results(state: &mut AppState, ctx: &egui::Context) {
    if let Some(rx) = &state.preview_worker_rx {
        let mut needs_enforce = false;

        // Calcular límite de VRAM disponible para cache
        let vram_limit_bytes = if state.prefer_vram_cache && state.estimated_vram_bytes > 0 {
            (state.estimated_vram_bytes as f32 * state.vram_cache_max_percent) as usize
        } else {
            usize::MAX // Si no hay límite, usar todo lo que permita egui
        };

        let current_vram_usage = preview_cache_vram_bytes(state);
        let vram_available = vram_limit_bytes.saturating_sub(current_vram_usage);

        while let Ok(result) = rx.try_recv() {
            // a worker result means at least one pending job completed
            state.preview_job_pending = false;
            match result {
                PreviewResult::Single(t, img) => {
                    // ESTRATEGIA AGRESIVA: Usar VRAM siempre que sea posible
                    let img_size = img.size[0] * img.size[1] * 4;
                    let use_vram = state.prefer_vram_cache
                        && (vram_available >= img_size || state.preview_worker_use_gpu);

                    // Actualizar textura en pantalla
                    let handle = ctx.load_texture(
                        "preview_center",
                        img.clone(),
                        egui::TextureOptions::NEAREST,
                    );
                    state.preview_texture = Some(handle);
                    state.preview_cache_center_time = Some(t);

                    if use_vram {
                        // Guardar en VRAM cache (no en RAM = ahorro masivo)
                        let tex_name = format!("preview_cached_{:.6}", t);
                        let th =
                            ctx.load_texture(&tex_name, img.clone(), egui::TextureOptions::NEAREST);
                        state
                            .preview_texture_cache
                            .retain(|(tt, _h, _s)| (tt - t).abs() > 1e-6);
                        state.preview_texture_cache.push((t, th, img_size));

                        // NO guardar en RAM frame cache (ahorro de RAM!)
                    } else if state.compress_preview_cache {
                        // compress to PNG and store bytes to reduce RAM
                        if let Some(bytes) = compress_color_image_to_png(&img) {
                            state
                                .preview_compressed_cache
                                .retain(|(tt, _b, _s)| (tt - t).abs() > 1e-6);
                            state.preview_compressed_cache.push((
                                t,
                                bytes,
                                (img.size[0], img.size[1]),
                            ));
                        } else {
                            state
                                .preview_frame_cache
                                .retain(|(tt, _)| (tt - t).abs() > 1e-6);
                            state.preview_frame_cache.push((t, img.clone()));
                        }
                    } else {
                        // keep in RAM as ColorImage (fallback)
                        state
                            .preview_frame_cache
                            .retain(|(tt, _)| (tt - t).abs() > 1e-6);
                        state.preview_frame_cache.push((t, img.clone()));
                    }

                    // mark that we must enforce limits after finishing this borrow
                    needs_enforce = true;
                }
                PreviewResult::Buffered(frames) => {
                    // ESTRATEGIA AGRESIVA: Llenar VRAM primero, RAM solo si se llena
                    // Calcular espacio VRAM disponible en este punto
                    let mut vram_space = vram_available;
                    let use_vram_strategy = state.prefer_vram_cache;

                    // downsample to MAX_PREVIEW_CACHE_FRAMES window first
                    let selected = if frames.len() > MAX_PREVIEW_CACHE_FRAMES {
                        let center = frames.len() / 2;
                        let half = MAX_PREVIEW_CACHE_FRAMES / 2;
                        let start = center.saturating_sub(half);
                        let end = (start + MAX_PREVIEW_CACHE_FRAMES).min(frames.len());
                        frames[start..end].to_vec()
                    } else {
                        frames.clone()
                    };

                    // Limpiar caches previos
                    if use_vram_strategy || state.preview_worker_use_gpu {
                        state.preview_texture_cache.clear();
                        state.preview_frame_cache.clear();
                        state.preview_compressed_cache.clear();

                        // Llenar VRAM hasta límite
                        for (t, img) in &selected {
                            let img_size = img.size[0] * img.size[1] * 4;

                            if vram_space >= img_size || state.preview_worker_use_gpu {
                                // Guardar en VRAM
                                let tex_name = format!("preview_cached_{:.6}", t);
                                let handle = ctx.load_texture(
                                    &tex_name,
                                    img.clone(),
                                    egui::TextureOptions::NEAREST,
                                );
                                state.preview_texture_cache.push((*t, handle, img_size));
                                vram_space = vram_space.saturating_sub(img_size);
                            } else if state.compress_preview_cache {
                                // Overflow a RAM comprimido
                                if let Some(bytes) = compress_color_image_to_png(img) {
                                    state.preview_compressed_cache.push((
                                        *t,
                                        bytes,
                                        (img.size[0], img.size[1]),
                                    ));
                                } else {
                                    state.preview_frame_cache.push((*t, img.clone()));
                                }
                            } else {
                                // Overflow a RAM sin comprimir
                                state.preview_frame_cache.push((*t, img.clone()));
                            }
                        }
                    } else if state.compress_preview_cache {
                        state.preview_compressed_cache.clear();
                        for (t, img) in &selected {
                            if let Some(bytes) = compress_color_image_to_png(img) {
                                state.preview_compressed_cache.push((
                                    *t,
                                    bytes,
                                    (img.size[0], img.size[1]),
                                ));
                            } else {
                                state.preview_frame_cache.push((*t, img.clone()));
                            }
                        }
                    } else {
                        state.preview_frame_cache = selected.clone();
                        state.preview_texture_cache.clear();
                        state.preview_compressed_cache.clear();
                    }

                    // Pick center frame dynamically and update the on-screen texture
                    if !selected.is_empty() {
                        let center_idx = selected.len() / 2;
                        if let Some((t, center_img)) = selected.get(center_idx) {
                            let handle = ctx.load_texture(
                                "preview_center",
                                center_img.clone(),
                                egui::TextureOptions::NEAREST,
                            );
                            state.preview_texture = Some(handle);
                            state.preview_cache_center_time = Some(*t);
                        }
                    }

                    // mark that we must enforce limits after finishing this borrow
                    needs_enforce = true;
                }
            }
        }
        if needs_enforce {
            enforce_preview_cache_limits(state, ctx);
        }
    }
}

/// Render and handle interactions for the central canvas area.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, main_ui_enabled: bool) {
    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        // Use Sense::drag() to handle panning and clicks
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::drag().union(egui::Sense::click()),
        );

        // --- Interaction ---
        if main_ui_enabled {
            // Panning: Right-click drag or Middle-click drag
            if response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle)
            {
                state.canvas_pan_x += response.drag_delta().x;
                state.canvas_pan_y += response.drag_delta().y;
            }

            // Zooming: Scroll wheel
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let zoom_delta = (scroll * 0.002).exp();

                    // Zoom towards mouse position
                    if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                        let zoom_before = state.canvas_zoom;
                        state.canvas_zoom *= zoom_delta;
                        state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
                        let actual_delta = state.canvas_zoom / zoom_before;

                        // Adjust pan to keep mouse-over point stationary
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

        let painter = ui.painter_at(rect);

        // Canvas bg: Gray
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(60, 60, 60));

        // --- Grid Rendering ---
        let zoom = state.canvas_zoom;
        let pan = egui::vec2(state.canvas_pan_x, state.canvas_pan_y);
        let center = rect.center();

        // Dynamic grid step (AutoCAD-like behavior: grid sub-divides)
        let mut base_step = 100.0;
        while base_step * zoom > 200.0 {
            base_step /= 10.0;
        }
        while base_step * zoom < 20.0 {
            base_step *= 10.0;
        }

        let step = base_step * zoom;

        // Calculate the starting position for the grid lines
        // We want origin to be at (center.x + pan.x, center.y + pan.y)
        let grid_origin = center + pan;

        let start_x = rect.left() + (grid_origin.x - rect.left()) % step - step;
        let start_y = rect.top() + (grid_origin.y - rect.top()) % step - step;

        let grid_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 40));
        let _major_grid_stroke = egui::Stroke::new(1.2, egui::Color32::BLACK);
        let origin_stroke_x = egui::Stroke::new(2.0, egui::Color32::from_rgb(150, 50, 50)); // Red-ish for X
        let origin_stroke_y = egui::Stroke::new(2.0, egui::Color32::from_rgb(50, 150, 50)); // Green-ish for Y

        // Vertical lines
        let mut x = start_x;
        while x <= rect.right() + step {
            if x >= rect.left() {
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    grid_stroke,
                );
            }
            x += step;
        }

        // Horizontal lines
        let mut y = start_y;
        while y <= rect.bottom() + step {
            if y >= rect.top() {
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    grid_stroke,
                );
            }
            y += step;
        }

        // Draw origin axes
        if grid_origin.x >= rect.left() && grid_origin.x <= rect.right() {
            painter.line_segment(
                [
                    egui::pos2(grid_origin.x, rect.top()),
                    egui::pos2(grid_origin.x, rect.bottom()),
                ],
                origin_stroke_y,
            );
        }
        if grid_origin.y >= rect.top() && grid_origin.y <= rect.bottom() {
            painter.line_segment(
                [
                    egui::pos2(rect.left(), grid_origin.y),
                    egui::pos2(rect.right(), grid_origin.y),
                ],
                origin_stroke_x,
            );
        }

        // --- Fictitious Composition Canvas (The "Paper" or "Main Viewport") ---
        // This is where the actual project elements will be drawn.
        // The size on screen only depends on the project resolution and zoom.
        let composition_size =
            egui::vec2(state.render_width as f32, state.render_height as f32) * zoom;
        let composition_min = grid_origin - composition_size / 2.0;
        let composition_rect = egui::Rect::from_min_size(composition_min, composition_size);

        // remember composition rect for other UI (Project Settings centering)
        state.last_composition_rect = Some(composition_rect);

        // Draw shadows/border for the composition area
        let shadow_rect = composition_rect.expand(4.0 * zoom);
        painter.rect_filled(shadow_rect, 0.0, egui::Color32::from_black_alpha(100));

        // Draw the white paper (background)
        painter.rect_filled(composition_rect, 0.0, egui::Color32::WHITE);
        painter.rect_stroke(
            composition_rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        );
        // Draw a shadow or border for the "Paper" to make it pop against the gray
        painter.rect_stroke(
            composition_rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        );

        // --- Software Rasterizer Pass ---
        // This buffer has the actual "preview" resolution.

        // Lazily build a per-frame position cache when possible. This
        // reduces repeated interpolation cost while scrubbing/playing when
        // the scene is stable. `build_position_cache` guards against
        // excessive memory usage.
        if state.position_cache.is_none() {
            if let Some(pc) = build_position_cache(state) {
                state.position_cache = Some(pc);
            }
        }
        // --- GPU / WGPU Rasterizer ---
        #[cfg(feature = "wgpu")]
        {
            let mut gpu_shapes = Vec::new();

            // try to obtain a cached frame for the current time (if available)
            let cached = cached_frame_for(state, state.time);
            let mut flat_idx: usize = 0;

            fn fill_gpu_shapes(
                shapes: &[crate::scene::Shape],
                gpu_shapes: &mut Vec<GpuShape>,
                _render_width: f32,
                _render_height: f32,
                parent_spawn: f32,
                current_time: f32,
                project_duration: f32,
                cached: Option<&Vec<(f32, f32)>>,
                flat_idx: &mut usize,
            ) {
                for shape in shapes {
                    let my_spawn = shape.spawn_time().max(parent_spawn);
                    match shape {
                        crate::scene::Shape::Circle {
                            x,
                            y,
                            radius,
                            color,
                            ..
                        } => {
                            // Use cached position when available, otherwise evaluate
                            let (eval_x, eval_y) = if let Some(frame) = cached {
                                let p = frame.get(*flat_idx).copied().unwrap_or((0.0, 0.0));
                                *flat_idx += 1;
                                p
                            } else {
                                *flat_idx += 1;
                                crate::animations::animations_manager::animated_xy_for(
                                    shape,
                                    current_time,
                                    project_duration,
                                )
                            };
                            gpu_shapes.push(GpuShape {
                                pos: [eval_x, eval_y],
                                size: [*radius, 0.0],
                                color: [
                                    color[0] as f32 / 255.0,
                                    color[1] as f32 / 255.0,
                                    color[2] as f32 / 255.0,
                                    color[3] as f32 / 255.0,
                                ],
                                shape_type: 0,
                                spawn_time: my_spawn,
                                p1: 0,
                                p2: 0,
                            });
                        }
                        crate::scene::Shape::Rect {
                            x, y, w, h, color, ..
                        } => {
                            let (eval_x, eval_y) = if let Some(frame) = cached {
                                let p = frame.get(*flat_idx).copied().unwrap_or((0.0, 0.0));
                                *flat_idx += 1;
                                p
                            } else {
                                *flat_idx += 1;
                                crate::animations::animations_manager::animated_xy_for(
                                    shape,
                                    current_time,
                                    project_duration,
                                )
                            };
                            gpu_shapes.push(GpuShape {
                                pos: [eval_x + *w / 2.0, eval_y + *h / 2.0],
                                size: [*w / 2.0, *h / 2.0],
                                color: [
                                    color[0] as f32 / 255.0,
                                    color[1] as f32 / 255.0,
                                    color[2] as f32 / 255.0,
                                    color[3] as f32 / 255.0,
                                ],
                                shape_type: 1,
                                spawn_time: my_spawn,
                                p1: 0,
                                p2: 0,
                            });
                        }
                        crate::scene::Shape::Group { children, .. } => {
                            fill_gpu_shapes(
                                children,
                                gpu_shapes,
                                _render_width,
                                _render_height,
                                my_spawn,
                                current_time,
                                project_duration,
                                cached,
                                flat_idx,
                            );
                        }
                    }
                }
            }

            fill_gpu_shapes(
                &state.scene,
                &mut gpu_shapes,
                state.render_width as f32,
                state.render_height as f32,
                0.0,
                state.time,
                state.duration_secs,
                cached,
                &mut flat_idx,
            );

            // Important: use the FULL canvas rect for the callback to avoid coordinate distortion
            let magnifier_pos = if state.picker_active {
                ui.input(|i| i.pointer.hover_pos())
            } else {
                None
            };

            let cb = egui_wgpu::Callback::new_paint_callback(
                rect, // Use full viewport rect, not just the paper rect
                CompositionCallback {
                    shapes: gpu_shapes,
                    render_width: state.render_width as f32,
                    render_height: state.render_height as f32,
                    preview_multiplier: state.preview_multiplier,
                    paper_rect: composition_rect,
                    viewport_rect: rect,
                    magnifier_pos,
                    time: state.time,
                },
            );

            painter.add(cb);
        }

        #[cfg(not(feature = "wgpu"))]
        {
            // Non-wgpu rasterizer: use cached positions when available. We
            // traverse the scene in the same order as `flatten()` so the
            // precomputed cache (if present) can be indexed by a running
            // `flat_idx` counter.
            let cached = cached_frame_for(state, state.time);
            let mut flat_idx: usize = 0;

            fn draw_shapes_recursive(
                ui_painter: &egui::Painter,
                shapes: &[crate::scene::Shape],
                composition_rect: egui::Rect,
                zoom: f32,
                current_time: f32,
                parent_spawn: f32,
                project_duration: f32,
                cached: Option<&Vec<(f32, f32)>>,
                flat_idx: &mut usize,
            ) {
                for shape in shapes {
                    let actual_spawn = shape.spawn_time().max(parent_spawn);
                    if current_time < actual_spawn {
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
                            let (eval_x, eval_y) = if let Some(frame) = cached {
                                let p = frame.get(*flat_idx).copied().unwrap_or((0.0, 0.0));
                                *flat_idx += 1;
                                p
                            } else {
                                *flat_idx += 1;
                                animated_xy_for(shape, current_time, project_duration)
                            };
                            let pos = composition_rect.min
                                + egui::vec2(
                                    eval_x * composition_rect.width(),
                                    eval_y * composition_rect.height(),
                                );
                            let scaled_radius = radius * composition_rect.width();
                            let c = egui::Color32::from_rgba_unmultiplied(
                                color[0], color[1], color[2], color[3],
                            );
                            ui_painter.circle_filled(pos, scaled_radius, c);
                        }
                        crate::scene::Shape::Rect {
                            x: _x,
                            y: _y,
                            w,
                            h,
                            color,
                            ..
                        } => {
                            let (eval_x, eval_y) = if let Some(frame) = cached {
                                let p = frame.get(*flat_idx).copied().unwrap_or((0.0, 0.0));
                                *flat_idx += 1;
                                p
                            } else {
                                *flat_idx += 1;
                                animated_xy_for(shape, current_time, project_duration)
                            };
                            let min = composition_rect.min
                                + egui::vec2(
                                    eval_x * composition_rect.width(),
                                    eval_y * composition_rect.height(),
                                );
                            let size = egui::vec2(
                                w * composition_rect.width(),
                                h * composition_rect.height(),
                            );
                            let rect = egui::Rect::from_min_size(min, size);
                            let c = egui::Color32::from_rgba_unmultiplied(
                                color[0], color[1], color[2], color[3],
                            );
                            ui_painter.rect_filled(rect, 0.0, c);
                        }
                        crate::scene::Shape::Group { children, .. } => {
                            draw_shapes_recursive(
                                ui_painter,
                                children,
                                composition_rect,
                                zoom,
                                current_time,
                                actual_spawn,
                                project_duration,
                                cached,
                                flat_idx,
                            );
                        }
                    }
                }
            }

            draw_shapes_recursive(
                &painter,
                &state.scene,
                composition_rect,
                zoom,
                state.time,
                0.0,
                state.duration_secs,
                cached,
                &mut flat_idx,
            );
        }

        // Interaction: clicks / selection relative to normalized coordinates
        if main_ui_enabled && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if composition_rect.contains(pos) {
                    let paper_uv = egui::pos2(
                        (pos.x - composition_rect.min.x) / composition_rect.width(),
                        (pos.y - composition_rect.min.y) / composition_rect.height(),
                    );

                    if state.picker_active {
                        // COLOR PICKER LOGIC
                        let color = sample_color_at(state, paper_uv, state.time);
                        let hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

                        // Copy to clipboard
                        ui.output_mut(|o| o.copied_text = hex.clone());

                        // Show Toast
                        state.picker_color = color;
                        state.toast_message = Some(format!("Color {} copied to clipboard!", hex));
                        state.toast_type = crate::app_state::ToastType::Success;
                        state.toast_deadline = ui.input(|i| i.time) + 3.0;

                        // Auto-disable picker after pick
                        state.picker_active = false;
                    } else {
                        // SELECTION LOGIC
                        // Determine hit path recursively
                        fn find_hit_path(
                            shapes: &[crate::scene::Shape],
                            pos: egui::Pos2,
                            composition_rect: egui::Rect,
                            zoom: f32,
                            current_path: Vec<usize>,
                            current_time: f32,
                            parent_spawn: f32,
                        ) -> Option<Vec<usize>> {
                            // Iterate in reverse order to prefer picking top-most elements
                            for (i, shape) in shapes.iter().enumerate().rev() {
                                let actual_spawn = shape.spawn_time().max(parent_spawn);
                                if current_time < actual_spawn {
                                    continue;
                                }

                                let mut path = current_path.clone();
                                path.push(i);

                                match shape {
                                    crate::scene::Shape::Circle { x, y, radius, .. } => {
                                        let cw = *x * composition_rect.width();
                                        let ch = *y * composition_rect.height();
                                        let center =
                                            composition_rect.left_top() + egui::vec2(cw, ch);
                                        let scaled_radius = radius * composition_rect.width();
                                        if pos.distance(center) <= scaled_radius {
                                            return Some(path);
                                        }
                                    }
                                    crate::scene::Shape::Rect { x, y, w, h, .. } => {
                                        let cw = *x * composition_rect.width();
                                        let ch = *y * composition_rect.height();
                                        let min = composition_rect.left_top() + egui::vec2(cw, ch);
                                        let size = egui::vec2(
                                            w * composition_rect.width(),
                                            h * composition_rect.height(),
                                        );
                                        let rect = egui::Rect::from_min_size(min, size);
                                        if rect.contains(pos) {
                                            return Some(path);
                                        }
                                    }
                                    crate::scene::Shape::Group { children, .. } => {
                                        // Try to hit children first
                                        if let Some(child_path) = find_hit_path(
                                            children,
                                            pos,
                                            composition_rect,
                                            zoom,
                                            path.clone(),
                                            current_time,
                                            actual_spawn,
                                        ) {
                                            return Some(child_path);
                                        }

                                        // If no child hit, but we want the group to be pickable as a whole,
                                        // we'd need a bounding box for the group. For now, groups aren't picked directly
                                        // unless we are picking their children.
                                    }
                                }
                            }
                            None
                        }

                        let hit_path = find_hit_path(
                            &state.scene,
                            pos,
                            composition_rect,
                            zoom,
                            Vec::new(),
                            state.time,
                            0.0,
                        );

                        // Update both the top-level selection and the selected node path
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

        // Draw selection highlight (supports nested selection paths)
        if let Some(path) = &state.selected_node_path {
            let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 165, 0));

            // Helper to draw highlights recursively for a shape
            fn draw_highlight_recursive(
                painter: &egui::Painter,
                shape: &crate::scene::Shape,
                composition_rect: egui::Rect,
                _zoom: f32,
                stroke: egui::Stroke,
                current_time: f32,
                parent_spawn: f32,
            ) {
                let actual_spawn = shape.spawn_time().max(parent_spawn);
                if current_time < actual_spawn {
                    return;
                }
                match shape {
                    crate::scene::Shape::Circle { x, y, radius, .. } => {
                        let cw = *x * composition_rect.width();
                        let ch = *y * composition_rect.height();
                        let center = composition_rect.left_top() + egui::vec2(cw, ch);
                        let scaled_radius = radius * composition_rect.width();
                        painter.circle_stroke(center, scaled_radius, stroke);
                    }
                    crate::scene::Shape::Rect { x, y, w, h, .. } => {
                        let cw = *x * composition_rect.width();
                        let ch = *y * composition_rect.height();
                        let min = composition_rect.left_top() + egui::vec2(cw, ch);
                        let size =
                            egui::vec2(w * composition_rect.width(), h * composition_rect.height());
                        painter.rect_stroke(egui::Rect::from_min_size(min, size), 0.0, stroke);
                    }
                    crate::scene::Shape::Group { children, .. } => {
                        for child in children {
                            draw_highlight_recursive(
                                painter,
                                child,
                                composition_rect,
                                _zoom,
                                stroke,
                                current_time,
                                actual_spawn,
                            );
                        }
                    }
                }
            }

            // Find the selected node
            let mut current_node: Option<&crate::scene::Shape> = state.scene.get(path[0]);
            for &idx in &path[1..] {
                current_node = match current_node {
                    Some(crate::scene::Shape::Group { children, .. }) => children.get(idx),
                    _ => None,
                };
            }

            if let Some(node) = current_node {
                draw_highlight_recursive(
                    &painter,
                    node,
                    composition_rect,
                    zoom,
                    stroke,
                    state.time,
                    0.0,
                );
            }
        } else if let Some(selected_idx) = state.selected {
            // Backwards-compatible fallback (should rarely be used since we set `selected_node_path` everywhere)
            if let Some(shape) = state.scene.get(selected_idx) {
                if state.time >= shape.spawn_time() {
                    let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 165, 0));
                    match shape {
                        crate::scene::Shape::Circle { x, y, radius, .. } => {
                            let cw = *x * composition_rect.width();
                            let ch = *y * composition_rect.height();
                            let center = composition_rect.left_top() + egui::vec2(cw, ch);
                            let scaled_radius = radius * composition_rect.width();
                            painter.circle_stroke(center, scaled_radius, stroke);
                        }
                        crate::scene::Shape::Rect { x, y, w, h, .. } => {
                            let cw = *x * composition_rect.width();
                            let ch = *y * composition_rect.height();
                            let min = composition_rect.left_top() + egui::vec2(cw, ch);
                            let size = egui::vec2(
                                w * composition_rect.width(),
                                h * composition_rect.height(),
                            );
                            painter.rect_stroke(egui::Rect::from_min_size(min, size), 0.0, stroke);
                        }
                        _ => {}
                    }
                }
            }
        }

        // --- Floating Quick Settings (Top-Left of the Canvas) ---
        // We place this inside the closure to reuse 'grid_origin', 'zoom', and 'rect'
        let mut menu_pos = rect.min;
        menu_pos += egui::vec2(10.0, 10.0); // Margin from top-left

        egui::Area::new("canvas_quick_settings")
            .fixed_pos(menu_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_black_alpha(150))
                    .rounding(4.0)
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;

                            // Color Picker Button
                            let picker_btn = egui::Button::new(
                                egui::RichText::new("📷").size(14.0),
                            )
                            .fill(if state.picker_active {
                                egui::Color32::from_rgb(255, 100, 0)
                            } else {
                                egui::Color32::TRANSPARENT
                            });

                            if ui
                                .add(picker_btn)
                                .on_hover_text("Color Picker & Magnifier")
                                .clicked()
                            {
                                state.picker_active = !state.picker_active;
                            }

                            // Show current picked color
                            let (rect, _response) =
                                ui.allocate_at_least(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                rect.shrink(2.0),
                                2.0,
                                egui::Color32::from_rgb(
                                    state.picker_color[0],
                                    state.picker_color[1],
                                    state.picker_color[2],
                                ),
                            );
                            ui.painter().rect_stroke(
                                rect.shrink(2.0),
                                2.0,
                                egui::Stroke::new(1.0, egui::Color32::GRAY),
                            );

                            ui.separator();

                            let current_label = format!("Preview: {}x", state.preview_multiplier);
                            ui.menu_button(current_label, |ui| {
                                ui.set_width(100.0);
                                let multipliers = [0.125, 0.25, 0.5, 1.0, 1.125, 1.25, 1.5, 2.0];
                                for &m in &multipliers {
                                    let label = format!("{}x", m);
                                    if ui
                                        .selectable_label(state.preview_multiplier == m, label)
                                        .clicked()
                                    {
                                        state.preview_multiplier = m;
                                        ui.close_menu();
                                    }
                                }
                            });

                            ui.separator();

                            ui.add(
                                egui::DragValue::new(&mut state.preview_fps)
                                    .prefix("FPS: ")
                                    .clamp_range(1..=240),
                            );

                            ui.separator();

                            // --- Mouse Coordinates relative to fictitious canvas (Normalized 0.0 - 1.0) ---
                            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                                // Calculate normalized coordinates (0.0 to 1.0) relative to the top-left of the composition_rect
                                let pct_x = (mouse_pos.x - composition_rect.min.x)
                                    / composition_rect.width();
                                let pct_y = (mouse_pos.y - composition_rect.min.y)
                                    / composition_rect.height();

                                ui.label(
                                    egui::RichText::new(format!(
                                        "X: {:.2}%, Y: {:.2}%",
                                        pct_x * 100.0,
                                        pct_y * 100.0
                                    ))
                                    .monospace()
                                    .color(egui::Color32::LIGHT_BLUE),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("X: ---%, Y: ---%")
                                        .monospace()
                                        .color(egui::Color32::GRAY),
                                );
                            }
                        });
                    });
            });
    });
}

#[cfg(feature = "wgpu")]
use eframe::egui_wgpu;
#[cfg(feature = "wgpu")]
use eframe::wgpu;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuShape {
    pos: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    shape_type: i32,
    spawn_time: f32,
    p1: i32,
    p2: i32,
}

#[cfg(feature = "wgpu")]
struct CompositionCallback {
    shapes: Vec<GpuShape>,
    render_width: f32,
    render_height: f32,
    preview_multiplier: f32,
    paper_rect: egui::Rect,
    viewport_rect: egui::Rect,
    // Magnifier / Picker
    magnifier_pos: Option<egui::Pos2>,
    time: f32,
}

#[cfg(feature = "wgpu")]
impl egui_wgpu::CallbackTrait for CompositionCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &mut GpuResources = callback_resources.get_mut().unwrap();

        let shape_data = bytemuck::cast_slice(&self.shapes);
        if shape_data.len() > resources.shape_buffer.size() as usize {
            resources.shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shape_buffer"),
                size: (shape_data.len() * 2 + 1024) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // Rebuild bind group
            resources.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composition_bind_group"),
                layout: &resources.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: resources.shape_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: resources.uniform_buffer.as_entire_binding(),
                    },
                ],
            });
        }

        if !self.shapes.is_empty() {
            queue.write_buffer(&resources.shape_buffer, 0, shape_data);
        }

        // Layout:
        // vec4 resolution (w, h, prev_w, prev_h)
        // vec4 paper_rect (min_x, min_y, max_x, max_y)
        // vec4 viewport_rect (min_x, min_y, max_x, max_y)
        // vec4 count_mag_pos (count, mag_x, mag_y, mag_active)
        let mag_active = if self.magnifier_pos.is_some() {
            1.0
        } else {
            0.0
        };
        let m_pos = self.magnifier_pos.unwrap_or(egui::Pos2::ZERO);

        let mut uniforms: [f32; 20] = [0.0; 20];
        uniforms[0] = self.render_width;
        uniforms[1] = self.render_height;
        uniforms[2] = self.render_width * self.preview_multiplier;
        uniforms[3] = self.render_height * self.preview_multiplier;
        uniforms[4] = self.paper_rect.min.x;
        uniforms[5] = self.paper_rect.min.y;
        uniforms[6] = self.paper_rect.max.x;
        uniforms[7] = self.paper_rect.max.y;
        uniforms[8] = self.viewport_rect.min.x;
        uniforms[9] = self.viewport_rect.min.y;
        uniforms[10] = self.viewport_rect.max.x;
        uniforms[11] = self.viewport_rect.max.y;
        uniforms[12] = self.shapes.len() as f32;
        uniforms[13] = m_pos.x;
        uniforms[14] = m_pos.y;
        uniforms[15] = mag_active;
        uniforms[16] = self.time; // Pass the current time
                                  // 17, 18, 19 remain 0.0 for padding

        queue.write_buffer(
            &resources.uniform_buffer,
            0,
            bytemuck::cast_slice(&uniforms),
        );

        Vec::new()
    }

    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        callback_resources: &'a egui_wgpu::CallbackResources,
    ) {
        let resources: &GpuResources = callback_resources.get().unwrap();
        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.draw(0..6, 0..1); // Draw 2 triangles covering the quad
    }
}

#[cfg(feature = "wgpu")]
pub struct GpuResources {
    pub pipeline: wgpu::RenderPipeline,
    pub shape_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

#[cfg(feature = "wgpu")]
impl GpuResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composition_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "composition.wgsl"
            ))),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composition_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composition_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composition_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape_buffer"),
            size: 1024, // Start small
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform_buffer"),
            size: 80, // 20 * f32
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composition_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: shape_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            pipeline,
            shape_buffer,
            uniform_buffer,
            bind_group,
            bind_group_layout,
        }
    }
}
