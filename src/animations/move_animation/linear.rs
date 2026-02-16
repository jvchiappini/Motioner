use crate::scene::Easing;

pub fn compute_progress(local_t: f32) -> f32 {
    local_t
}

pub fn to_dsl_string() -> String {
    "type = linear".to_string()
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    if val.contains("linear") {
        Some(Easing::Linear)
    } else {
        None
    }
}
