use crate::app_state::AppState;
use eframe::egui;

/// Parse a hex color string like `"#RRGGBB"` or `"#RRGGBBAA"` into RGBA bytes.
pub fn parse_hex(hex: &str) -> Option<[u8; 4]> {
    if !hex.starts_with('#') {
        return None;
    }
    let s = &hex[1..];
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some([r, g, b, 255])
    } else if s.len() == 8 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        let a = u8::from_str_radix(&s[6..8], 16).ok()?;
        Some([r, g, b, a])
    } else {
        None
    }
}

/// Format RGBA bytes into `#rrggbb` or `#rrggbbaa` (lowercase hex).
pub fn format_hex(color: [u8; 4], alpha: bool) -> String {
    if alpha {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color[0], color[1], color[2], color[3]
        )
    } else {
        format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
    }
}

/// Find hex color literals in the DSL `TextEdit` output and render clickable
/// color picker hotspots (kept here so `code_panel.rs` isn't huge).
pub fn handle_color_pickers(
    ui: &mut egui::Ui,
    state: &mut AppState,
    output: &egui::text_edit::TextEditOutput,
) {
    let galley = &output.galley;
    let galley_pos = output.galley_pos;

    let code = &state.dsl_code;
    let mut search_idx = 0;
    while let Some(start_offset) = code[search_idx..].find('#') {
        let abs_start = search_idx + start_offset;
        let mut end = abs_start + 1;
        while end < code.len() && code.as_bytes()[end].is_ascii_hexdigit() {
            end += 1;
        }

        let hex_str = &code[abs_start..end];
        // Only show picker for valid hex colors inside quotes in the DSL
        let is_quoted = abs_start > 0
            && end < code.len()
            && code.as_bytes()[abs_start - 1] == b'"'
            && code.as_bytes()[end] == b'"';

        if is_quoted && (hex_str.len() == 7 || hex_str.len() == 9) {
            if let Some(color) = parse_hex(hex_str) {
                let cursor = egui::text::CCursor::new(abs_start - 1); // Start from the opening quote
                let galley_cursor = galley.from_ccursor(cursor);
                let pos_start = galley.pos_from_cursor(&galley_cursor);

                let cursor_end = egui::text::CCursor::new(end + 1); // Up to the closing quote
                let galley_cursor_end = galley.from_ccursor(cursor_end);
                let pos_end = galley.pos_from_cursor(&galley_cursor_end);

                let rect = egui::Rect::from_min_max(
                    galley_pos + pos_start.min.to_vec2(),
                    galley_pos + pos_end.max.to_vec2(),
                );

                if ui.clip_rect().intersects(rect) {
                    let response = ui.interact(
                        rect,
                        ui.make_persistent_id(("hex_click", abs_start)),
                        egui::Sense::click(),
                    );

                    if response.hovered() {
                        ui.painter().rect_stroke(
                            rect.expand(1.0),
                            2.0,
                            egui::Stroke::new(1.0, egui::Color32::from_gray(100)),
                        );
                    }

                    if response.clicked() {
                        state.color_picker_data = Some(crate::app_state::ColorPickerData {
                            range: abs_start..end,
                            color,
                            is_alpha: hex_str.len() == 9,
                        });
                    }
                }
            }
        }
        search_idx = end;
    }
}

/// Return the byte index of the matching `)` for the `(` located at
/// `open_pos` inside `s`. Returns `None` when no matching closing paren
/// is found. The function ignores parentheses that appear inside string
/// literals and correctly handles escaped quotes (e.g. `"`).
///
/// Originally defined in `code_panel::mod`, this helper is now colocated with
/// other text-related utilities.
pub fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
    if open_pos >= s.len() {
        return None;
    }

    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut prev_was_escape = false;

    for (i, ch) in s[open_pos..].char_indices() {
        let idx = open_pos + i; // byte index relative to `s`

        // Toggle string state unless the quote is escaped.
        if ch == '"' && !prev_was_escape {
            in_string = !in_string;
            prev_was_escape = false;
            continue;
        }

        // Track escape state for the next character inside strings
        prev_was_escape = ch == '\\' && in_string && !prev_was_escape;

        if in_string {
            continue;
        }

        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format_roundtrip() {
        let h = "#78c8ff";
        let parsed = parse_hex(h).expect("parsed");
        assert_eq!(parsed, [0x78, 0xc8, 0xff, 255]);
        let fmt = format_hex(parsed, false);
        assert_eq!(fmt, h);

        let h2 = "#11223344";
        let p2 = parse_hex(h2).expect("parsed2");
        assert_eq!(p2, [0x11, 0x22, 0x33, 0x44]);
        assert_eq!(format_hex(p2, true), h2);
    }
}
