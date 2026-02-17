use crate::scene::Easing;

fn ease_out_bounce(mut t: f32) -> f32 {
    // standard penner piecewise bounce out
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        t -= 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        t -= 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        t -= 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}

pub fn compute_progress(local_t: f32, bounciness: f32) -> f32 {
    // blend between linear (no bounce) and bounce curve depending on bounciness
    let t = local_t.clamp(0.0, 1.0);
    let base = ease_out_bounce(t);
    let alpha = bounciness.clamp(0.0, 3.0) / 3.0; // 0..1
    (1.0 - alpha) * t + alpha * base
}

pub fn to_dsl_string(bounciness: f32) -> String {
    if (bounciness - 1.0).abs() < 1e-6 {
        "bounce".to_string()
    } else {
        format!("bounce(bounciness = {:.3})", bounciness)
    }
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val.trim().trim_start_matches("type").trim().trim_start_matches('=').trim();
    if s.starts_with("bounce") {
        let mut bounciness = 1.0f32;
        if let Some(open) = s.find('(') {
            if let Some(close) = s.rfind(')') {
                let inner = &s[open + 1..close];
                for part in inner.split(',') {
                    let p = part.trim();
                    if p.starts_with("bounciness") && p.contains('=') {
                        if let Some(eq) = p.find('=') {
                            if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                bounciness = v;
                            }
                        }
                    }
                }
            }
        }
        return Some(Easing::Bounce { bounciness });
    }
    None
}
