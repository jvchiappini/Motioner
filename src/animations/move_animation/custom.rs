// These helpers are only used during CPU-based sampling and when editing
// custom easing curves in the UI; the GPU compute shader does not support
// arbitrary point lists.

#[allow(dead_code)]
pub fn to_dsl_string(points: &[(f32, f32)]) -> String {
    let pts_str = points
        .iter()
        .map(|(t, v)| format!("({:.2}, {:.2})", t, v))
        .collect::<Vec<_>>()
        .join(", ");
    format!("custom(points = [{}])", pts_str)
}

