use crate::scene::BezierPoint;
// CPU/curve-editor easing; the GPU currently only implements a handful of
// pre‑defined curves and ignores arbitrary custom bezier points.

#[allow(clippy::too_many_arguments)]
fn solve_cubic_bezier(
    x_target: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
) -> f32 {
    let mut t = (x_target - x0) / (x3 - x0); // Initial guess
    t = t.clamp(0.0, 1.0);

    for _ in 0..8 {
        let x = cubic_bezier_1d(t, x0, x1, x2, x3);
        let slope = cubic_bezier_derivative_1d(t, x0, x1, x2, x3);
        if slope.abs() < 1e-6 {
            break;
        }
        t -= (x - x_target) / slope;
        t = t.clamp(0.0, 1.0);
    }

    cubic_bezier_1d(t, y0, y1, y2, y3)
}

fn cubic_bezier_1d(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
}

fn cubic_bezier_derivative_1d(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * (p1 - p0) + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (p3 - p2)
}

pub fn to_dsl_string(points: &[BezierPoint]) -> String {
    let pts_str = points
        .iter()
        .map(|p| {
            format!(
                "(({:.2}, {:.2}), ({:.2}, {:.2}), ({:.2}, {:.2}))",
                p.pos.0,
                p.pos.1,
                p.handle_left.0,
                p.handle_left.1,
                p.handle_right.0,
                p.handle_right.1
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("custom_bezier(points = [{}])", pts_str)
}

