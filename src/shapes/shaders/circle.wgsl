// WGSL: circle shape fragment helper
// Implement the same visual behaviour as the previous inlined circle SDF.
fn shape_circle(in: VertexOutput, effective_uv: vec2<f32>) -> vec4<f32> {
    // Same logic as before: circle centered at origin with radius == 1.0 (quad extent)
    let d = length(effective_uv) - 1.0;
    if (d > 0.0) {
        return vec4<f32>(in.color.rgb, 0.0);
    } else {
        return vec4<f32>(in.color.rgb, in.color.a);
    }
}
