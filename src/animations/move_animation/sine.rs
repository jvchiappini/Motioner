use crate::scene::Easing;

pub fn compute_progress(local_t: f32) -> f32 {
    // easeInOutSine: 0.5 * (1 - cos(pi * t))
    0.5 * (1.0 - (std::f32::consts::PI * local_t).cos())
}

#[allow(dead_code)]
pub fn to_dsl_string() -> String {
    "sine".to_string()
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val
        .trim()
        .trim_start_matches("type")
        .trim()
        .trim_start_matches('=')
        .trim();
    if s.starts_with("sine") {
        Some(Easing::Sine)
    } else {
        None
    }
}
