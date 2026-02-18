struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) shape_type: i32,
    @location(3) @interpolate(flat) size: vec2<f32>,
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
}

@group(0) @binding(0) var<storage, read> shapes: array<Shape>;
@group(0) @binding(1) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let s = shapes[in.instance_index];
    var out: VertexOutput;

    // Si el objeto aún no "nace", lo mandamos fuera de pantalla
    // if (uniforms.time < s.spawn_time) {
    //    out.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    //    return out;
    // }

    // Coordenadas del Quad (-1..1)
    var quad_pos = vec2<f32>(0.0, 0.0);
    if (in.vertex_index == 0u) { quad_pos = vec2<f32>(-1.0, -1.0); }
    else if (in.vertex_index == 1u) { quad_pos = vec2<f32>(1.0, -1.0); }
    else if (in.vertex_index == 2u) { quad_pos = vec2<f32>(-1.0, 1.0); }
    else if (in.vertex_index == 3u) { quad_pos = vec2<f32>(-1.0, 1.0); }
    else if (in.vertex_index == 4u) { quad_pos = vec2<f32>(1.0, -1.0); }
    else { quad_pos = vec2<f32>(1.0, 1.0); }

    out.local_uv = quad_pos; // Para calcular el SDF en el fragment shader
    out.color = s.color;
    out.shape_type = s.shape_type;
    out.size = s.size;

    // Transformación: De coordenadas normalizadas de escena (0..1) a coordenadas de clip (-1..1)
    // El tamaño del quad depende del tamaño del objeto
    let world_pos_px = s.pos + (quad_pos * s.size);
    let world_pos_norm = world_pos_px / uniforms.resolution;
    
    // Map normalized (0..1) to paper area, then to screen
    let screen_pos = mix(uniforms.paper_rect.xy, uniforms.paper_rect.zw, world_pos_norm);
    
    // Convert screen pixel position to NDC (-1..1)
    let ndc_pos = (screen_pos - uniforms.viewport_rect.xy) / (uniforms.viewport_rect.zw - uniforms.viewport_rect.xy) * 2.0 - 1.0;
    
    // Invertimos Y porque WGPU e imagen van al revés
    out.position = vec4<f32>(ndc_pos.x, -ndc_pos.y, 0.0, 1.0);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var alpha = 1.0;
    
    if (in.shape_type == 0) { // Circle
        let d = length(in.local_uv) - 1.0;
        // Anti-aliasing suave
        let aa = fwidth(d);
        alpha = 1.0 - smoothstep(-aa, aa, d);
    } else if (in.shape_type == 1) { // Rect
        // El quad ya tiene forma de rectángulo, así que alpha es casi siempre 1.0
        // pero podemos redondear esquinas aquí si quisiéramos
        alpha = 1.0;
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
