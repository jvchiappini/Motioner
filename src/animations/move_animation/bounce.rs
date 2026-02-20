// Easing math used for CPU-side interpolation and curve editing.  The runtime
// animation pipeline now performs all interpolation on the GPU; these helpers
// remain for previews and the easing curve editor.
// Easing math used for CPU-side interpolation and curve editing.  The runtime
// animation pipeline now performs all interpolation on the GPU; these helpers
// remain for previews and the easing curve editor.

#[allow(dead_code)]
pub fn to_dsl_string(bounciness: f32) -> String {
    if (bounciness - 1.0).abs() < 1e-6 {
        "bounce".to_string()
    } else {
        format!("bounce(bounciness = {:.3})", bounciness)
    }
}

