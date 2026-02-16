use crate::scene::Easing;

pub fn compute_progress(local_t: f32, power: f32) -> f32 {
    if (power - 1.0).abs() < std::f32::EPSILON {
        local_t
    } else if local_t < 0.5 {
        0.5 * (2.0 * local_t).powf(power)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - local_t)).powf(power)
    }
}

pub fn to_dsl_string(power: f32) -> String {
    if (power - 1.0).abs() < 1e-6 {
        "type = ease_in_out".to_string()
    } else {
        format!("type = ease_in_out(power = {:.3})", power)
    }
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    if val.starts_with("ease_in_out") {
        let mut power = 1.0f32;
        if let Some(open) = val.find('(') {
            if let Some(close) = val.rfind(')') {
                let inner = &val[open + 1..close];
                for part in inner.split(',') {
                    let p = part.trim();
                    if p.starts_with("power") && p.contains('=') {
                        if let Some(eq) = p.find('=') {
                            if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                power = v;
                            }
                        }
                    }
                }
            }
        }
        Some(Easing::EaseInOut { power })
    } else {
        None
    }
}
