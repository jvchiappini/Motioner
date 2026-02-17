use crate::scene::{BezierPoint, Easing};

pub fn compute_progress(local_t: f32, points: &[BezierPoint]) -> f32 {
    if points.is_empty() {
        return local_t;
    }
    if points.len() == 1 {
        return points[0].pos.1;
    }

    // Find the bounding points
    let mut p_start = &points[0];
    let mut p_end = &points[1];
    let mut found = false;

    for i in 0..points.len() - 1 {
        if local_t >= points[i].pos.0 && local_t <= points[i + 1].pos.0 {
            p_start = &points[i];
            p_end = &points[i + 1];
            found = true;
            break;
        }
    }

    if !found {
        if local_t < points[0].pos.0 {
            return points[0].pos.1;
        } else {
            return points.last().unwrap().pos.1;
        }
    }

    let x0 = p_start.pos.0;
    let y0 = p_start.pos.1;
    let x1 = x0 + p_start.handle_right.0;
    let y1 = y0 + p_start.handle_right.1;
    let x2 = p_end.pos.0 + p_end.handle_left.0;
    let y2 = p_end.pos.1 + p_end.handle_left.1;
    let x3 = p_end.pos.0;
    let y3 = p_end.pos.1;

    solve_cubic_bezier(local_t, x0, y0, x1, y1, x2, y2, x3, y3)
}

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

#[allow(dead_code)]
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

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val.trim();
    if s.starts_with("custom_bezier") {
        let mut points = Vec::new();
        if let Some(open_bracket) = s.find('[') {
            if let Some(close_bracket) = s.rfind(']') {
                let inner = &s[open_bracket + 1..close_bracket];
                // format: ((x,y), (hlx,hly), (hrx,hry))
                let mut current = inner;
                while let Some(start_tuple) = current.find("((") {
                    if let Some(end_tuple) = current[start_tuple..].find("))") {
                        let inner_tuple = &current[start_tuple + 1..start_tuple + end_tuple + 1];
                        // inner_tuple: (x,y), (hlx,hly), (hrx,hry)
                        let parts: Vec<&str> = inner_tuple.split("),").collect();
                        if parts.len() == 3 {
                            let parse_coord = |c: &str| -> (f32, f32) {
                                let c = c.trim().trim_matches('(').trim_matches(')');
                                let p: Vec<&str> = c.split(',').collect();
                                if p.len() == 2 {
                                    (
                                        p[0].trim().parse().unwrap_or(0.0),
                                        p[1].trim().parse().unwrap_or(0.0),
                                    )
                                } else {
                                    (0.0, 0.0)
                                }
                            };
                            let pos = parse_coord(parts[0]);
                            let hl = parse_coord(parts[1]);
                            let hr = parse_coord(parts[2]);
                            points.push(BezierPoint {
                                pos,
                                handle_left: hl,
                                handle_right: hr,
                            });
                        }
                        current = &current[start_tuple + end_tuple + 2..];
                    } else {
                        break;
                    }
                }
            }
        }
        if !points.is_empty() {
            return Some(Easing::CustomBezier { points });
        }
    }
    None
}
