// CPU easing helper.  GPU compute shader will map all `EaseIn` curves to a
// fixed quadratic/easing function; power parameter is ignored at the moment.

pub fn to_dsl_string(power: f32) -> String {
    if (power - 1.0).abs() < 1e-6 {
        "ease_in".to_string()
    } else {
        format!("ease_in(power = {:.3})", power)
    }
}

