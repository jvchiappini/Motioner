use crate::scene::Easing;

pub fn compute_progress(local_t: f32, p1: (f32, f32), p2: (f32, f32)) -> f32 {
    solve_cubic_bezier(local_t, p1.0, p1.1, p2.0, p2.1)
}

pub fn to_dsl_string(p1: (f32, f32), p2: (f32, f32)) -> String {
    format!(
        "type = bezier(p1 = ({:.2}, {:.2}), p2 = ({:.2}, {:.2}))",
        p1.0, p1.1, p2.0, p2.1
    )
}

pub fn parse_dsl(_val: &str) -> Option<Easing> {
    // Original code didn't have bezier parsing in parse_move_block.
    None
}

/// Helper to solve cubic bezier y for a given x (time).
fn solve_cubic_bezier(x_target: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let mut t = x_target; // Initial guess
    for _ in 0..8 {
        let x = cubic_axis(t, x1, x2);
        let slope = cubic_derivative(t, x1, x2);
        if slope.abs() < 1e-6 {
            break;
        }
        t -= (x - x_target) / slope;
    }
    t = t.clamp(0.0, 1.0);
    cubic_axis(t, y1, y2)
}

fn cubic_axis(t: f32, p1: f32, p2: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
}

fn cubic_derivative(t: f32, p1: f32, p2: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}
