use crate::scene::Easing;

/// Damped harmonic oscillator based easing. Defaults chosen to give a pleasant
/// underdamped overshoot for normalized time in [0,1].
pub fn compute_progress(local_t: f32, damping: f32, stiffness: f32, mass: f32) -> f32 {
    if local_t <= 0.0 {
        return 0.0;
    }
    if local_t >= 1.0 {
        return 1.0;
    }

    // natural frequency
    let omega0 = (stiffness / mass).max(0.0).sqrt();
    let zeta = if omega0.abs() < std::f32::EPSILON {
        1.0
    } else {
        damping / (2.0 * (stiffness * mass).sqrt())
    };
    let t = local_t;

    let value = if zeta < 1.0 {
        // underdamped
        let omega_d = omega0 * (1.0 - zeta * zeta).sqrt();
        let exp_term = (-zeta * omega0 * t).exp();
        let cos = (omega_d * t).cos();
        let sin = (omega_d * t).sin();
        // normalized response that starts at 0 and approaches 1 with damped oscillation
        let resp = 1.0 - exp_term * (cos + (zeta / (1.0 - zeta * zeta).sqrt()) * sin);
        resp
    } else if (zeta - 1.0).abs() < 1e-4 {
        // critically damped — simple decay
        1.0 - (1.0 + omega0 * t) * (-omega0 * t).exp()
    } else {
        // overdamped — approximate with a fast decaying exponential toward 1
        1.0 - (-omega0 * t).exp()
    };

    value.clamp(0.0, 1.0)
}

pub fn to_dsl_string(damping: f32, stiffness: f32, mass: f32) -> String {
    if (damping - 0.7).abs() < 1e-6 && (stiffness - 120.0).abs() < 1e-3 && (mass - 1.0).abs() < 1e-6
    {
        "spring".to_string()
    } else {
        format!(
            "spring(damping = {:.3}, stiffness = {:.3}, mass = {:.3})",
            damping, stiffness, mass
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
    if s.starts_with("spring") {
        let mut damping = 0.7f32;
        let mut stiffness = 120.0f32;
        let mut mass = 1.0f32;
        if let Some(open) = s.find('(') {
            if let Some(close) = s.rfind(')') {
                let inner = &s[open + 1..close];
                for part in inner.split(',') {
                    let p = part.trim();
                    if p.starts_with("damping") && p.contains('=') {
                        if let Some(eq) = p.find('=') {
                            if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                damping = v;
                            }
                        }
                    }
                    if p.starts_with("stiffness") && p.contains('=') {
                        if let Some(eq) = p.find('=') {
                            if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                stiffness = v;
                            }
                        }
                    }
                    if p.starts_with("mass") && p.contains('=') {
                        if let Some(eq) = p.find('=') {
                            if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                mass = v;
                            }
                        }
                    }
                }
            }
        }
        return Some(Easing::Spring {
            damping,
            stiffness,
            mass,
        });
    }
    None
}
