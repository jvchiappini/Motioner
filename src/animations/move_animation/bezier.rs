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

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val.trim_start_matches("type").trim_start_matches('=').trim();
    if s.starts_with("bezier") {
        let mut p1 = (0.0, 0.0);
        let mut p2 = (1.0, 1.0);
        if let Some(open) = s.find('(') {
            if let Some(close) = s.rfind(')') {
                let inner = &s[open + 1..close];
                // basic comma split for p1=(...), p2=(...)
                let parts: Vec<&str> = inner.split("p2").collect();
                if parts.len() == 2 {
                    // Part 1 should contain p1 = (...)
                    if let Some(p1_eq) = parts[0].find('=') {
                        let p1_val = parts[0][p1_eq+1..].trim().trim_matches(',');
                        if p1_val.starts_with('(') && p1_val.contains(')') {
                            let p1_inner = &p1_val[1..p1_val.find(')').unwrap()];
                            let coords: Vec<&str> = p1_inner.split(',').collect();
                            if coords.len() == 2 {
                                p1.0 = coords[0].trim().parse().unwrap_or(0.0);
                                p1.1 = coords[1].trim().parse().unwrap_or(0.0);
                            }
                        }
                    }
                    // Part 2 should contain = (...)
                    if let Some(p2_eq) = parts[1].find('=') {
                        let p2_val = parts[1][p2_eq+1..].trim().trim_matches(',');
                        if p2_val.starts_with('(') && p2_val.contains(')') {
                            let p2_inner = &p2_val[1..p2_val.find(')').unwrap()];
                            let coords: Vec<&str> = p2_inner.split(',').collect();
                            if coords.len() == 2 {
                                p2.0 = coords[0].trim().parse().unwrap_or(1.0);
                                p2.1 = coords[1].trim().parse().unwrap_or(1.0);
                            }
                        }
                    }
                }
            }
        }
        return Some(Easing::Bezier { p1, p2 });
    }
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
