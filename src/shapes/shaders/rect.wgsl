// WGSL: rect shape fragment helper
fn shape_rect(in: VertexOutput, raw_uv: vec2<f32>) -> vec4<f32> {
    // Rect implicit from -1..1 in both axes
    // Use smoothstep for high-quality antialiasing at the boundary
    let d = max(abs(raw_uv.x), abs(raw_uv.y));
    let fw = fwidth(d);
    let alpha = 1.0 - smoothstep(1.0 - fw, 1.0, d);

    if (alpha <= 0.0) {
        return vec4<f32>(0.0);
    } 

    let norm_u = raw_uv.x * 0.5 + 0.5;
    let reveal_alpha = smoothstep(in.reveal - 0.005, in.reveal, norm_u);
    if (norm_u > in.reveal) {
        return vec4<f32>(in.color.rgb, in.color.a * alpha * (1.0 - reveal_alpha));
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
