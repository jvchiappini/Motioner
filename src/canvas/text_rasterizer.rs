/// Rasterizador de texto CPU → buffer RGBA.
/// Genera un buffer RGBA8 del tamaño render_width × render_height con todo
/// el texto de la escena dibujado en sus posiciones animadas.
/// El buffer luego se sube como textura a la GPU para que el shader lo muestre
/// con filtrado NEAREST (pixelado, sin resolución "infinita").
use ab_glyph::{Font, FontArc, Glyph, ScaleFont};
use std::collections::HashMap;

/// Candidatos de fuentes del sistema a probar como fallback (Windows primero).
const SYSTEM_FONT_CANDIDATES: &[&str] = &[
    "C:\\Windows\\Fonts\\arial.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "C:\\Windows\\Fonts\\calibri.ttf",
    "C:\\Windows\\Fonts\\verdana.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/Arial.ttf",
];

/// Resultado de la rasterización: píxeles RGBA y dimensiones.
pub struct TextRasterResult {
    pub pixels: Vec<u8>, // RGBA8, tamaño: w * h * 4
    pub width: u32,
    pub height: u32,
}

/// Rasteriza UN elemento `Text` en un buffer RGBA independiente del tamaño `rw × rh`.
/// Retorna `None` si no se dibujó ningún píxel visible (texto invisible o sin fuente).
pub fn rasterize_single_text(
    text_shape: &crate::scene::Shape,
    rw: u32,
    rh: u32,
    current_time: f32,
    project_duration: f32,
    font_arc_cache: &mut HashMap<String, FontArc>,
    font_map: &HashMap<String, std::path::PathBuf>,
    handlers: &[crate::dsl::runtime::DslHandler],
    parent_spawn: f32,
) -> Option<TextRasterResult> {
    if rw == 0 || rh == 0 {
        return None;
    }

    // Asegurar fuente del sistema en caché
    if !font_arc_cache.contains_key("__system__") {
        if let Some(font) = get_system_font(font_map) {
            font_arc_cache.insert("__system__".to_string(), font);
        }
    }

    let mut pixels = vec![0u8; (rw * rh * 4) as usize];
    let mut has_text = false;
    let frame_idx = (current_time * 60.0).round() as u32;

    // Extraer el Text interior
    let t = match text_shape {
        crate::scene::Shape::Text(t) => t,
        _ => return None,
    };

    if !t.visible {
        return None;
    }

    let actual_spawn = text_shape.spawn_time().max(parent_spawn);
    if current_time < actual_spawn {
        return None;
    }

    // Posición animada
    let mut transient = text_shape.clone();
    crate::events::time_changed_event::apply_on_time_handlers(
        std::slice::from_mut(&mut transient),
        handlers,
        current_time,
        frame_idx,
    );
    let (eval_x, eval_y) = crate::animations::animations_manager::animated_xy_for(
        &transient,
        current_time,
        project_duration,
    );

    let x_px = (eval_x * rw as f32).round() as i32;
    let y_px = (eval_y * rh as f32).round() as i32;

    if t.spans.is_empty() {
        let color = t.color;
        let font_name = if t.font == "System" || t.font.is_empty() {
            None
        } else {
            Some(t.font.as_str())
        };
        let font = resolve_font(font_name, font_arc_cache, font_map);
        let size_px = t.size * rh as f32;
        draw_text_to_buffer(
            &mut pixels,
            rw,
            rh,
            &t.value,
            font.as_ref(),
            size_px,
            x_px,
            y_px,
            color,
            &mut has_text,
        );
    } else {
        let mut cursor_x = x_px;
        for span in &t.spans {
            let font_name = if span.font == "System" || span.font.is_empty() {
                None
            } else {
                Some(span.font.as_str())
            };
            let font = resolve_font(font_name, font_arc_cache, font_map);
            let size_px = span.size * rh as f32;
            let advance = draw_text_to_buffer(
                &mut pixels,
                rw,
                rh,
                &span.text,
                font.as_ref(),
                size_px,
                cursor_x,
                y_px,
                span.color,
                &mut has_text,
            );
            cursor_x += advance;
        }
    }

    if has_text {
        Some(TextRasterResult { pixels, width: rw, height: rh })
    } else {
        None
    }
}

/// Rasteriza todos los elementos de tipo `Text` de la escena en un buffer RGBA.
pub fn rasterize_text_layer(
    shapes: &[crate::scene::Shape],
    rw: u32,
    rh: u32,
    current_time: f32,
    project_duration: f32,
    font_arc_cache: &mut HashMap<String, FontArc>,
    font_map: &HashMap<String, std::path::PathBuf>,
    handlers: &[crate::dsl::runtime::DslHandler],
    parent_spawn: f32,
) -> Option<TextRasterResult> {
    if rw == 0 || rh == 0 {
        return None;
    }

    // Asegurar que la fuente del sistema esté cargada en el caché
    if !font_arc_cache.contains_key("__system__") {
        if let Some(font) = get_system_font(font_map) {
            font_arc_cache.insert("__system__".to_string(), font);
        }
    }

    // Buffer transparente (fondo alpha = 0)
    let mut pixels = vec![0u8; (rw * rh * 4) as usize];
    let mut has_text = false;

    let frame_idx = (current_time * 60.0).round() as u32;

    rasterize_recursive(
        shapes,
        &mut pixels,
        rw,
        rh,
        current_time,
        project_duration,
        font_arc_cache,
        font_map,
        handlers,
        parent_spawn,
        frame_idx,
        &mut has_text,
    );

    if has_text {
        Some(TextRasterResult {
            pixels,
            width: rw,
            height: rh,
        })
    } else {
        None
    }
}

fn rasterize_recursive(
    shapes: &[crate::scene::Shape],
    pixels: &mut Vec<u8>,
    rw: u32,
    rh: u32,
    current_time: f32,
    project_duration: f32,
    font_arc_cache: &HashMap<String, FontArc>,
    font_map: &HashMap<String, std::path::PathBuf>,
    handlers: &[crate::dsl::runtime::DslHandler],
    parent_spawn: f32,
    frame_idx: u32,
    has_text: &mut bool,
) {
    for shape in shapes.iter().rev() {
        let actual_spawn = shape.spawn_time().max(parent_spawn);
        if current_time < actual_spawn {
            continue;
        }
        match shape {
            crate::scene::Shape::Group { children, .. } => {
                rasterize_recursive(
                    children,
                    pixels,
                    rw,
                    rh,
                    current_time,
                    project_duration,
                    font_arc_cache,
                    font_map,
                    handlers,
                    actual_spawn,
                    frame_idx,
                    has_text,
                );
            }
            crate::scene::Shape::Text(t) => {
                if !t.visible {
                    continue;
                }

                // Posición animada
                let mut transient = shape.clone();
                crate::events::time_changed_event::apply_on_time_handlers(
                    std::slice::from_mut(&mut transient),
                    handlers,
                    current_time,
                    frame_idx,
                );
                let (eval_x, eval_y) = crate::animations::animations_manager::animated_xy_for(
                    &transient,
                    current_time,
                    project_duration,
                );

                let x_px = (eval_x * rw as f32).round() as i32;
                let y_px = (eval_y * rh as f32).round() as i32;

                if t.spans.is_empty() {
                    // Texto simple
                    let color = t.color;
                    let font_name = if t.font == "System" || t.font.is_empty() {
                        None
                    } else {
                        Some(t.font.as_str())
                    };
                    let font = resolve_font(font_name, font_arc_cache, font_map);
                    let size_px = t.size * rh as f32; // Fracción de altura → píxeles
                    draw_text_to_buffer(
                        pixels,
                        rw,
                        rh,
                        &t.value,
                        font.as_ref(),
                        size_px,
                        x_px,
                        y_px,
                        color,
                        has_text,
                    );
                } else {
                    // Rich spans: dibujamos cada span uno detrás del otro
                    let mut cursor_x = x_px;
                    for span in &t.spans {
                        let font_name = if span.font == "System" || span.font.is_empty() {
                            None
                        } else {
                            Some(span.font.as_str())
                        };
                        let font = resolve_font(font_name, font_arc_cache, font_map);
                        let size_px = span.size * rh as f32; // Fracción de altura → píxeles
                        let advance = draw_text_to_buffer(
                            pixels,
                            rw,
                            rh,
                            &span.text,
                            font.as_ref(),
                            size_px,
                            cursor_x,
                            y_px,
                            span.color,
                            has_text,
                        );
                        cursor_x += advance;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Obtiene o carga una fuente. Para "System"/vacío usa el fallback del sistema.
fn resolve_font(
    name: Option<&str>,
    cache: &HashMap<String, FontArc>,
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
            get_system_font(font_map)
        }
        // "System" o vacío → usar fuente del sistema (primero del caché)
        _ => cache
            .get("__system__")
            .cloned()
            .or_else(|| get_system_font(font_map)),
    }
}

/// Carga la fuente del sistema desde disco (sin caché interno — el caché está en font_arc_cache).
fn get_system_font(font_map: &HashMap<String, std::path::PathBuf>) -> Option<FontArc> {
    // 1. Probar paths hardcodeados del sistema
    for candidate in SYSTEM_FONT_CANDIDATES {
        if let Ok(data) = std::fs::read(candidate) {
            if let Ok(font) = FontArc::try_from_vec(data) {
                eprintln!(
                    "[text_rasterizer] Sistema: cargada fuente desde {}",
                    candidate
                );
                return Some(font);
            }
        }
    }
    // 2. Usar cualquier fuente disponible en font_map como último recurso
    for (_name, path) in font_map {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = FontArc::try_from_vec(data) {
                eprintln!(
                    "[text_rasterizer] Fallback: cargada fuente desde {:?}",
                    path
                );
                return Some(font);
            }
        }
    }
    eprintln!("[text_rasterizer] ADVERTENCIA: No se encontró ninguna fuente del sistema.");
    None
}

/// Dibuja texto en el buffer y retorna el avance horizontal total en píxeles.
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
) -> i32 {
    let Some(font) = font else {
        // Sin fuente disponible: no dibujar nada
        return 0;
    };

    // Escalar la fuente al tamaño en píxeles
    let scale = ab_glyph::PxScale::from(size_pts);
    let scaled = font.as_scaled(scale);

    let ascent = scaled.ascent();
    let mut cursor_x = x as f32;
    let baseline_y = y as f32 + ascent;

    let mut total_advance = 0.0f32;
    let mut prev_glyph: Option<ab_glyph::GlyphId> = None;

    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        // Kerning
        if let Some(prev) = prev_glyph {
            cursor_x += scaled.kern(prev, glyph_id);
            total_advance += scaled.kern(prev, glyph_id);
        }
        prev_glyph = Some(glyph_id);

        let glyph: Glyph =
            glyph_id.with_scale_and_position(scale, ab_glyph::point(cursor_x, baseline_y));

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

        let advance = scaled.h_advance(glyph_id);
        cursor_x += advance;
        total_advance += advance;
    }

    total_advance.round() as i32
}
