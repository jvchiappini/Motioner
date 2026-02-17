use eframe::egui;

pub(crate) fn highlight_code(
    job: &mut egui::text::LayoutJob,
    code: &str,
    defined_names: &std::collections::HashSet<String>,
    handlers: &[crate::dsl::runtime::DslHandler],
) {
    let font_id = egui::FontId::monospace(14.0);

    // Simple tokenizer based on characters
    let mut chars = code.char_indices().peekable();
    let mut last_idx = 0;

    // Rainbow bracket colors (Pastel/Neon for dark theme)
    let rainbow_colors = [
        egui::Color32::from_rgb(255, 100, 100), // Red
        egui::Color32::from_rgb(255, 200, 0),   // Orange/Gold
        egui::Color32::from_rgb(255, 255, 0),   // Yellow
        egui::Color32::from_rgb(50, 255, 50),   // Green
        egui::Color32::from_rgb(50, 200, 255),  // Blue
        egui::Color32::from_rgb(200, 100, 255), // Purple
        egui::Color32::from_rgb(255, 100, 200), // Pink
    ];
    let mut bracket_depth: usize = 0;

    while let Some((idx, c)) = chars.next() {
        // 1. Whitespace
        if c.is_whitespace() {
            if idx > last_idx {
                append_text(
                    job,
                    &code[last_idx..idx],
                    &font_id,
                    egui::Color32::LIGHT_GRAY,
                );
            }
            append_text(
                job,
                &code[idx..idx + 1],
                &font_id,
                egui::Color32::LIGHT_GRAY,
            );
            last_idx = idx + 1;
            continue;
        }

        // 2. Comments (// ...)
        if c == '/' {
            if let Some((_, '/')) = chars.peek() {
                chars.next(); // eat second slash
                let start = idx;
                let mut end = idx + 2;
                while let Some((i, next_c)) = chars.peek() {
                    if *next_c == '\n' {
                        break;
                    }
                    end = *i + 1;
                    chars.next();
                }
                append_text(
                    job,
                    &code[last_idx..start],
                    &font_id,
                    egui::Color32::LIGHT_GRAY,
                );
                append_text(
                    job,
                    &code[start..end],
                    &font_id,
                    egui::Color32::from_rgb(90, 120, 90),
                ); // Greenish comment
                last_idx = end;
                continue;
            }
        }

        // 3. Strings ("...")
        if c == '"' {
            append_text(
                job,
                &code[last_idx..idx],
                &font_id,
                egui::Color32::LIGHT_GRAY,
            );

            let start = idx;
            let mut end = idx + 1;
            while let Some((i, next_c)) = chars.next() {
                end = i + 1;
                if next_c == '"' {
                    break;
                }
            }

            let content = &code[start..end];
            // Check if it's a hex color: "#RRGGBB" or "#RRGGBBAA"
            if content.len() == 9 || content.len() == 11 {
                let inner = &content[1..content.len() - 1];
                if let Some(c) = crate::code_panel::utils::parse_hex(inner) {
                    // Highlight hex content with its actual color
                    append_text(job, "\"", &font_id, egui::Color32::from_rgb(206, 145, 120));
                    append_text(
                        job,
                        inner,
                        &font_id,
                        egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
                    );
                    append_text(job, "\"", &font_id, egui::Color32::from_rgb(206, 145, 120));
                } else {
                    append_text(
                        job,
                        content,
                        &font_id,
                        egui::Color32::from_rgb(206, 145, 120),
                    );
                }
            } else {
                append_text(
                    job,
                    content,
                    &font_id,
                    egui::Color32::from_rgb(206, 145, 120),
                );
            }
            last_idx = end;
            continue;
        }

        // 4. Brackets (Rainbow)
        if "()[]{}".contains(c) {
            append_text(
                job,
                &code[last_idx..idx],
                &font_id,
                egui::Color32::LIGHT_GRAY,
            );

            let color_idx = if ")]}".contains(c) {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
                bracket_depth
            } else {
                let d = bracket_depth;
                bracket_depth += 1;
                d
            };

            let color = rainbow_colors[color_idx % rainbow_colors.len()];
            append_text(job, &code[idx..idx + 1], &font_id, color);
            last_idx = idx + 1;
            continue;
        }

        // 5. DSL Blocks, Parameters and Identifiers
        if c.is_alphabetic() || c == '_' {
            if idx > last_idx {
                append_text(
                    job,
                    &code[last_idx..idx],
                    &font_id,
                    egui::Color32::LIGHT_GRAY,
                );
            }

            let start = idx;
            let mut end = idx + 1;
            while let Some((i, next_c)) = chars.peek() {
                if next_c.is_alphanumeric() || *next_c == '_' {
                    end = *i + 1;
                    chars.next();
                } else {
                    break;
                }
            }

            let word = &code[start..end];
            // If this identifier matches a registered DSL handler, use its color.
            if let Some(h) = handlers.iter().find(|h| h.name == word) {
                let c = h.color;
                append_text(
                    job,
                    word,
                    &font_id,
                    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
                );
                last_idx = end;
                continue;
            }

            // If this identifier is a move-element-style utility, try to infer a color
            // from an upcoming `color = "#rrggbb"` parameter in the call. Fallback to
            // the default object-teal color.
            // If this identifier is a known DSL *method* (e.g. `move_element`),
            // try to parse the call to infer a per-call color; otherwise use
            // the centralized method color registry as a fallback. This makes it
            // trivial to add new methods + colors via `dsl::method_color()`.
            if let Some(mcol) = crate::dsl::method_color(word) {
                let rest = &code[end..];
                if let Some(open_paren_pos) = rest.find('(') {
                    if let Some(close_paren_pos) =
                        crate::code_panel::find_matching_paren(rest, open_paren_pos)
                    {
                        if let Some(call_end) = end.checked_add(close_paren_pos + 1) {
                            if call_end <= code.len() {
                                let call_substr = &code[start..call_end];
                                // Special-case parsing for move_element (centralized parser)
                                if word == "move_element" {
                                    if let Ok(me) = crate::shapes::utilities::move_element::MoveElement::parse_dsl(call_substr) {
                                        let c = me.color;
                                        append_text(job, word, &font_id, egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]));
                                        last_idx = end;
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }

                // Fallback to the registry color for this method
                append_text(
                    job,
                    word,
                    &font_id,
                    egui::Color32::from_rgba_unmultiplied(mcol[0], mcol[1], mcol[2], mcol[3]),
                );
                last_idx = end;
                continue;
            }

            let color = match word {
                // Primary Block Keywords
                "circle" | "rect" | "move" | "size" | "timeline" => {
                    egui::Color32::from_rgb(86, 156, 214)
                } // Blue (#569CD6)

                // Parameters / Properties
                "name" | "x" | "y" | "radius" | "w" | "h" | "width" | "height" | "fill"
                | "spawn" | "to" | "during" | "ease" | "startAt" | "time" | "element" | "type"
                | "fps" | "duration" => egui::Color32::from_rgb(156, 220, 254), // Light Blue (#9CDCFE)

                // Values / Constants / Easings
                "linear" | "ease_in" | "ease_out" | "ease_in_out" | "bezier" | "custom"
                | "sine" | "expo" | "circ" | "spring" | "elastic" | "bounce" | "points"
                | "power" | "damping" | "stiffness" | "mass" | "amplitude" | "period"
                | "bounciness" => egui::Color32::from_rgb(220, 220, 170), // Gold (#DCDCAA)

                _ => {
                    // Check if this is a defined object name
                    if defined_names.contains(word) {
                        egui::Color32::from_rgb(78, 201, 176) // Teal (#4EC9B0) for Objects
                    } else {
                        egui::Color32::LIGHT_GRAY
                    }
                }
            };

            append_text(job, word, &font_id, color);
            last_idx = end;
            continue;
        }

        // 6. Numbers
        if c.is_ascii_digit() {
            if idx > last_idx {
                append_text(
                    job,
                    &code[last_idx..idx],
                    &font_id,
                    egui::Color32::LIGHT_GRAY,
                );
            }
            let start = idx;
            let mut end = idx + 1;
            while let Some((i, next_c)) = chars.peek() {
                if next_c.is_ascii_digit() || *next_c == '.' {
                    end = *i + 1;
                    chars.next();
                } else {
                    break;
                }
            }
            append_text(
                job,
                &code[start..end],
                &font_id,
                egui::Color32::from_rgb(181, 206, 168),
            ); // Light Green number
            last_idx = end;
            continue;
        }

        // 7. Operators and separators
        if "=->,".contains(c) {
            append_text(
                job,
                &code[last_idx..idx],
                &font_id,
                egui::Color32::LIGHT_GRAY,
            );
            append_text(
                job,
                &code[idx..idx + 1],
                &font_id,
                egui::Color32::from_rgb(212, 212, 212),
            ); // Subtle gray for ops
            last_idx = idx + 1;
            continue;
        }
    }

    // Flush remaining
    if last_idx < code.len() {
        append_text(job, &code[last_idx..], &font_id, egui::Color32::LIGHT_GRAY);
    }
}

fn append_text(
    job: &mut egui::text::LayoutJob,
    text: &str,
    font_id: &egui::FontId,
    color: egui::Color32,
) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        egui::text::TextFormat {
            font_id: font_id.clone(),
            color,
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_handles_simple_and_modified_input() {
        let mut job = egui::text::LayoutJob::default();
        let handlers: Vec<crate::dsl::runtime::DslHandler> = Vec::new();
        let defined_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        // original valid call
        let s1 = "on_time { move_element(name = \"Circle\", x = seconds * 0.1, y = 0.5) }";
        highlight_code(&mut job, s1, &defined_names, &handlers);

        // modified (different name / values) — should not panic and should complete
        let mut job2 = egui::text::LayoutJob::default();
        let s2 = "on_time { move_element(name = \"Circle2\", x = 0.2, y = 0.6) }";
        highlight_code(&mut job2, s2, &defined_names, &handlers);
    }
}
