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
fn shape_text(_in: VertexOutput, _effective_uv: vec2<f32>) -> vec4<f32> {
    // Normalized coordinate within the quad [0..1]
    let local = _effective_uv * 0.5 + vec2<f32>(0.5, 0.5);
    let u = local.x;
    let v = local.y;
    let offset = u32(_in.p1);
    let len    = u32(_in.p2);
    if (len == 0u) { return vec4<f32>(0.0); }

    // ── Find which glyph owns this UV column ──────────────────────────────────
    var cum: f32 = 0.0;
    var glyph_adv: f32 = 0.0;
    var idx: u32 = offset;
    var i: u32 = 0u;
    for (; i < len; i = i + 1u) {
        let g = glyphs[offset + i];
        idx       = offset + i;
        glyph_adv = g.advance;
        if (u < cum + g.advance) { break; }
        cum = cum + g.advance;
    }
    let g        = glyphs[idx];
    let char_u   = (u - cum) / max(glyph_adv, 1e-6);
    let sample_uv = mix(g.uv0, g.uv1, vec2<f32>(char_u, v));
    let col      = textureSample(text_atlas, text_sampler, sample_uv);

    // ── Decode atlas channels ─────────────────────────────────────────────────
    let coverage = col.a;
    if (coverage < 0.5) { return vec4<f32>(0.0); }

    let pixel_type = col.b;   // 1.0 = outline, ~0.5 = fill
    let stroke_priority = col.r;
    let fill_coverage = col.g;

    // ── Per-character reveal progress ─────────────────────────────────────────
    let lag_ratio   = 0.15;
    let total_slots = f32(len) - (f32(len) - 1.0) * lag_ratio;
    let global_p    = _in.reveal * total_slots;
    let char_start  = f32(i) * (1.0 - lag_ratio);
    let char_progress = clamp(global_p - char_start, 0.0, 1.0);

    // Nothing should be visible before the character animation starts.
    if (char_progress <= 0.0) { return vec4<f32>(0.0); }

    // ── Outline pixel ─────────────────────────────────────────────────────────
    if (pixel_type > 0.75) {
        // Outline draws during 0% → 80% of char_progress.
        let outline_end = 0.8;
        let progress = clamp(char_progress / outline_end, 0.0, 1.0);
        
        let adjusted_priority = stroke_priority * 0.95 + 0.02;
        let window = 0.03;
        let visible = smoothstep(adjusted_priority - window, adjusted_priority, progress);
        if (visible <= 0.0) { return vec4<f32>(0.0); }
        return g.color * visible;
    }

    // ── Fill pixel ────────────────────────────────────────────────────────────
    if (pixel_type > 0.25) {
        // Fill fades in uniformly during 60% → 100% of char_progress.
        let fill_start = 0.6;
        if (char_progress < fill_start) { return vec4<f32>(0.0); }

        let fill_progress = (char_progress - fill_start) / (1.0 - fill_start);
        
        // Use G channel for smoothness/anti-aliasing, and fill_progress for global fade
        return g.color * (fill_coverage * fill_progress);
    }

    return vec4<f32>(0.0);
}

fn rotate_fade(v: f32, start: f32, end: f32) -> f32 {
    return clamp((v - start) / (end - start), 0.0, 1.0);
}
