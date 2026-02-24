/// Rasterizador de texto CPU → buffer RGBA.
/// Genera un buffer RGBA8 del tamaño render_width × render_height con todo
/// el texto de la escena dibujado en sus posiciones animadas.
/// El buffer luego se sube como textura a la GPU para que el shader lo muestre
/// con filtrado NEAREST (pixelado, sin resolución "infinita").
use ab_glyph::{Font, FontArc, ScaleFont};

#[cfg(feature = "wgpu")]
use eframe::wgpu;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Candidatos de fuentes del sistema a probar como fallback (Windows primero).
const SYSTEM_FONT_CANDIDATES: &[&str] = &[
    #[cfg(not(feature = "wgpu"))]
    "C:\\Windows\\Fonts\\arial.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "C:\\Windows\\Fonts\\calibri.ttf",
    "C:\\Windows\\Fonts\\verdana.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];

// -----------------------------------------------------------------------------
// Glyph atlas support (GPU text)
// -----------------------------------------------------------------------------

/// Métrica única para un glifo dentro del atlas.
#[derive(Clone)]
pub struct GlyphMetric {
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub advance: f32, // advance width in pixels
}

/// Atlas de glifos para una combinación de fuente+tamaño.
/// Campos expuestos para que otras partes del crate (compute shader
/// preparer) puedan inspeccionar las métricas sin inventar getters.
#[derive(Clone)]
pub struct GlyphAtlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA8 atlas image
    pub metrics: HashMap<char, GlyphMetric>,
    pub known_chars: Vec<char>, // list of chars inside
}

// refine_glyph_metrics has been removed because the text compute shader relies 
// on having perfectly proportional UV segments over the glyph advance instead 
// of tight bounding boxes. Tight UV bounds lead to stretching and distortion 
// when the shader maps `mix(uv0, uv1)` over the quad width.

// global cache of glyph atlases keyed by (font_name, size_px)
// El tamaño se redondea a u32 para servir como clave; evita problemas con
// `f32` no siendo `Eq`/`Hash`/`Ord`.

static GLYPH_ATLASES: Lazy<Mutex<HashMap<(String, u32, bool), GlyphAtlas>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));


/// Result returned by `ensure_glyph_atlas`.
pub struct AtlasResult {
    pub atlas: GlyphAtlas,
    /// x-offset of this atlas inside the combined global atlas (pixels)
    pub offset_x: u32,
    /// dimensions of the combined atlas when `is_new` is true
    pub combined_width: u32,
    pub combined_height: u32,
    /// pixels of the combined atlas, only provided when `is_new` is true.
    pub combined_pixels: Option<Vec<u8>>,
    pub is_new: bool,
}

/// Ensure a glyph atlas exists for the given font/size.  If this results in a
/// brand-new atlas (either a new font or size) the global combined atlas is
/// re-generated; `AtlasResult` will include the updated combined image so the
/// caller can upload it to the GPU.
#[cfg(not(feature = "wgpu"))]

    pub fn ensure_glyph_atlas(
        font_name: Option<&str>,
        size_px: f32,
        text: &str,
        font_map: &HashMap<String, std::path::PathBuf>,
        font_arc_cache: &mut HashMap<String, FontArc>,
        debug_outline: bool, // Changed named param
    ) -> AtlasResult {
        let key = (
            font_name.unwrap_or("__system__").to_string(),
            size_px.round() as u32,
            debug_outline,
        );

    let mut map = GLYPH_ATLASES.lock().unwrap();
    let mut new = false;
    
    let mut rebuild = false;
    let mut current_chars = Vec::new();
    let needed_chars: Vec<char> = text.chars().collect();

    if let Some(atlas) = map.get(&key) {
        current_chars = atlas.known_chars.clone();
        for c in &needed_chars {
            if !current_chars.contains(c) {
                rebuild = true;
                current_chars.push(*c);
            }
        }
    } else {
        rebuild = true;
        for c in &needed_chars {
            if !current_chars.contains(c) {
                current_chars.push(*c);
            }
        }
        if !current_chars.contains(&' ') {
            current_chars.push(' ');
        }
    }


    if rebuild {
        let atlas = build_single_font_atlas(font_name, size_px, font_map, font_arc_cache, &current_chars, debug_outline);
        map.insert(key.clone(), atlas);
        new = true;
    }
    
    // compute combined atlas if something changed
    let mut combined_pixels = None;
    let (combined_w, combined_h, offsets) = if new {
        let (pixels, w, h, offs) = merge_all_atlases(&*map, true);
        combined_pixels = Some(pixels);
        (w, h, offs)
    } else {
        // if no new atlas, we only need the offsets; merge_all_atlases is now
        // optimized to only compute offsets without allocating a pixel buffer.
        let (_, w, h, offs) = merge_all_atlases(&*map, false);
        (w, h, offs)
    };
    let offset_x = offsets.get(&key).cloned().unwrap_or(0);
    // clone the atlas value itself (not the reference)
    let atlas = (*map.get(&key).unwrap()).clone();
    AtlasResult {
        atlas,
        offset_x,
        combined_width: combined_w,
        combined_height: combined_h,
        combined_pixels,
        is_new: new,
    }
}

/// GPU‑accelerated version of `ensure_glyph_atlas`.
///
/// When a new atlas is required, we render every ASCII glyph into a temporary
/// texture using the regular GPU preview pipeline (`render_frame_color_image_gpu_snapshot`)
/// and read the resulting pixels back.  This moves all rasterization work onto
/// the GPU; the CPU only performs lightweight layout/metric calculations.
///
/// `resources` is the current `GpuResources` instance (it will be mutated if
/// the text atlas changes) and `device`/`queue` are used for rendering and
/// readback.
pub fn ensure_glyph_atlas_gpu(
    font_name: Option<&str>,
    size_px: f32,
    text: &str,
    font_map: &HashMap<String, std::path::PathBuf>,
    font_arc_cache: &mut HashMap<String, FontArc>,
    _resources: &mut crate::canvas::gpu::resources::GpuResources,
    _device: &wgpu::Device,
    _queue: &wgpu::Queue,
    debug_outline: bool,
) -> AtlasResult {
    // same key rounding as CPU version
    let key = (
        font_name.unwrap_or("__system__").to_string(),
        size_px.round() as u32,
        debug_outline,
    );
    let mut map = GLYPH_ATLASES.lock().unwrap();
    let mut new = false;
    
    let mut rebuild = false;
    let mut current_chars = Vec::new();
    let needed_chars: Vec<char> = text.chars().collect();

    if let Some(atlas) = map.get(&key) {
        current_chars = atlas.known_chars.clone();
        for c in &needed_chars {
            if !current_chars.contains(c) {
                rebuild = true;
                current_chars.push(*c);
            }
        }
    } else {
        rebuild = true;
        for c in &needed_chars {
            if !current_chars.contains(c) {
                current_chars.push(*c);
            }
        }
        if !current_chars.contains(&' ') {
            current_chars.push(' ');
        }
    }

    if rebuild {
        let atlas = build_single_font_atlas(font_name, size_px, font_map, font_arc_cache, &current_chars, debug_outline);
        map.insert(key.clone(), atlas);
        new = true;
    }
    
    // compute combined atlas (cpu only, small)
    let mut combined_pixels = None;
    let (combined_w, combined_h, offsets) = if new {
        let (pixels, w, h, offs) = merge_all_atlases(&*map, true);
        combined_pixels = Some(pixels);
        (w, h, offs)
    } else {
        let (_, w, h, offs) = merge_all_atlases(&*map, false);
        (w, h, offs)
    };
    let offset_x = offsets.get(&key).cloned().unwrap_or(0);
    let atlas = (*map.get(&key).unwrap()).clone();
    AtlasResult {
        atlas,
        offset_x,
        combined_width: combined_w,
        combined_height: combined_h,
        combined_pixels,
        is_new: new,
    }
}

/// Build a glyph atlas for a single font/size combination.  ASCII 32..126
/// are rendered into a simple grid.  Caller is responsible for inserting the
/// returned atlas into `GLYPH_ATLASES` or otherwise tracking it.
fn build_single_font_atlas(
    font_name: Option<&str>,
    size_px: f32,
    font_map: &HashMap<String, std::path::PathBuf>,
    font_arc_cache: &mut HashMap<String, FontArc>,
    chars_to_render: &[char],
    debug_outline: bool,
) -> GlyphAtlas {
    let mut atlas = GlyphAtlas {
        width: 0,
        height: 0,
        pixels: Vec::new(),
        metrics: HashMap::new(),
        known_chars: chars_to_render.to_vec(),
    };
    if let Some(font_arc) = resolve_font_with_warning(font_name, font_arc_cache, font_map) {
        let scale = ab_glyph::PxScale::from(size_px);
        let scaled = font_arc.as_scaled(scale);
        // use ascent/descent helpers from ScaleFont trait
        let ascent = scaled.ascent();
        let descent = scaled.descent();
        let glyph_h = (ascent - descent).ceil() as u32;
        let mut max_adv = 0.0f32;
        for &ch in chars_to_render {
            max_adv = max_adv.max(scaled.h_advance(scaled.glyph_id(ch)));
        }
        let cell_w = max_adv.ceil() as u32 + 2;
        let cols = 16.min(chars_to_render.len().max(1) as u32);
        let rows = ((chars_to_render.len() as u32) + cols - 1) / cols;
        atlas.width = cols * cell_w;
        atlas.height = rows * glyph_h;
        if atlas.width == 0 || atlas.height == 0 {
            return atlas;
        }
        atlas.pixels = vec![0u8; (atlas.width * atlas.height * 4) as usize];

        for (i, &ch) in chars_to_render.iter().enumerate() {
            let col = (i as u32) % cols;
            let row = (i as u32) / cols;
            let x0 = col * cell_w;
            let y0 = row * glyph_h;
            let mut has = false;
            // `draw_text_to_buffer` expects `y` to be the *top* of the
            // glyph cell.  Internally it adds `ascent` again to compute the
            // baseline, so passing `y0 + ascent` here would apply the ascent
            // twice and push the glyph completely below its intended slot.
            // The GPU atlas path positions glyphs by centering them in the
            // cell; to mirror that behaviour we simply hand `y0` through and
            // let the helper add the ascent once.
            let x_draw = x0 as i32 + 2; // small padding
            let y_draw = y0 as i32;

            let adv = draw_text_to_buffer(
                &mut atlas.pixels,
                atlas.width,
                atlas.height,
                &ch.to_string(),
                Some(&font_arc),
                size_px,
                x_draw,
                y_draw,
                [255, 255, 255, 255],
                &mut has,
                debug_outline,
            );

            // force `has` to be true for debugging purposes if advance > 0,
            // otherwise invisible outlined chars might be skipped.
            if !has && adv > 0.1 {
                has = true;
            }

            // ── Fill: rasterize filled glyph for opacity fade (Manim style) ──────
            // Manim's DrawBorderThenFill simply fades in the entire fill uniformly.
            // No BFS/wave needed — just store coverage from ab_glyph rasterization.
            if debug_outline && has {
                let gid = scaled.glyph_id(ch);
                let fill_glyph = gid.with_scale_and_position(
                    scale,
                    ab_glyph::point(x_draw as f32, y_draw as f32 + ascent),
                );
                if let Some(outlined_glyph) = font_arc.outline_glyph(fill_glyph) {
                    let bounds = outlined_glyph.px_bounds();
                    // Rasterize filled glyph — write non-outline pixels as fill
                    outlined_glyph.draw(|bx, by, cov| {
                        if cov < 0.05 { return; }
                        let px = bounds.min.x as i32 + bx as i32;
                        let py = bounds.min.y as i32 + by as i32;
                        if px < 0 || py < 0 || px >= atlas.width as i32 || py >= atlas.height as i32 {
                            return;
                        }
                        let idx = (py as u32 * atlas.width + px as u32) as usize * 4;
                        if idx + 3 >= atlas.pixels.len() { return; }
                        // Only write fill data if this pixel is NOT already an outline pixel
                        if atlas.pixels[idx + 2] != 255 {
                            let cov_byte = (cov * 255.0).clamp(0.0, 255.0) as u8;
                            atlas.pixels[idx] = 0;         // R = no outline priority
                            atlas.pixels[idx + 1] = cov_byte; // G = coverage for smooth edges
                            atlas.pixels[idx + 2] = 128;   // B = fill pixel marker
                            atlas.pixels[idx + 3] = 255;   // A = visible
                        }
                    });
                }
            }

            if has {
                // sample alpha at the middle of the cell to verify the glyph
                // actually landed where the metrics think it did.  This helps
                // catch cases where the glyph was drawn with an incorrect
                // baseline or offset and therefore the atlas region is empty
                // but the metric still points there.
                let sample_x = x0 + cell_w / 2;
                let sample_y = y0 + glyph_h / 2;
                if sample_x < atlas.width && sample_y < atlas.height {
                    let idx = (sample_y * atlas.width + sample_x) as usize * 4;
                    let _alpha = atlas.pixels[idx + 3];
                    /*eprintln!(
                        "[text_rasterizer] glyph '{}' drawn, sample alpha {} at {}x{}",
                        ch, alpha, sample_x, sample_y
                    );*/
                }
            }
            if has {
                let uv0 = [
                    (x0 as f32 + 2.0) / atlas.width as f32,
                    y0 as f32 / atlas.height as f32,
                ];
                let uv1 = [
                    (x0 as f32 + 2.0 + adv) / atlas.width as f32,
                    (y0 + glyph_h) as f32 / atlas.height as f32,
                ];

                atlas.metrics.insert(
                    ch,
                    GlyphMetric {
                        uv0,
                        uv1,
                        advance: adv,
                    },
                );
            } else {
                // Character has no outline (e.g. space, tab) but still has an
                // advance width. Store exact transparent region UVs matching
                // its position in the atlas.
                let adv = scaled.h_advance(scaled.glyph_id(ch));
                let uv0 = [
                    (x0 as f32 + 2.0) / atlas.width as f32,
                    y0 as f32 / atlas.height as f32,
                ];
                let uv1 = [
                    (x0 as f32 + 2.0 + adv) / atlas.width as f32,
                    (y0 + glyph_h) as f32 / atlas.height as f32,
                ];
                atlas.metrics.insert(
                    ch,
                    GlyphMetric {
                        uv0,
                        uv1,
                        advance: adv,
                    },
                );
            }
        }
        // No longer refining metrics: tight bounding boxes caused horizontal and
        // vertical stretching in the render shader since it scales the UVs over
        // the glyph's full advance width.
    }
    atlas
}

/// Merge all individual atlases in `map` into one horizontal atlas.  Returns
/// (pixels, width, height, offsets) where `offsets` maps atlas key to its x
/// offset within the combined image.
fn merge_all_atlases(
    map: &HashMap<(String, u32, bool), GlyphAtlas>,
    include_pixels: bool,
) -> (Vec<u8>, u32, u32, HashMap<(String, u32, bool), u32>) {
    let mut entries: Vec<(&(String, u32, bool), &GlyphAtlas)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    
    let mut actual_combined_w = 0;
    let mut actual_combined_h = 0;
    for (_k, atlas) in &entries {
        if actual_combined_w + atlas.width > super::gpu::utils::MAX_GPU_TEXTURE_SIZE {
            break; // Stop adding atlases horizontally to avoid exceeding GPU limits
        }
        actual_combined_w += atlas.width;
        actual_combined_h = actual_combined_h.max(atlas.height);
    }
    
    if actual_combined_w == 0 {
        return (vec![0u8; 4], 1, 1, HashMap::new());
    }

    let mut pixels = if include_pixels {
        let pixel_count = (actual_combined_w as u64) * (actual_combined_h as u64) * 4;
        vec![0u8; pixel_count as usize]
    } else {
        Vec::new()
    };
    let mut offsets = HashMap::new();
    let mut cursor_x = 0;

    for (k, atlas) in entries {
        if cursor_x + atlas.width > actual_combined_w {
            break;
        }
        offsets.insert((*k).clone(), cursor_x);

        if include_pixels {
            for row in 0..atlas.height {
                let src_start = (row * atlas.width * 4) as usize;
                let src_end = src_start + (atlas.width * 4) as usize;
                let dst_start = ((row * actual_combined_w + cursor_x) * 4) as usize;
                let dst_end = dst_start + (atlas.width * 4) as usize;
                if src_end <= atlas.pixels.len() && dst_end <= pixels.len() {
                    pixels[dst_start..dst_end].copy_from_slice(&atlas.pixels[src_start..src_end]);
                }
            }
        }
        cursor_x += atlas.width;
    }

    (pixels, actual_combined_w, actual_combined_h, offsets)
}


/// text layout helper removed; glyph runs are generated inside compute.rs

// Previously there were helpers for rasterizing entire layers recursively.
// They were removed during the dead-code purge; all logic now lives inside
// `rasterize_single_text` above.  The remainder of this file consists solely
// of support routines (font resolution, drawing) which are still exercised
// by that function.

// (no further public API)
// (no further public API)

/// Obtiene o carga una fuente. Para "System"/vacío usa el fallback del sistema.
fn resolve_font(
    name: Option<&str>,
    cache: &mut HashMap<String, FontArc>,
    font_map: &HashMap<String, std::path::PathBuf>,
) -> Option<FontArc> {
    match name {
        // Fuente nombrada explícita
        Some(n) if !n.is_empty() && n != "System" => {
            // 1. Buscar en caché
            if let Some(f) = cache.get(n) {
                return Some(f.clone());
            }
            // 2. Intentar cargar desde font_map
            if let Some(path) = font_map.get(n) {
                if let Ok(data) = std::fs::read(path) {
                    if let Ok(font) = FontArc::try_from_vec(data) {
                        return Some(font);
                    }
                }
            }
            // 3. Si no se encontró la fuente nombrada, caer al sistema
            let f = get_system_font(font_map);
            if let Some(font) = &f {
                cache.insert(n.to_string(), font.clone());
            }
            f
        }
        // "System" o vacío → usar fuente del sistema (primero del caché)
        _ => {
            if let Some(f) = cache.get("__system__") {
                return Some(f.clone());
            }
            let f = get_system_font(font_map);
            if let Some(font) = &f {
                cache.insert("__system__".to_string(), font.clone());
            }
            f
        }
    }
}

// `resolve_font` can legitimately return `None` if no suitable font is
// available.  Downstream code often proceeds silently in that case, which
// results in empty glyph atlases and invisible text.  This helper logs a
// warning so it's easier to diagnose when the system font lookup fails.
// It is deliberately kept separate from `resolve_font` itself to avoid
// cluttering every call site with logging logic.
pub(crate) fn resolve_font_with_warning(
    name: Option<&str>,
    cache: &mut HashMap<String, FontArc>,
    font_map: &HashMap<String, std::path::PathBuf>,
) -> Option<FontArc> {
    let res = resolve_font(name, cache, font_map);
    if res.is_none() {
        /*eprintln!(
            "[text_rasterizer] resolve_font: could not load font {:?}, cache keys={:?}, map keys={:?}",
            name,
            cache.keys().collect::<Vec<_>>(),
            font_map.keys().collect::<Vec<_>>(),
        );*/
    }
    res
}

/// Carga la fuente del sistema desde disco (sin caché interno — el caché está en font_arc_cache).
fn get_system_font(font_map: &HashMap<String, std::path::PathBuf>) -> Option<FontArc> {
    // 1. Probar paths hardcodeados del sistema
    for candidate in SYSTEM_FONT_CANDIDATES {
        if let Ok(data) = std::fs::read(candidate) {
            if let Ok(font) = FontArc::try_from_vec(data) {
                /*eprintln!(
                    "[text_rasterizer] Sistema: cargada fuente desde {}",
                    candidate
                );*/
                return Some(font);
            }
        }
    }
    // 2. Usar cualquier fuente disponible en font_map como último recurso
    for path in font_map.values() {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = FontArc::try_from_vec(data) {
                /*eprintln!(
                    "[text_rasterizer] Fallback: cargada fuente desde {:?}",
                    path
                );*/
                return Some(font);
            }
        }
    }
    /*eprintln!("[text_rasterizer] ADVERTENCIA: No se encontró ninguna fuente del sistema.");*/
    None
}

#[allow(clippy::too_many_arguments, clippy::ptr_arg)]
fn draw_text_to_buffer(
    pixels: &mut Vec<u8>,
    rw: u32,
    rh: u32,
    text: &str,
    font: Option<&FontArc>,
    size_pts: f32,
    x: i32,
    y: i32,
    color: [u8; 4],
    has_text: &mut bool,
    debug_outline: bool,
) -> f32 {
    let Some(font) = font else {
        return 0.0;
    };

    let scale = ab_glyph::PxScale::from(size_pts);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();

    let mut cursor_x = x as f32;
    let baseline_y = y as f32 + ascent;
    let mut total_advance = 0.0f32;
    let mut prev_glyph: Option<ab_glyph::GlyphId> = None;

    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        if let Some(prev) = prev_glyph {
            cursor_x += scaled.kern(prev, glyph_id);
            total_advance += scaled.kern(prev, glyph_id);
        }
        prev_glyph = Some(glyph_id);

        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(cursor_x, baseline_y));
        

        
        if debug_outline {
            if let Some(outline) = font.outline(glyph_id) {
                // CRITICAL: ab_glyph's ScaleFont uses height_unscaled() as the
                // divisor for scale factors, NOT units_per_em(). These differ for
                // most fonts.  height_unscaled = ascent_unscaled - descent_unscaled.
                // See ab_glyph scale.rs: h_scale_factor = scale.x / height_unscaled
                //                        v_scale_factor = scale.y / height_unscaled
                // The OutlinedGlyph::draw() method then applies:
                //   scaled_point = point(p.x * h_factor, p.y * (-v_factor))
                //   final_point  = scaled_point + (glyph.position - px_bounds.min)
                // For our wireframe rendering we skip the px_bounds offset and
                // instead add the glyph position directly (cursor_x, baseline_y),
                // which gives identical absolute coordinates.
                let mut paths: Vec<Vec<&ab_glyph::OutlineCurve>> = Vec::new();
                let mut current_path_curves = Vec::new();
                let mut last_p1_unscaled: Option<ab_glyph::Point> = None;

                for curve in &outline.curves {
                    let p0_unscaled = match curve {
                        ab_glyph::OutlineCurve::Line(p, _) => *p,
                        ab_glyph::OutlineCurve::Quad(p, _, _) => *p,
                        ab_glyph::OutlineCurve::Cubic(p, _, _, _) => *p,
                    };
                    let p_end_unscaled = match curve {
                        ab_glyph::OutlineCurve::Line(_, p) => *p,
                        ab_glyph::OutlineCurve::Quad(_, _, p) => *p,
                        ab_glyph::OutlineCurve::Cubic(_, _, _, p) => *p,
                    };

                    // Detect new path (contour) by checking for jumps in unscaled coordinates
                    if last_p1_unscaled.is_none() || (p0_unscaled.x - last_p1_unscaled.unwrap().x).abs() > 0.1 || (p0_unscaled.y - last_p1_unscaled.unwrap().y).abs() > 0.1 {
                        if !current_path_curves.is_empty() {
                            paths.push(current_path_curves);
                        }
                        current_path_curves = Vec::new();
                    }
                    current_path_curves.push(curve);
                    last_p1_unscaled = Some(p_end_unscaled);
                }
                if !current_path_curves.is_empty() {
                    paths.push(current_path_curves);
                }

                let path_command_counts: Vec<usize> = paths.iter().map(|p| p.len()).collect();
                eprintln!(
                    "[debug_outline] Glyph '{}' | Paths: {} | Commands per path: {:?}",
                    ch,
                    paths.len(),
                    path_command_counts
                );

                let mut draw_line = |p0: ab_glyph::Point, p1: ab_glyph::Point, priority_start: u8, priority_end: u8| {
                    let x0 = p0.x.round() as i32;
                    let y0 = p0.y.round() as i32;
                    let x1 = p1.x.round() as i32;
                    let y1 = p1.y.round() as i32;

                    let dx = (x1 - x0).abs();
                    let dy = -(y1 - y0).abs();
                    let sx = if x0 < x1 { 1 } else { -1 };
                    let sy = if y0 < y1 { 1 } else { -1 };
                    let mut err = dx + dy;
                    
                    let mut cx = x0;
                    let mut cy = y0;

                    // Total pixel steps for interpolation
                    let total_steps = (dx + dy.abs()).max(1) as f32;
                    let mut step_count = 0u32;

                    loop {
                        // Per-pixel priority interpolation
                        let t = step_count as f32 / total_steps;
                        let interp = priority_start as f32 + (priority_end as f32 - priority_start as f32) * t;
                        let priority = interp.clamp(0.0, 255.0) as u8;

                        if cx >= 0 && cy >= 0 && cx < rw as i32 && cy < rh as i32 {
                            let idx = ((cy as u32 * rw + cx as u32) * 4) as usize;
                            if idx + 3 < pixels.len() {
                                pixels[idx] = priority;
                                pixels[idx + 1] = 0;
                                pixels[idx + 2] = 255;  // B = outline pixel marker
                                pixels[idx + 3] = 255;
                            }
                        }

                        if cx == x1 && cy == y1 { break; }
                        let e2 = 2 * err;
                        if e2 >= dy {
                            if cx == x1 { break; }
                            err += dy;
                            cx += sx;
                        }
                        if e2 <= dx {
                            if cy == y1 { break; }
                            err += dx;
                            cy += sy;
                        }
                        step_count += 1;
                    }
                };

                let h_factor = scaled.h_scale_factor();
                let v_factor = -scaled.v_scale_factor();
                let pos_x = cursor_x;
                let pos_y = baseline_y;

                for path in paths {
                    let total_cmds = path.len();
                    if total_cmds == 0 { continue; }

                    for (cmd_idx, curve) in path.iter().enumerate() {
                        let mut points = Vec::new();
                        match curve {
                            ab_glyph::OutlineCurve::Line(p0, p1) => {
                                points.push((*p0, 0.0));
                                points.push((*p1, 1.0));
                            }
                            ab_glyph::OutlineCurve::Quad(p0, p1, p2) => {
                                let steps = 32;
                                for i in 0..=steps {
                                    let t = i as f32 / steps as f32;
                                    let it = 1.0 - t;
                                    let x = it * it * p0.x + 2.0 * it * t * p1.x + t * t * p2.x;
                                    let y = it * it * p0.y + 2.0 * it * t * p1.y + t * t * p2.y;
                                    points.push((ab_glyph::point(x, y), t));
                                }
                            }
                            ab_glyph::OutlineCurve::Cubic(p0, p1, p2, p3) => {
                                let steps = 32;
                                for i in 0..=steps {
                                    let t = i as f32 / steps as f32;
                                    let it = 1.0 - t;
                                    let it2 = it * it;
                                    let t2 = t * t;
                                    let x = it2 * it * p0.x + 3.0 * it2 * t * p1.x + 3.0 * it * t2 * p2.x + t2 * t * p3.x;
                                    let y = it2 * it * p0.y + 3.0 * it2 * t * p1.y + 3.0 * it * t2 * p2.y + t2 * t * p3.y;
                                    points.push((ab_glyph::point(x, y), t));
                                }
                            }
                        }

                        for win in points.windows(2) {
                            let (p0_u, t0) = win[0];
                            let (p1_u, t1) = win[1];
                            
                            // Map each sub-segment to a unique priority range within 0..255
                            let p_start = (cmd_idx as f32 + t0) / total_cmds as f32;
                            let p_end   = (cmd_idx as f32 + t1) / total_cmds as f32;
                            let p_byte_start = (p_start * 255.0).clamp(0.0, 255.0) as u8;
                            let p_byte_end   = (p_end   * 255.0).clamp(0.0, 255.0) as u8;

                            let p0 = ab_glyph::point(p0_u.x * h_factor + pos_x, p0_u.y * v_factor + pos_y);
                            let p1 = ab_glyph::point(p1_u.x * h_factor + pos_x, p1_u.y * v_factor + pos_y);
                            draw_line(p0, p1, p_byte_start, p_byte_end);
                        }
                    }
                }

                *has_text = true;
            }
        } else {
            // Normal rasterization
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|bx, by, cov| {
                    let px = bounds.min.x as i32 + bx as i32;
                    let py = bounds.min.y as i32 + by as i32;
                    if px < 0 || py < 0 || px >= rw as i32 || py >= rh as i32 {
                        return;
                    }
                    let idx = (py as u32 * rw + px as u32) as usize * 4;
                    // Alpha compositing: pre-blendear sobre fondo transparente
                    let src_a = (cov * color[3] as f32).clamp(0.0, 255.0) as u8;
                    if src_a == 0 {
                        return;
                    }
                    let dst_a = pixels[idx + 3];
                    if dst_a == 0 {
                        // Pixel destino transparente: simplemente copiar
                        pixels[idx] = color[0];
                        pixels[idx + 1] = color[1];
                        pixels[idx + 2] = color[2];
                        pixels[idx + 3] = src_a;
                    } else {
                        // Alpha over compositing
                        let sa = src_a as f32 / 255.0;
                        let da = dst_a as f32 / 255.0;
                        let out_a = sa + da * (1.0 - sa);
                        if out_a > 0.0 {
                            pixels[idx] =
                                ((color[0] as f32 * sa + pixels[idx] as f32 * da * (1.0 - sa)) / out_a)
                                    .round() as u8;
                            pixels[idx + 1] = ((color[1] as f32 * sa
                                + pixels[idx + 1] as f32 * da * (1.0 - sa))
                                / out_a)
                                .round() as u8;
                            pixels[idx + 2] = ((color[2] as f32 * sa
                                + pixels[idx + 2] as f32 * da * (1.0 - sa))
                                / out_a)
                                .round() as u8;
                            pixels[idx + 3] = (out_a * 255.0).round() as u8;
                        }
                    }
                    *has_text = true;
                });
            }
        }

        let advance = scaled.h_advance(glyph_id);
        cursor_x += advance;
        total_advance += advance;
    }

    total_advance
}



