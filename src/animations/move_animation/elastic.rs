use crate::scene::Easing;

pub fn compute_progress(local_t: f32, mut amplitude: f32, mut period: f32) -> f32 {
    // easeInOutElastic (adapted from common Penner implementation)
    if local_t <= 0.0 {
        return 0.0;
    }
    if local_t >= 1.0 {
        return 1.0;
    }

    let pi2 = std::f32::consts::PI * 2.0;
    let t = local_t;
    if amplitude.abs() < std::f32::EPSILON {
        amplitude = 1.0;
    }
    if period.abs() < std::f32::EPSILON {
        period = 0.3;
    }

    let s = if amplitude < 1.0 {
        period / 4.0
    } else {
        let asinv = (1.0 / amplitude).clamp(-1.0, 1.0).asin();
        period / pi2 * asinv
    };

    if t < 0.5 {
        let t2 = 2.0 * t - 1.0;
        -0.5 * (amplitude * (2f32).powf(10.0 * t2) * ((t2 - s) * pi2 / period).sin())
    } else {
        let t2 = 2.0 * t - 1.0;
        0.5 * (amplitude * (2f32).powf(-10.0 * t2) * ((t2 - s) * pi2 / period).sin()) + 1.0
    }
}

pub fn to_dsl_string(amplitude: f32, period: f32) -> String {
    if (amplitude - 1.0).abs() < 1e-6 && (period - 0.3).abs() < 1e-6 {
        "elastic".to_string()
    } else {
        format!(
            "elastic(amplitude = {:.3}, period = {:.3})",
            amplitude, period
        )
    }
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val
        .trim()
        .trim_start_matches("type")
        .trim()
        .trim_start_matches('=')
        .trim();
    if s.starts_with("elastic") {
        let mut amplitude = 1.0f32;
        let mut period = 0.3f32;
        if let Some(open) = s.find('(') {
            if let Some(close) = s.rfind(')') {
                let inner = &s[open + 1..close];
                for part in inner.split(',') {
                    let p = part.trim();
                    if p.starts_with("amplitude") && p.contains('=') {
                        if let Some(eq) = p.find('=') {
                            if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                amplitude = v;
                            }
                        }
                    }
                    if p.starts_with("period") && p.contains('=') {
                        if let Some(eq) = p.find('=') {
                            if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                period = v;
                            }
                        }
                    }
                }
            }
        }
        return Some(Easing::Elastic { amplitude, period });
    }
    None
}
