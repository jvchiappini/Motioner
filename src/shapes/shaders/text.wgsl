// Glyph metadata stored in a flat buffer. Each glyph entry provides the
// UV rectangle in the global glyph atlas and a normalized advance width.
// NOTE: struct Glyph is declared in common.wgsl (included first).
@group(0) @binding(4) var<storage, read> glyphs: array<Glyph>;

// WGSL: text shape helper — renders text by sampling the global glyph atlas.
//
// Atlas channel encoding (set by text_rasterizer):
//   R: stroke priority  [0..255] → normalized [0..1] — draw order within path.
//   G: interior coverage [0..255] → normalized [0..1] — for anti-aliasing.
//   B: pixel type flag:
//       1.0 (255) = outline pixel → use R for stroke priority.
//       ~0.5 (128) = fill pixel    → uniform fade-in.
//       0.0       = background  → discard.
//   A: coverage — 1.0 for any drawable pixel, 0.0 for background.
//
// write_text animation (Manim style):
//   Phase 1 (0% → 80%): Outline strokes are drawn in priority order.
//   Phase 2 (60% → 100%): Interior fill fades in uniformly.
//   All paths within a character are drawn in parallel.
//   Characters overlap by lag_ratio (next char starts when previous is 85% done).
fn shape_text(_in: VertexOutput, _snapped_uv: vec2<f32>, _raw_uv: vec2<f32>) -> vec4<f32> {
    // Normalized coordinate within the quad [0..1]
    let local_snapped = _snapped_uv * 0.5 + vec2<f32>(0.5, 0.5);
    let local_raw     = _raw_uv * 0.5 + vec2<f32>(0.5, 0.5);

    let u_snapped = local_snapped.x;
    let v_snapped = local_snapped.y;
    let u_raw     = local_raw.x;
    let v_raw     = local_raw.y;

    let offset = u32(_in.p1);
    let len    = u32(_in.p2);
    if (len == 0u) { return vec4<f32>(0.0); }

    // ── Find which glyph owns this UV column (based on snapped X for reveal consistency) ──
    var cum_snapped: f32 = 0.0;
    var glyph_adv: f32 = 0.0;
    var idx: u32 = offset;
    var i: u32 = 0u;
    for (; i < len; i = i + 1u) {
        let g = glyphs[offset + i];
        idx       = offset + i;
        glyph_adv = g.advance;
        if (u_snapped < cum_snapped + g.advance) { break; }
        cum_snapped = cum_snapped + g.advance;
    }
    
    // We also need the cumulative offset for the RAW coordinate to sample texture correctly
    var cum_raw: f32 = 0.0;
    for (var j: u32 = 0u; j < i; j = j + 1u) {
        cum_raw = cum_raw + glyphs[offset + j].advance;
    }

    let g             = glyphs[idx];
    let char_u_raw    = (u_raw - cum_raw) / max(glyph_adv, 1e-6);
    let sample_uv_raw = mix(g.uv0, g.uv1, vec2<f32>(char_u_raw, v_raw));

    // For priority/pixel_type, we sample the SNAPPED position to avoid "shimmering" during reveal.
    let char_u_snapped    = (u_snapped - cum_snapped) / max(glyph_adv, 1e-6);
    let sample_uv_snapped = mix(g.uv0, g.uv1, vec2<f32>(char_u_snapped, v_snapped));
    
    // Antialiasing: sample coverage with the linear sampler from RAW uv,
    // but keep priority/pixel_type with nearest from SNAPPED uv.
    let col_linear  = textureSample(text_atlas, text_linear_sampler, sample_uv_raw);
    let col_nearest = textureSample(text_atlas, text_sampler, sample_uv_snapped);

    // ── Decode atlas channels ─────────────────────────────────────────────────
    let coverage = col_linear.a;
    if (coverage <= 0.001) { return vec4<f32>(0.0); }

    let pixel_type = col_nearest.b;   // 1.0 = outline, ~0.5 = fill
    let stroke_priority = col_nearest.r;
    let fill_coverage = col_linear.g; // use linear for smooth interior edges

    // ── Per-character reveal progress ─────────────────────────────────────────
    let lag_ratio   = 0.15;
    let total_slots = f32(len) - (f32(len) - 1.0) * lag_ratio;
    let global_p    = _in.reveal * total_slots;
    let char_start  = f32(i) * (1.0 - lag_ratio);
    let char_progress = clamp(global_p - char_start, 0.0, 1.0);

    // Nothing should be visible before the character animation starts.
    if (char_progress <= 0.0) { return vec4<f32>(0.0); }

    // ── Animation progress calculations ──────────────────────────────────────
    let outline_end = 0.8;
    let fill_start  = 0.6;
    let window      = 0.03;

    // 1. Stroke visibility (priority-based sequential reveal)
    let stroke_progress = clamp(char_progress / outline_end, 0.0, 1.0);
    let adjusted_priority = stroke_priority * 0.95 + 0.02;
    let stroke_visible = smoothstep(adjusted_priority - window, adjusted_priority, stroke_progress);
    
    // Only apply stroke reveal if this pixel is actually marked as an outline pixel
    // (B channel >= 0.75). Otherwise, it's purely a fill pixel.
    let is_outline = step(0.75, pixel_type);
    let stroke_alpha = stroke_visible * coverage * is_outline;

    // 2. Fill visibility (global character fade-in)
    let fill_progress = clamp((char_progress - fill_start) / (1.0 - fill_start), 0.0, 1.0);
    let fill_alpha = fill_progress * fill_coverage;

    // 3. Unified result: Union of stroke and fill
    // This closes any gaps between the path splatting and the fill area.
    let final_alpha = max(stroke_alpha, fill_alpha);

    if (final_alpha <= 0.001) { return vec4<f32>(0.0); }
    return g.color * final_alpha;
}

fn rotate_fade(v: f32, start: f32, end: f32) -> f32 {
    return clamp((v - start) / (end - start), 0.0, 1.0);
}
