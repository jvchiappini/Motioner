// Easing curve math for CPU/preview.  GPU currently ignores the `power`
// parameter and will fall back to linear in many cases.
// Easing curve math for CPU/preview.  GPU currently ignores the `power`
// parameter and will fall back to linear in many cases.
#[allow(dead_code)]
pub fn to_dsl_string(power: f32) -> String {
    if (power - 1.0).abs() < 1e-6 {
        "ease_in_out".to_string()
    } else {
        format!("ease_in_out(power = {:.3})", power)
    }
}

