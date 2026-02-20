// Damped spring easing formulas used on CPU.  The GPU currently does not
// support this curve (falls back to linear), so these helpers are primarily
// for editing/preview purposes.

#[allow(dead_code)]
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

