use crate::scene::Easing;

pub mod bezier;
pub mod custom;
pub mod ease_in;
pub mod ease_in_out;
pub mod ease_out;
pub mod linear;
pub mod sine;
pub mod expo;
pub mod circ;
pub mod spring;
pub mod elastic;
pub mod bounce;

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

        let progress = match &self.easing {
            Easing::Linear => linear::compute_progress(local_t),
            Easing::EaseIn { power } => ease_in::compute_progress(local_t, *power),
            Easing::EaseOut { power } => ease_out::compute_progress(local_t, *power),
            Easing::EaseInOut { power } => ease_in_out::compute_progress(local_t, *power),
            Easing::Custom { points } => custom::compute_progress(local_t, points),
            Easing::Bezier { p1, p2 } => bezier::compute_progress(local_t, *p1, *p2),
            Easing::Sine => sine::compute_progress(local_t),
            Easing::Expo => expo::compute_progress(local_t),
            Easing::Circ => circ::compute_progress(local_t),
            Easing::Spring { damping, stiffness, mass } => {
                spring::compute_progress(local_t, *damping, *stiffness, *mass)
            }
            Easing::Elastic { amplitude, period } => {
                elastic::compute_progress(local_t, *amplitude, *period)
            }
            Easing::Bounce { bounciness } => bounce::compute_progress(local_t, *bounciness),
        };

        let ix = base_x + progress * (self.to_x - base_x);
        let iy = base_y + progress * (self.to_y - base_y);
        (ix, iy)
    }

    /// Sample a sequence of positions for this animation at a given FPS.
    pub fn positions_by_frame(&self, base_x: f32, base_y: f32, fps: u32) -> Vec<(f32, f32, f32)> {
        let mut frames = Vec::new();
        let duration = (self.end - self.start).abs();
        if duration < std::f32::EPSILON {
            frames.push((self.start, self.to_x, self.to_y));
            return frames;
        }

        let num_steps = (duration * fps as f32).round() as u32;
        let step = duration / num_steps as f32;

        for i in 0..=num_steps {
            let t = self.start + i as f32 * step;
            let (x, y) = self.sample_position(base_x, base_y, t);
            frames.push((t, x, y));
        }
        frames
    }

    pub fn to_dsl_block(&self, element_name: Option<&str>, indent: &str) -> String {
        let ease_str = match &self.easing {
            Easing::Linear => "linear".to_string(),
            Easing::EaseIn { power } => format!("ease_in(power = {:.3})", power),
            Easing::EaseOut { power } => format!("ease_out(power = {:.3})", power),
            Easing::EaseInOut { power } => format!("ease_in_out(power = {:.3})", power),
            Easing::Bezier { p1, p2 } => format!(
                "bezier(p1 = ({:.3}, {:.3}), p2 = ({:.3}, {:.3}))",
                p1.0, p1.1, p2.0, p2.1
            ),
            Easing::Custom { points } => {
                let pts: Vec<String> = points
                    .iter()
                    .map(|(t, v)| format!("({:.3}, {:.3})", t, v))
                    .collect();
                format!("custom(points = [{}])", pts.join(", "))
            }
            Easing::Sine => "sine".to_string(),
            Easing::Expo => "expo".to_string(),
            Easing::Circ => "circ".to_string(),
            Easing::Spring { damping, stiffness, mass } => {
                spring::to_dsl_string(*damping, *stiffness, *mass)
            }
            Easing::Elastic { amplitude, period } => elastic::to_dsl_string(*amplitude, *period),
            Easing::Bounce { bounciness } => bounce::to_dsl_string(*bounciness),
        };

        let mut out = format!("{}move {{\n", indent);
        let inner_indent = format!("{}    ", indent);

        if let Some(name) = element_name {
            out.push_str(&format!("{}element = \"{}\",\n", inner_indent, name));
        }

        out.push_str(&format!(
            "{}to = ({:.3}, {:.3}),\n",
            inner_indent, self.to_x, self.to_y
        ));
        out.push_str(&format!(
            "{}during = {:.3} -> {:.3},\n",
            inner_indent, self.start, self.end
        ));
        out.push_str(&format!("{}ease = {}\n", inner_indent, ease_str));
        out.push_str(&format!("{}}}\n", indent));
        out
    }

    pub fn to_dsl_snippet(&self, element_name: &str, indent: &str) -> String {
        self.to_dsl_block(None, indent)
    }
}
=

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
        if b.is_empty() {
            i += 1;
            continue;
        }

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
        } else if (b.starts_with("type") || b.starts_with("ease")) && b.contains('=') {
            if let Some(eq) = b.find('=') {
                let val = b[eq + 1..].trim().trim_matches(',').to_lowercase();
                // Support ease/easing (preferred) or type (legacy)
                let clean_val = if val.starts_with("type =") {
                    val.clone()
                } else {
                    format!("type = {}", val)
                };
                

                if let Some(e) = linear::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = ease_in_out::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = ease_in::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = ease_out::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = sine::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = expo::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = circ::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = spring::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = elastic::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = bounce::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = custom::parse_dsl(&clean_val) {
                    easing_kind = e;
                } else if let Some(e) = bezier::parse_dsl(&clean_val) {
                    easing_kind = e;
                }
            }
        } else if b.starts_with("to") && b.contains('=') {
            // to = (x, y)
            if let Some(eq) = b.find('=') {
                let val = b[eq + 1..].trim().trim_matches(',');
                if val.starts_with('(') && val.ends_with(')') {
                    let inner = &val[1..val.len() - 1];
                    let parts: Vec<&str> = inner.split(',').collect();
                    if parts.len() == 2 {
                        end_x = parts[0].trim().parse().ok();
                        end_y = parts[1].trim().parse().ok();
                    }
                }
            }
        } else if b.starts_with("during") && b.contains('=') {
            // during = start -> end
            if let Some(eq) = b.find('=') {
                let val = b[eq + 1..].trim().trim_matches(',');
                if let Some(arrow) = val.find("->") {
                    start_at = val[..arrow].trim().parse().ok();
                    end_time = val[arrow + 2..].trim().parse().ok();
                }
            }
        } else if b.starts_with("startAt") && b.contains('=') {
            if let Some(eq) = b.find('=') {
                if let Ok(v) = b[eq + 1..].trim().trim_matches(',').parse::<f32>() {
                    start_at = Some(v);
                }
            }
        } else if b.starts_with("end") && b.contains('{') {
            // parse nested end { ... } block (legacy support)
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
