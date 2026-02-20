// Elastic easing math used when drawing/editing; GPU compute doesn't support
// amplitude/period yet and will fallback to linear.

#[allow(dead_code)]
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

