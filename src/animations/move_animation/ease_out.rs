// CPU easing helper for out-curves.  GPU side currently only supports a
// small set of curves; parameterized versions fall back to linear.

#[allow(dead_code)]
pub fn to_dsl_string(power: f32) -> String {
    if (power - 1.0).abs() < 1e-6 {
        "ease_out".to_string()
    } else {
        format!("ease_out(power = {:.3})", power)
    }
}

