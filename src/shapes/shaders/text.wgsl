// WGSL: text shape helper — samples the supplied text atlas using the interpolated UV.
fn shape_text(_in: VertexOutput, _effective_uv: vec2<f32>) -> vec4<f32> {
    // _in.tex_uv is already interpolated to the atlas region for this shape
    return textureSample(text_atlas, text_sampler, _in.tex_uv);
}
