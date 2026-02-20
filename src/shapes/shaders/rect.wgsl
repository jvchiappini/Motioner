// WGSL: rect shape fragment helper
fn shape_rect(in: VertexOutput, effective_uv: vec2<f32>) -> vec4<f32> {
    // Rect implicit from -1..1 in both axes
    let d = max(abs(effective_uv.x), abs(effective_uv.y)) - 1.0;
    if (d > 0.0) {
        return vec4<f32>(in.color.rgb, 0.0);
    } else {
        return vec4<f32>(in.color.rgb, in.color.a);
    }
}
