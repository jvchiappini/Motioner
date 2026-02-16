use crate::scene::Easing;

/// Concrete implementation for a linear "move" animation.
#[derive(Clone, Debug)]
pub struct MoveAnimation {
    pub to_x: f32,
    pub to_y: f32,
    pub start: f32,
    pub end: f32,
    pub easing: Easing,
}

impl MoveAnimation {
    /// Create from scene::Animation::Move (returns None for other variants).
    pub fn from_scene(anim: &crate::scene::Animation) -> Option<Self> {
        if let crate::scene::Animation::Move {
            to_x,
            to_y,
            start,
            end,
            easing,
        } = anim
        {
            Some(MoveAnimation {
                to_x: *to_x,
                to_y: *to_y,
                start: *start,
                end: *end,
                easing: easing.clone(),
            })
        } else {
            None
        }
    }

    /// Sample the animated position given the element's base (x,y) at `project_time`.
    /// Behaviour matches the previous inline logic: before start -> base, after end -> to, between -> interpolated.
    pub fn sample_position(&self, base_x: f32, base_y: f32, project_time: f32) -> (f32, f32) {
        if project_time <= self.start {
            return (base_x, base_y);
        }
        if project_time >= self.end {
            return (self.to_x, self.to_y);
        }

        // safe since start < end (if equal, treat as instant end)
        let denom = (self.end - self.start).abs();
        let local_t = if denom < std::f32::EPSILON {
            1.0
        } else {
            (project_time - self.start) / denom
        };

        match self.easing {
            // Default linear easing
            Easing::Linear => {
                let ix = base_x + local_t * (self.to_x - base_x);
                let iy = base_y + local_t * (self.to_y - base_y);
                (ix, iy)
            }

            // Symmetric ease-in/out (power controls curvature)
            Easing::EaseInOut { power } => {
                let progress = if (power - 1.0).abs() < std::f32::EPSILON {
                    local_t
                } else if local_t < 0.5 {
                    0.5 * (2.0 * local_t).powf(power)
                } else {
                    1.0 - 0.5 * (2.0 * (1.0 - local_t)).powf(power)
                };

                let ix = base_x + progress * (self.to_x - base_x);
                let iy = base_y + progress * (self.to_y - base_y);
                (ix, iy)
            }

            // Ease-in: progress = t^power (accelerating)
            Easing::EaseIn { power } => {
                let progress = local_t.powf(power);
                let ix = base_x + progress * (self.to_x - base_x);
                let iy = base_y + progress * (self.to_y - base_y);
                (ix, iy)
            }

            // Ease-out: progress = 1 - (1-t)^power (decelerating)
            Easing::EaseOut { power } => {
                let progress = 1.0 - (1.0 - local_t).powf(power);
                let ix = base_x + progress * (self.to_x - base_x);
                let iy = base_y + progress * (self.to_y - base_y);
                (ix, iy)
            }

            // Custom easing: piecewise linear interpolation between points
            Easing::Custom { ref points } => {
                let progress = if points.is_empty() {
                    local_t
                } else {
                    // Find the segment local_t falls into
                    // We assume points are sorted by t (x-axis)
                    // If not sorted, we might get weird results, but for UI we ensure specific order.
                    // We also assume (0,0) and (1,1) are conceptually there if not in list.

                    // Actually, let's enforce a rule: Custom easing must have at least (0,0) and (1,1).
                    // If user provides points, we interpolate between them.

                    // Let's perform a simple linear search or binary search
                    let mut p0 = (0.0, 0.0);
                    let mut p1 = (1.0, 1.0);

                    // Find the bounding points
                    for i in 0..points.len() {
                        if points[i].0 >= local_t {
                            p1 = points[i];
                            if i > 0 {
                                p0 = points[i - 1];
                            } else {
                                // If the first point is after local_t, we interpolate from (0,0) to p1
                                p0 = (0.0, 0.0);
                            }
                            break;
                        }
                    }

                    // If we went through all points and didn't find one >= local_t,
                    // then local_t is after the last point. Interpolate from last point to (1,1).
                    if local_t > points.last().map(|p| p.0).unwrap_or(0.0) {
                        p0 = points.last().cloned().unwrap_or((0.0, 0.0));
                        p1 = (1.0, 1.0);
                    }

                    // Interpolate between p0 and p1
                    let segment_duration = p1.0 - p0.0;
                    if segment_duration.abs() < std::f32::EPSILON {
                        p1.1
                    } else {
                        let segment_t = (local_t - p0.0) / segment_duration;
                        // Linear interpolation
                        p0.1 + segment_t * (p1.1 - p0.1)
                        // To support bezier later, we would use control points here
                    }
                };

                let ix = base_x + progress * (self.to_x - base_x);
                let iy = base_y + progress * (self.to_y - base_y);
                (ix, iy)
            }

            // Cubic Bezier easing (CSS-like)
            Easing::Bezier { p1, p2 } => {
                let t = local_t;
                // Cubic Bezier formula for 1D (progress vs t):
                // We need to solve for x(T) = t to find T, then y(T) is progress.
                // But generally easing curves are defined such that x is time.
                // Standard cubic-bezier(x1, y1, x2, y2) uses Newton's method to find T given x.

                // For simplicity, let's implement a quick solve.
                let progress = solve_cubic_bezier(t, p1.0, p1.1, p2.0, p2.1);

                let ix = base_x + progress * (self.to_x - base_x);
                let iy = base_y + progress * (self.to_y - base_y);
                (ix, iy)
            }
        }
    }

    pub fn to_dsl_snippet(&self, element_name: &str, indent: &str) -> String {
        let ease_str = match &self.easing {
            Easing::Linear => "linear".to_string(),
            Easing::EaseIn { power } => format!("ease-in({})", power),
            Easing::EaseOut { power } => format!("ease-out({})", power),
            Easing::EaseInOut { power } => format!("ease-in-out({})", power),
            Easing::Bezier { p1, p2 } => format!(
                "bezier(({:.2}, {:.2}), ({:.2}, {:.2}))",
                p1.0, p1.1, p2.0, p2.1
            ),
            Easing::Custom { points } => {
                let pts_str = points
                    .iter()
                    .map(|(t, v)| format!("({:.2}, {:.2})", t, v))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("custom([{}])", pts_str)
            }
        };

        format!(
            "{}{}.animate_to({:.2}, {:.2}).during({:.2}, {:.2}).ease({});\n",
            indent, element_name, self.to_x, self.to_y, self.start, self.end, ease_str
        )
    }
}

/// Helper to solve cubic bezier y for a given x (time).
/// x(t) = (1-t)^3 * 0 + 3(1-t)^2 * t * x1 + 3(1-t) * t^2 * x2 + t^3 * 1
/// y(t) = (1-t)^3 * 0 + 3(1-t)^2 * t * y1 + 3(1-t) * t^2 * y2 + t^3 * 1
fn solve_cubic_bezier(x_target: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let mut t = x_target; // Initial guess
                          // Newton's method
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_at_end_returns_target() {
        let ma = MoveAnimation {
            to_x: 0.7,
            to_y: 0.5,
            start: 0.0,
            end: 5.0,
            easing: Easing::Linear,
        };

        let (x, y) = ma.sample_position(0.5, 0.5, 5.0);
        assert!((x - 0.7).abs() < 1e-6);
        assert!((y - 0.5).abs() < 1e-6);

        // times after end stay at target
        let (x2, y2) = ma.sample_position(0.5, 0.5, 6.0);
        assert!((x2 - 0.7).abs() < 1e-6);
        assert!((y2 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn positions_by_frame_samples_edges() {
        let ma = MoveAnimation {
            to_x: 1.0,
            to_y: 0.0,
            start: 0.0,
            end: 1.0,
            easing: Easing::Linear,
        };

        let frames = ma.positions_by_frame(0.0, 1.0, 2);
        // fps=2 -> step=0.5 -> expect times [0.0, 0.5, 1.0]
        let times: Vec<f32> = frames.iter().map(|(t, _, _)| *t).collect();
        assert_eq!(times, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn ease_in_out_power_changes_profile() {
        let base = MoveAnimation {
            to_x: 0.8,
            to_y: 0.2,
            start: 0.0,
            end: 4.0,
            easing: Easing::Linear,
        };
        let eio_pow1 = MoveAnimation {
            easing: Easing::EaseInOut { power: 1.0 },
            ..base.clone()
        };
        let eio_pow2 = MoveAnimation {
            easing: Easing::EaseInOut { power: 2.0 },
            ..base.clone()
        };

        // power == 1.0 should match linear
        let (xl, yl) = base.sample_position(0.2, 0.2, 1.5);
        let (x1, y1) = eio_pow1.sample_position(0.2, 0.2, 1.5);
        assert!((xl - x1).abs() < 1e-6);
        assert!((yl - y1).abs() < 1e-6);

        // power == 2.0 should differ from linear (non-constant speed)
        let (x2, y2) = eio_pow2.sample_position(0.2, 0.2, 1.5);
        assert!((xl - x2).abs() > 1e-6 || (yl - y2).abs() > 1e-6);
    }

    #[test]
    fn ease_in_out_and_variants_behave() {
        let base = MoveAnimation {
            to_x: 0.9,
            to_y: 0.1,
            start: 0.0,
            end: 4.0,
            easing: Easing::Linear,
        };

        // ease_in (power=1) == linear
        let ei1 = MoveAnimation {
            easing: Easing::EaseIn { power: 1.0 },
            ..base.clone()
        };
        let (xl, yl) = base.sample_position(0.1, 0.1, 1.2);
        let (xe1, ye1) = ei1.sample_position(0.1, 0.1, 1.2);
        assert!((xl - xe1).abs() < 1e-6 && (yl - ye1).abs() < 1e-6);

        // ease_in (power=2) differs from linear
        let ei2 = MoveAnimation {
            easing: Easing::EaseIn { power: 2.0 },
            ..base.clone()
        };
        let (xe2, ye2) = ei2.sample_position(0.1, 0.1, 1.2);
        assert!((xl - xe2).abs() > 1e-6 || (yl - ye2).abs() > 1e-6);

        // ease_out (power=2) should also differ
        let eo2 = MoveAnimation {
            easing: Easing::EaseOut { power: 2.0 },
            ..base.clone()
        };
        let (xo2, yo2) = eo2.sample_position(0.1, 0.1, 1.2);
        assert!((xl - xo2).abs() > 1e-6 || (yl - yo2).abs() > 1e-6);

        // ease_in_out(power) should behave as the symmetric curve (power=1 => linear)
        let eio = MoveAnimation {
            easing: Easing::EaseInOut { power: 2.0 },
            ..base.clone()
        };
        let (x_eio, y_eio) = eio.sample_position(0.1, 0.1, 1.2);
        // ensure it's different from linear at the same sample
        assert!((xl - x_eio).abs() > 1e-6 || (yl - y_eio).abs() > 1e-6);
    }

    #[test]
    fn to_dsl_emits_ease_types() {
        let ma_ei = MoveAnimation {
            easing: Easing::EaseIn { power: 1.0 },
            to_x: 0.0,
            to_y: 0.0,
            start: 0.0,
            end: 1.0,
        };
        assert!(ma_ei.to_dsl_snippet("E", "").contains("type = ease_in"));

        let ma_eo = MoveAnimation {
            easing: Easing::EaseOut { power: 2.0 },
            ..ma_ei.clone()
        };
        assert!(ma_eo
            .to_dsl_snippet("E", "")
            .contains("type = ease_out(power = 2.000)"));

        let ma_eio = MoveAnimation {
            easing: Easing::EaseInOut { power: 3.0 },
            ..ma_ei.clone()
        };
        assert!(ma_eio
            .to_dsl_snippet("E", "")
            .contains("type = ease_in_out(power = 3.000)"));
    }

    #[test]
    fn to_dsl_snippet_emits_ease_in_out_with_power() {
        let ma_default = MoveAnimation {
            to_x: 0.7,
            to_y: 0.5,
            start: 0.0,
            end: 5.0,
            easing: Easing::EaseInOut { power: 1.0 },
        };
        let s_default = ma_default.to_dsl_snippet("Circle", "    ");
        assert!(s_default.contains("type = ease_in_out"));

        let ma_pow = MoveAnimation {
            easing: Easing::EaseInOut { power: 2.0 },
            ..ma_default.clone()
        };
        let s_pow = ma_pow.to_dsl_snippet("Circle", "    ");
        assert!(s_pow.contains("type = ease_in_out(power = 2.000)"));
    }

    #[test]
    fn parse_move_block_rejects_lerp() {
        let lines = vec![
            "element = \"E\"",
            "type = lerp(power = 2.0)",
            "startAt = 0.0",
            "end {",
            "    time = 1.0",
            "    x = 0.5",
            "    y = 0.5",
            "}",
        ];

        let parsed = parse_move_block(&lines.iter().map(|s| *s).collect::<Vec<&str>>());
        // `lerp` must no longer be recognized by the parser — it should not parse as EaseInOut.
        assert!(parsed.is_some());
        let pm = parsed.unwrap();
        assert!(matches!(pm.easing, crate::scene::Easing::Linear));
    }
}

/// Result produced by parsing a `move { ... }` DSL block.
#[derive(Clone, Debug)]
pub struct ParsedMove {
    pub element: Option<String>,
    pub start: f32,
    pub end: f32,
    pub to_x: f32,
    pub to_y: f32,
    pub easing: crate::scene::Easing,
}

/// Parse the inner lines of a `move { ... }` block and return a `ParsedMove`.
///
/// Expected input is the lines inside `move { ... }` (each line already trimmed).
/// Returns `None` when required fields are missing or unparsable.
pub fn parse_move_block(lines: &[&str]) -> Option<ParsedMove> {
    let mut element: Option<String> = None;
    let mut easing_kind = crate::scene::Easing::Linear;
    let mut start_at: Option<f32> = None;
    let mut end_time: Option<f32> = None;
    let mut end_x: Option<f32> = None;
    let mut end_y: Option<f32> = None;

    let mut i = 0usize;
    while i < lines.len() {
        let b = lines[i].trim();
        if b.starts_with("element") && b.contains('=') {
            if let Some(eq) = b.find('=') {
                let val = b[eq + 1..]
                    .trim()
                    .trim_matches(',')
                    .trim()
                    .trim_matches('"')
                    .to_string();
                element = Some(val);
            }
        } else if b.starts_with("type") && b.contains('=') {
            if let Some(eq) = b.find('=') {
                let val = b[eq + 1..].trim().trim_matches(',').to_lowercase();
                if val.contains("linear") {
                    easing_kind = crate::scene::Easing::Linear;
                } else if val.starts_with("ease_in_out") {
                    let mut power = 1.0f32;
                    if let Some(open) = val.find('(') {
                        if let Some(close) = val.rfind(')') {
                            let inner = &val[open + 1..close];
                            for part in inner.split(',') {
                                let p = part.trim();
                                if p.starts_with("power") && p.contains('=') {
                                    if let Some(eq) = p.find('=') {
                                        if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                            power = v;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    easing_kind = crate::scene::Easing::EaseInOut { power };
                } else if val.starts_with("ease_in") {
                    let mut power = 1.0f32;
                    if let Some(open) = val.find('(') {
                        if let Some(close) = val.rfind(')') {
                            let inner = &val[open + 1..close];
                            for part in inner.split(',') {
                                let p = part.trim();
                                if p.starts_with("power") && p.contains('=') {
                                    if let Some(eq) = p.find('=') {
                                        if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                            power = v;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    easing_kind = crate::scene::Easing::EaseIn { power };
                } else if val.starts_with("ease_out") {
                    let mut power = 1.0f32;
                    if let Some(open) = val.find('(') {
                        if let Some(close) = val.rfind(')') {
                            let inner = &val[open + 1..close];
                            for part in inner.split(',') {
                                let p = part.trim();
                                if p.starts_with("power") && p.contains('=') {
                                    if let Some(eq) = p.find('=') {
                                        if let Ok(v) = p[eq + 1..].trim().parse::<f32>() {
                                            power = v;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    easing_kind = crate::scene::Easing::EaseOut { power };
                }
            }
        } else if b.starts_with("startAt") && b.contains('=') {
            if let Some(eq) = b.find('=') {
                if let Ok(v) = b[eq + 1..].trim().trim_matches(',').parse::<f32>() {
                    start_at = Some(v);
                }
            }
        } else if b.starts_with("end") && b.contains('{') {
            // parse nested end { ... } block
            i += 1;
            while i < lines.len() {
                let e = lines[i].trim();
                if e == "}" {
                    break;
                }
                if e.starts_with("time") && e.contains('=') {
                    if let Some(eq) = e.find('=') {
                        if let Ok(v) = e[eq + 1..].trim().trim_matches(',').parse::<f32>() {
                            end_time = Some(v);
                        }
                    }
                }
                if e.starts_with("x") && e.contains('=') {
                    if let Some(eq) = e.find('=') {
                        if let Ok(v) = e[eq + 1..].trim().trim_matches(',').parse::<f32>() {
                            end_x = Some(v);
                        }
                    }
                }
                if e.starts_with("y") && e.contains('=') {
                    if let Some(eq) = e.find('=') {
                        if let Ok(v) = e[eq + 1..].trim().trim_matches(',').parse::<f32>() {
                            end_y = Some(v);
                        }
                    }
                }
                i += 1;
            }
        }
        i += 1;
    }

    // require start, end, x, y
    if let (Some(sa), Some(et), Some(ex), Some(ey)) = (start_at, end_time, end_x, end_y) {
        Some(ParsedMove {
            element,
            start: sa,
            end: et,
            to_x: ex,
            to_y: ey,
            easing: easing_kind,
        })
    } else {
        None
    }
}
