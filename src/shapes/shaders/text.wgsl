// Glyph metadata stored in a flat buffer.  Each glyph entry provides the
// UV rectangle in the global glyph atlas and a normalized advance width.
struct Glyph {
    uv0: vec2<f32>,
    uv1: vec2<f32>,
    advance: f32,
    _pad_align_0: f32,
    _pad_align_1: f32,
    _pad_align_2: f32,
    color: vec4<f32>,
    _pad: vec4<f32>,
}

@group(0) @binding(4) var<storage, read> glyphs: array<Glyph>;

// WGSL: text shape helper — renders text by sampling the global glyph atlas
// according to the glyph sequence referenced by `p1`/`p2` in the shape.
fn shape_text(_in: VertexOutput, _effective_uv: vec2<f32>) -> vec4<f32> {
    // normalized coordinate within the quad [0..1]
    let local = _effective_uv * 0.5 + vec2<f32>(0.5, 0.5);
    let u = local.x;
    let v = local.y;
    let offset = u32(_in.p1);
    let len = u32(_in.p2);
    if (len == 0u) {
        return vec4<f32>(0.0);
    }

    var cum: f32 = 0.0;
    var glyph_adv: f32 = 0.0;
    var idx: u32 = offset;
    for (var i: u32 = 0u; i < len; i = i + 1u) {
        let g = glyphs[offset + i];
        idx = offset + i;          // always track last visited glyph
        glyph_adv = g.advance;     // keep last advance in case u > total width
        if (u < cum + g.advance) {
            break;
        }
        cum = cum + g.advance;
    }
    let g = glyphs[idx];
    let rel = (u - cum) / max(glyph_adv, 1e-6);
    let sample_uv = mix(g.uv0, g.uv1, vec2<f32>(rel, v));
    let col = textureSample(text_atlas, text_sampler, sample_uv);
    return col * g.color;
}
