// WGSL: circle shape fragment helper
// Implement the same visual behaviour as the previous inlined circle SDF.
fn shape_circle(in: VertexOutput, raw_uv: vec2<f32>) -> vec4<f32> {
    // Circle centered at origin with radius == 1.0 (quad extent)
    // Use smoothstep for high-quality antialiasing at the boundary
    let dist = length(raw_uv);
    let edge_width = fwidth(dist); 
    let alpha = 1.0 - smoothstep(1.0 - edge_width, 1.0, dist);
    
    if (alpha <= 0.0) {
        return vec4<f32>(0.0);
    }

    // Sweep reveal (0 to 1)
    let angle = atan2(raw_uv.y, raw_uv.x); // -PI to PI
    let norm_angle = (angle / 3.14159265) * 0.5 + 0.5;
    
    // Antialias the reveal edge too
    let reveal_alpha = smoothstep(in.reveal - 0.005, in.reveal, norm_angle);
    if (norm_angle > in.reveal) {
         // for a sharp cutoff with AA, we'd need to blend, but let's just 
         // use a simple smoothstep for now
         return vec4<f32>(in.color.rgb, in.color.a * alpha * (1.0 - reveal_alpha));
    }
    
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
