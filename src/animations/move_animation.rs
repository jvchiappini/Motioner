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
            Easing::Linear => {
                let ix = base_x + local_t * (self.to_x - base_x);
                let iy = base_y + local_t * (self.to_y - base_y);
                (ix, iy)
            }
        }
    }

    /// Produce the DSL snippet for this animation (indent must include trailing spaces if needed).
    pub fn to_dsl_snippet(&self, element_name: &str, indent: &str) -> String {
        // match previous formatting used by `scene::to_dsl_impl`
        let mut out = String::new();
        out.push_str(&format!("\n{}move {{\n", indent));
        out.push_str(&format!("{}    element = \"{}\"\n", indent, element_name));
        out.push_str(&format!("{}    type = linear\n", indent));
        out.push_str(&format!("{}    startAt = {:.3}\n", indent, self.start));
        out.push_str(&format!("{}    end {{\n", indent));
        out.push_str(&format!("{}        time = {:.3}\n", indent, self.end));
        out.push_str(&format!("{}        x = {:.3}\n", indent, self.to_x));
        out.push_str(&format!("{}        y = {:.3}\n", indent, self.to_y));
        out.push_str(&format!("{}    }}\n", indent));
        out.push_str(&format!("{}}}", indent));
        out
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
