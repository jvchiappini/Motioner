use crate::scene::Easing;

pub fn compute_progress(local_t: f32) -> f32 {
    // easeInOutExpo (smooth fast start/stop)
    if local_t <= 0.0 {
        return 0.0;
    }
    if local_t >= 1.0 {
        return 1.0;
    }
    if local_t < 0.5 {
        0.5 * (2f32).powf(20.0 * local_t - 10.0)
    } else {
        1.0 - 0.5 * (2f32).powf(-20.0 * local_t + 10.0)
    }
}

pub fn to_dsl_string() -> String {
    "expo".to_string()
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val
        .trim()
        .trim_start_matches("type")
        .trim()
        .trim_start_matches('=')
        .trim();
    if s.starts_with("expo") {
        Some(Easing::Expo)
    } else {
        None
    }
}
