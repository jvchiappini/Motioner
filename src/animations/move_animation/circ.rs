use crate::scene::Easing;

pub fn compute_progress(local_t: f32) -> f32 {
    // easeInOutCirc
    if local_t < 0.5 {
        let t = 2.0 * local_t;
        0.5 * (1.0 - (1.0 - t * t).sqrt())
    } else {
        let t = 2.0 * local_t - 2.0;
        0.5 * ((1.0 - t * t).sqrt() + 1.0)
    }
}

pub fn to_dsl_string() -> String {
    "circ".to_string()
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val.trim().trim_start_matches("type").trim().trim_start_matches('=').trim();
    if s.starts_with("circ") {
        Some(Easing::Circ)
    } else {
        None
    }
}
