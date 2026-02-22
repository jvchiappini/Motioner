// ─── Compute Shader: Keyframe Interpolation ───────────────────────────────────
//
// Reads the per-element keyframe tracks uploaded by the Rust side and writes
// the interpolated position (and other animated properties) for the current
// frame into the GpuShape output array.
//
// One workgroup invocation per element (dispatch_x = element_count).
//
// Bindings (group 0):
//   0 — uniforms          (read)   : frame index + fps + element count
//   1 — keyframe_buffer   (read)   : flat array of GpuKeyframe (all elements, all tracks)
//   2 — element_descs     (read)   : per-element descriptor (track offsets/lengths + base data)
//   3 — output_shapes     (write)  : GpuShape array consumed by the render pass

// ─── Structs ──────────────────────────────────────────────────────────────────

/// Uniform block passed once per dispatch.
struct ComputeUniforms {
    /// Current frame index (u32 cast to f32 for arithmetic convenience).
    current_frame: u32,
    /// Frames per second of the project.
    fps: u32,
    /// Total number of elements to process.
    element_count: u32,
    _pad: u32,
    /// Render target resolution in pixels (x = width, y = height).
    resolution: vec2<f32>,
}

/// A single keyframe for one property of one element.
/// `easing` encodes the curve to apply between this keyframe and the next:
///   0 = Linear
///   1 = EaseIn
///   2 = EaseOut
///   3 = EaseInOut
///   4 = Sine
///   5 = Expo
///   6 = Circ
///   (extend as more curves are added on the Rust side)
struct GpuKeyframe {
    frame: u32,
    value: f32,
    easing: u32,
    _pad: u32,
}

/// Per-element descriptor: where each property track starts in `keyframe_buffer`
/// and how many keyframes it contains, plus static base data.
struct GpuElementDesc {
    x_offset:      u32,
    x_len:         u32,
    y_offset:      u32,
    y_len:         u32,
    radius_offset: u32,
    radius_len:    u32,
    w_offset:      u32,
    w_len:         u32,
    h_offset:      u32,
    h_len:         u32,
    shape_type:    i32,
    spawn_frame:   u32,
    kill_frame:    u32,
    
    r_offset:      u32,
    g_offset:      u32,
    b_offset:      u32,
    a_offset:      u32,
    
    r_len:         u32,
    g_len:         u32,
    b_len:         u32,
    a_len:         u32,

    base_size:     vec2<f32>,
    uv0:           vec2<f32>,
    uv1:           vec2<f32>,
}

/// Output written to the render pass shape buffer.
/// Must match `GpuShape` on the Rust side exactly (repr(C)).
struct GpuShape {
    pos:        vec2<f32>,
    size:       vec2<f32>,
    color:      vec4<f32>,
    shape_type: i32,
    spawn_time: f32,
    p1:         i32,
    p2:         i32,
    uv0:        vec2<f32>,
    uv1:        vec2<f32>,
}

// ─── Bindings ─────────────────────────────────────────────────────────────────

@group(0) @binding(0) var<uniform>            cu:             ComputeUniforms;
@group(0) @binding(1) var<storage, read>      keyframes:      array<GpuKeyframe>;
@group(0) @binding(2) var<storage, read>      element_descs:  array<GpuElementDesc>;
@group(0) @binding(3) var<storage, read_write> output_shapes: array<GpuShape>;

// ─── Easing functions ─────────────────────────────────────────────────────────

fn ease_linear(t: f32) -> f32 { return t; }

fn ease_in(t: f32) -> f32 { return t * t; }

fn ease_out(t: f32) -> f32 { return 1.0 - (1.0 - t) * (1.0 - t); }

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 { return 2.0 * t * t; }
    return 1.0 - 2.0 * (1.0 - t) * (1.0 - t);
}

fn ease_sine(t: f32) -> f32 {
    return 1.0 - cos(t * 3.14159265 * 0.5);
}

fn ease_expo(t: f32) -> f32 {
    if t <= 0.0 { return 0.0; }
    return pow(2.0, 10.0 * (t - 1.0));
}

fn ease_circ(t: f32) -> f32 {
    return 1.0 - sqrt(max(0.0, 1.0 - t * t));
}

fn apply_easing(t: f32, easing: u32) -> f32 {
    switch easing {
        case 0u: { return ease_linear(t); }
        case 1u: { return ease_in(t); }
        case 2u: { return ease_out(t); }
        case 3u: { return ease_in_out(t); }
        case 4u: { return ease_sine(t); }
        case 5u: { return ease_expo(t); }
        case 6u: { return ease_circ(t); }
        default: { return ease_linear(t); }
    }
}

// ─── Track sampler ────────────────────────────────────────────────────────────

/// Sample a flat keyframe track at `current_frame`.
/// Returns the interpolated value between the two surrounding keyframes.
/// Falls back to the last keyframe's value if past the end, or the first if before start.
fn sample_track(offset: u32, len: u32, current_frame: u32) -> f32 {
    if len == 0u { return 0.0; }

    // Find the last keyframe <= current_frame.
    var prev_idx: i32 = -1;
    for (var i: u32 = 0u; i < len; i = i + 1u) {
        let kf = keyframes[offset + i];
        if kf.frame <= current_frame {
            prev_idx = i32(i);
        } else {
            break;
        }
    }

    // Before the first keyframe: return first value.
    if prev_idx < 0 {
        return keyframes[offset].value;
    }

    let prev = keyframes[offset + u32(prev_idx)];

    // Exactly on or past the last keyframe: return last value.
    let next_i = u32(prev_idx) + 1u;
    if next_i >= len {
        return prev.value;
    }

    let next = keyframes[offset + next_i];

    // Interpolate between prev and next using the easing stored on prev.
    let frame_range = f32(next.frame) - f32(prev.frame);
    if frame_range <= 0.0 { return next.value; }

    let local_t = f32(current_frame - prev.frame) / frame_range;
    let eased_t = apply_easing(clamp(local_t, 0.0, 1.0), prev.easing);
    return mix(prev.value, next.value, eased_t);
}

// ─── Main dispatch ────────────────────────────────────────────────────────────

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= cu.element_count { return; }

    let desc = element_descs[idx];

    // Skip elements that are not yet spawned or already killed.
    if cu.current_frame < desc.spawn_frame { return; }
    if desc.kill_frame != 0xFFFFFFFFu && cu.current_frame >= desc.kill_frame { return; }

    // Sample animated tracks.
    var x      = sample_track(desc.x_offset,      desc.x_len,      cu.current_frame);
    var y      = sample_track(desc.y_offset,       desc.y_len,      cu.current_frame);
    let radius = sample_track(desc.radius_offset,  desc.radius_len, cu.current_frame);
    let w      = sample_track(desc.w_offset,       desc.w_len,      cu.current_frame);
    let h      = sample_track(desc.h_offset,       desc.h_len,      cu.current_frame);
    
    let r      = sample_track(desc.r_offset,       desc.r_len,      cu.current_frame);
    let g      = sample_track(desc.g_offset,       desc.g_len,      cu.current_frame);
    let b      = sample_track(desc.b_offset,       desc.b_len,      cu.current_frame);
    let a      = sample_track(desc.a_offset,       desc.a_len,      cu.current_frame);

    // Derive size from shape type.
    var size = vec2<f32>(0.0, 0.0);
    if desc.shape_type == 0 {
        size = vec2<f32>(radius * cu.resolution.x, radius * cu.resolution.x);
    } else {
        size = vec2<f32>(w * cu.resolution.x * 0.5, h * cu.resolution.y * 0.5);
    }
    
    // No move_commands loop – all positional animation has been baked into
    // the x/y keyframe tracks at parse time, so the sampler above already
    // returns the correct interpolated position for the current frame.

    let spawn_time = f32(desc.spawn_frame) / f32(cu.fps);

    // Write output shape.
    var out: GpuShape;
    out.pos        = vec2<f32>(x * cu.resolution.x, y * cu.resolution.y);
    out.size       = size;
    out.color      = vec4<f32>(r, g, b, a);
    out.shape_type = desc.shape_type;
    out.spawn_time = spawn_time;
    out.p1         = 0;
    out.p2         = 0;
    out.uv0        = desc.uv0;
    out.uv1        = desc.uv1;

    output_shapes[idx] = out;
}
