use crate::scene::Easing;

pub fn compute_progress(local_t: f32, power: f32) -> f32 {
    1.0 - (1.0 - local_t).powf(power)
}

#[allow(dead_code)]
pub fn to_dsl_string(power: f32) -> String {
    if (power - 1.0).abs() < 1e-6 {
        "ease_out".to_string()
    } else {
        format!("ease_out(power = {:.3})", power)
    }
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val
        .trim()
        .trim_start_matches("type")
        .trim()
        .trim_start_matches('=')
        .trim();
    if s.starts_with("ease_out") {
        let mut power = 1.0f32;
        if let Some(open) = s.find('(') {
            if let Some(close) = s.rfind(')') {
                let inner = &s[open + 1..close];
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
        Some(Easing::EaseOut { power })
    } else {
        None
    }
}
