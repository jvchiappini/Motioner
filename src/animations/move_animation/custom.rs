use crate::scene::Easing;

pub fn compute_progress(local_t: f32, points: &[(f32, f32)]) -> f32 {
    if points.is_empty() {
        return local_t;
    }

    let mut p0 = (0.0, 0.0);
    let mut p1 = (1.0, 1.0);

    // Find the bounding points
    for i in 0..points.len() {
        if points[i].0 >= local_t {
            p1 = points[i];
            if i > 0 {
                p0 = points[i - 1];
            } else {
                p0 = (0.0, 0.0);
            }
            break;
        }
    }

    if local_t > points.last().map(|p| p.0).unwrap_or(0.0) {
        p0 = points.last().cloned().unwrap_or((0.0, 0.0));
        p1 = (1.0, 1.0);
    }

    let segment_duration = p1.0 - p0.0;
    if segment_duration.abs() < std::f32::EPSILON {
        p1.1
    } else {
        let segment_t = (local_t - p0.0) / segment_duration;
        p0.1 + segment_t * (p1.1 - p0.1)
    }
}

pub fn to_dsl_string(points: &[(f32, f32)]) -> String {
    let pts_str = points
        .iter()
        .map(|(t, v)| format!("({:.2}, {:.2})", t, v))
        .collect::<Vec<_>>()
        .join(", ");
    format!("custom(points = [{}])", pts_str)
}

pub fn parse_dsl(val: &str) -> Option<Easing> {
    let s = val.trim().trim_start_matches("type").trim().trim_start_matches('=').trim();
    if s.starts_with("custom") {
        let mut points = Vec::new();
        if let Some(open_bracket) = s.find('[') {
            if let Some(close_bracket) = s.rfind(']') {
                let inner = &s[open_bracket + 1..close_bracket];
                // basic comma split for (t, v)
                // Note: this is a bit fragile if there are commas inside coordinates, but good enough for now
                let mut current = inner;
                while let Some(start_paren) = current.find('(') {
                    if let Some(end_paren) = current[start_paren..].find(')') {
                        let coords = &current[start_paren + 1..start_paren + end_paren];
                        let parts: Vec<&str> = coords.split(',').collect();
                        if parts.len() == 2 {
                            let t = parts[0].trim().parse().unwrap_or(0.0);
                            let v = parts[1].trim().parse().unwrap_or(0.0);
                            points.push((t, v));
                        }
                        current = &current[start_paren + end_paren + 1..];
                    } else {
                        break;
                    }
                }
            }
        }
        return Some(Easing::Custom { points });
    }
    None
}
