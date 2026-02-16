use crate::scene::Easing;

pub fn compute_progress(local_t: f32, points: &[(f32, f32)]) -> f32 {
    if points.is_empty() {
        return local_t;
    }

    let mut p0 = (0.0, 0.0);
    let mut p1 = (1.0, 1.0);

    // Find the bounding points
    for i in 0..points.len() {
        if points[i].0 >= local_t {
            p1 = points[i];
            if i > 0 {
                p0 = points[i - 1];
            } else {
                p0 = (0.0, 0.0);
            }
            break;
        }
    }

    if local_t > points.last().map(|p| p.0).unwrap_or(0.0) {
        p0 = points.last().cloned().unwrap_or((0.0, 0.0));
        p1 = (1.0, 1.0);
    }

    let segment_duration = p1.0 - p0.0;
    if segment_duration.abs() < std::f32::EPSILON {
        p1.1
    } else {
        let segment_t = (local_t - p0.0) / segment_duration;
        p0.1 + segment_t * (p1.1 - p0.1)
    }
}

pub fn to_dsl_string(points: &[(f32, f32)]) -> String {
    let pts_str = points
        .iter()
        .map(|(t, v)| format!("({:.2}, {:.2})", t, v))
        .collect::<Vec<_>>()
        .join(", ");
    format!("type = custom(points = [{}])", pts_str)
}

pub fn parse_dsl(_val: &str) -> Option<Easing> {
    // Original code didn't have custom parsing in parse_move_block.
    None
}
