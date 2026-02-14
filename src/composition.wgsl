struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) screen_pos: vec2<f32>,
};

struct Shape {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    shape_type: i32,
    spawn_time: f32,
    p1: i32,
    p2: i32,
}

struct Uniforms {
    resolution: vec2<f32>,
    preview_res: vec2<f32>,
    paper_rect: vec4<f32>,
    viewport_rect: vec4<f32>,
    count: f32,
    mag_x: f32,
    mag_y: f32,
    mag_active: f32,
    time: f32,
    p1: f32,
    p2: f32,
    p3: f32,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    
    var pos = vec2<f32>(0.0, 0.0);
    var uv = vec2<f32>(0.0, 0.0);
    
    if (vertex_index == 0u) { 
        pos = vec2<f32>(-1.0, -1.0); uv = vec2<f32>(0.0, 1.0); 
    } else if (vertex_index == 1u) { 
        pos = vec2<f32>( 1.0, -1.0); uv = vec2<f32>(1.0, 1.0); 
    } else if (vertex_index == 2u) { 
        pos = vec2<f32>(-1.0,  1.0); uv = vec2<f32>(0.0, 0.0); 
    } else if (vertex_index == 3u) { 
        pos = vec2<f32>(-1.0,  1.0); uv = vec2<f32>(0.0, 0.0); 
    } else if (vertex_index == 4u) { 
        pos = vec2<f32>( 1.0, -1.0); uv = vec2<f32>(1.0, 1.0); 
    } else { 
        pos = vec2<f32>( 1.0,  1.0); uv = vec2<f32>(1.0, 0.0); 
    }

    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    // Map UVs (0..1) to viewport coordinates
    out.screen_pos = mix(uniforms.viewport_rect.xy, uniforms.viewport_rect.zw, uv);
    return out;
}

@group(0) @binding(0) var<storage, read> shapes: array<Shape>;
@group(0) @binding(1) var<uniform> uniforms: Uniforms;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p_min = uniforms.paper_rect.xy;
    let p_max = uniforms.paper_rect.zw;
    
    // Magnifier Logic
    var final_uv_pos = in.screen_pos;
    var is_magnifier = false;
    
    if (uniforms.mag_active > 0.5) {
        let mag_center = vec2<f32>(uniforms.mag_x, uniforms.mag_y);
        let dist_to_mag = distance(in.screen_pos, mag_center);
        let mag_radius = 80.0; 
        
        if (dist_to_mag < mag_radius) {
            is_magnifier = true;
            let rel = (in.screen_pos - mag_center) / 8.0; 
            final_uv_pos = mag_center + rel;
        } else if (dist_to_mag < mag_radius + 2.0) {
            return vec4<f32>(1.0, 0.5, 0.0, 1.0); 
        }
    }

    if (final_uv_pos.x < p_min.x || final_uv_pos.x > p_max.x || 
        final_uv_pos.y < p_min.y || final_uv_pos.y > p_max.y) {
        discard;
    }

    let paper_uv = (final_uv_pos - p_min) / (p_max - p_min);
    let snapped_uv = (floor(paper_uv * uniforms.preview_res) + 0.5) / uniforms.preview_res;
    let pixel_pos = snapped_uv * uniforms.resolution;

    var color = vec4<f32>(1.0, 1.0, 1.0, 1.0); 

    for (var i = 0u; i < u32(uniforms.count); i++) {
        let s = shapes[i];
        if (uniforms.time < s.spawn_time) {
            continue;
        }
        let shape_pos_pixel = s.pos * uniforms.resolution;
        var alpha = 0.0;
        let aa = length(uniforms.resolution / uniforms.preview_res) * 0.5;

        if (s.shape_type == 0) {
            let d = distance(pixel_pos, shape_pos_pixel) - (s.size.x * uniforms.resolution.x);
            alpha = 1.0 - smoothstep(-aa, aa, d);
        } else if (s.shape_type == 1) {
            let d_vec = abs(pixel_pos - shape_pos_pixel) - (s.size * uniforms.resolution);
            let d = length(max(d_vec, vec2<f32>(0.0))) + min(max(d_vec.x, d_vec.y), 0.0);
            alpha = 1.0 - smoothstep(-aa, aa, d);
        }

        let src = vec4<f32>(s.color.rgb, s.color.a * alpha);
        color = vec4<f32>(mix(color.rgb, src.rgb, src.a), color.a);
    }

    if (is_magnifier) {
        let grid_uv = paper_uv * uniforms.preview_res;
        let grid = step(0.90, fract(grid_uv.x)) + step(0.90, fract(grid_uv.y));
        if (grid > 0.0) {
            color = mix(color, vec4<f32>(0.5, 0.5, 0.5, 1.0), 0.2);
        }
    }

    return color;
}
