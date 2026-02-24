// Glyph metadata stored in a flat buffer. Each glyph entry provides the
// UV rectangle in the global glyph atlas and a normalized advance width.
// NOTE: struct Glyph is declared in common.wgsl (included first).
@group(0) @binding(4) var<storage, read> glyphs: array<Glyph>;

// WGSL: text shape helper — renders text by sampling the global glyph atlas.
//
// Atlas channel encoding (set by text_rasterizer):
//   R: stroke priority  [0..255] → normalized to [0..1] — order in which
//      the outline pixel is drawn. 0 = first, 255 = last.
//   G: unused (reserved)
//   B: unused (reserved)
//   A: alpha coverage — only boundary (outline) pixels have alpha > 0.
//      Interior fill pixels are NOT written to the atlas in this step.
//
// write_text Step 1 — only outline, no fill:
//   The shader reveals outline pixels in priority order as `reveal` goes 0→1.
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
    // R channel: stroke priority (0 = first pixel drawn, 1 = last pixel drawn)
    let stroke_priority = col.r;
    // A channel: coverage — only outline pixels have alpha > 0 (no fill stored)
    let coverage        = col.a;

    if (coverage < 0.01) { return vec4<f32>(0.0); }

    // ── Per-character reveal progress ─────────────────────────────────────────
    // Overlap between characters: lag_ratio controls how much chars overlap.
    let lag_ratio   = 0.5;
    let total_slots = f32(len) - (f32(len) - 1.0) * lag_ratio;
    let global_p    = _in.reveal * total_slots;
    let char_start  = f32(i) * (1.0 - lag_ratio);
    let char_progress = clamp(global_p - char_start, 0.0, 1.0);

    // A pixel becomes visible when char_progress reaches its priority value.
    // We use a narrow smoothstep window so the "pen tip" is visible.
    // We clamp the upper bound of the window to 1.0 but allow the window to 
    // "overshoot" slightly so that at char_progress = 1.0, even pixels with 
    // stroke_priority = 1.0 are fully opaque (smoothstep(0.98, 1.0, 1.0) = 1.0).
    let window = 0.02;
    let visible = smoothstep(stroke_priority - window, stroke_priority, char_progress);

    if (visible <= 0.0) { return vec4<f32>(0.0); }

    return g.color * (coverage * visible);
}

fn rotate_fade(v: f32, start: f32, end: f32) -> f32 {
    return clamp((v - start) / (end - start), 0.0, 1.0);
}
