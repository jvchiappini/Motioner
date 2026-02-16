use crate::app_state::AppState;
use crate::autocomplete; // Added this
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        // Save button removed — autosave will persist while editing.
        if ui
            .button(if state.code_fullscreen {
                "📉 Minimize"
            } else {
                "📈 Maximize"
            })
            .clicked()
        {
            state.code_fullscreen = !state.code_fullscreen;
        }

        // Autosave indicator (replaces toast for editor autosaves)
        let now = ui.ctx().input(|i| i.time);
        if state.autosave_pending {
            ui.label(egui::RichText::new("Autosaving…").weak());
        } else if let Some(err) = &state.autosave_error {
            ui.colored_label(egui::Color32::from_rgb(220, 100, 100), "Autosave failed")
                .on_hover_text(err);
        } else if let Some(t) = state.autosave_last_success_time {
            if now - t < 2.0 {
                ui.colored_label(egui::Color32::from_rgb(120, 200, 140), "Autosaved ✓");
            } else {
                ui.label(
                    egui::RichText::new("Edit code — autosave while typing")
                        .italics()
                        .weak(),
                );
            }
        } else {
            ui.label(
                egui::RichText::new("Edit code — autosave while typing")
                    .italics()
                    .weak(),
            );
        }
    });
    ui.separator();

    let defined_names: std::collections::HashSet<String> =
        state.scene.iter().map(|s| s.name().to_string()).collect();

    let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
        let mut layout_job = egui::text::LayoutJob::default();
        highlight_code(&mut layout_job, string, &defined_names); // custom highlighter
        layout_job.wrap.max_width = wrap_width; // no wrapping
        ui.fonts(|f| f.layout_job(layout_job))
    };

    let available_rect = ui.available_rect_before_wrap();
    // Paint the background for the whole scroll area to look like the editor
    ui.painter()
        .rect_filled(available_rect, 0.0, egui::Color32::from_rgb(10, 10, 10));

    let is_fullscreen = state.code_fullscreen;
    let minimap_width = if is_fullscreen { 120.0 } else { 0.0 };

    // Editor Area Rect (Full width or minus minimap)
    let editor_rect = egui::Rect::from_min_max(
        available_rect.min,
        egui::pos2(available_rect.max.x - minimap_width, available_rect.max.y),
    );

    // Create a child UI for the editor to constrain it
    let mut editor_ui = ui.child_ui(editor_rect, *ui.layout());

    let scroll_output = egui::ScrollArea::vertical()
        .id_source("code_editor_scroll")
        .auto_shrink([false, false]) // Use full available space
        .show(&mut editor_ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(ui.available_height()); // Ensure clickable area covers view

            let text_edit_id = ui.make_persistent_id("dsl_text_edit");

            // 1. Process Input BEFORE TextEdit (Consume keys)
            autocomplete::process_input(ui, text_edit_id, state);

            // Editor with a left gutter for line numbers (VSCode-style)
            let gutter_width = 56.0f32;

            // We'll capture the TextEdit response so it remains available after the
            // `ui.horizontal` closure (needed by autosave logic below).
            let mut editor_response: Option<egui::Response> = None;

            ui.horizontal(|ui| {
                // Reserve gutter area (will scroll together with the TextEdit because
                // both are inside the same ScrollArea closure). Use a clickable rect
                // so we can detect gutter clicks via the returned Response.
                let gutter_response = ui.allocate_rect(
                    egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(gutter_width, ui.available_height()),
                    ),
                    egui::Sense::click(),
                );
                let gutter_rect = gutter_response.rect;

                // Main code text edit
                let rows = state.dsl_code.lines().count().max(6); // ensure a reasonable minimum height
                let text_edit = egui::TextEdit::multiline(&mut state.dsl_code)
                    .id(text_edit_id) // Explicit ID
                    .font(egui::TextStyle::Monospace) // for cursor height
                    .code_editor()
                    .desired_rows(rows)
                    .frame(false) // Transparent frame so background shows through
                    .desired_width(f32::INFINITY)
                    .lock_focus(true)
                    .layouter(&mut layouter);

                let output = text_edit.show(ui);
                let output_rect = output.response.rect; // capture rect so we can draw while still owning `output`

                // 2. Update State & Render Popup AFTER TextEdit
                autocomplete::handle_state_and_render(ui, &output.response, state);

                // --- GUTTER RENDERING ---
                // Match the font size used in highlight_code (14.0)
                let font_id = egui::FontId::monospace(14.0);

                // Determine active logical line efficiently
                let mut active_line_idx: usize = 0;
                if let Some(te_state) = egui::TextEdit::load_state(ui.ctx(), text_edit_id) {
                    if let Some(range) = te_state.cursor.char_range() {
                        let char_idx = range.primary.index;
                        active_line_idx = state
                            .dsl_code
                            .chars()
                            .take(char_idx)
                            .filter(|&c| c == '\n')
                            .count();
                    }
                }

                // Draw gutter background
                let mut full_gutter_rect = gutter_rect;
                full_gutter_rect.set_bottom(ui.clip_rect().bottom().max(output_rect.bottom()));

                let gutter_painter = ui.painter().with_clip_rect(full_gutter_rect);
                gutter_painter.rect_filled(
                    full_gutter_rect,
                    0.0,
                    egui::Color32::from_rgb(24, 24, 24),
                );

                let gutter_text_color = egui::Color32::from_gray(100);

                // Use the galley to accurately position line numbers, handling wrapping and different scales
                let galley = &output.galley;
                let galley_pos = output.galley_pos;

                let mut current_logical_line = 0;

                for row in &galley.rows {
                    // Get character start index for this row using the galley's cursor system
                    // We use the vertical center of the row to ensure we hit the right row
                    let row_center_y = row.rect.center().y;
                    let cursor = galley.cursor_from_pos(egui::vec2(0.0, row_center_y));
                    let row_start_idx = cursor.ccursor.index;

                    // A row is the start of a logical line if it starts at index 0
                    // or if the character immediately preceding it is a newline.
                    let is_start_of_logical_line = row_start_idx == 0 || {
                        let prev_idx = row_start_idx - 1;
                        state.dsl_code.chars().nth(prev_idx) == Some('\n')
                    };

                    if is_start_of_logical_line {
                        let line_index = current_logical_line;
                        current_logical_line += 1;

                        // Calculate Y position based on the galley's layout
                        let y = galley_pos.y + row.rect.top();

                        // Optimization: Skip drawing if outside the visible clip rect
                        if y + row.rect.height() < ui.clip_rect().top() {
                            continue;
                        }
                        if y > ui.clip_rect().bottom() {
                            break;
                        }

                        let num = format!("{}", line_index + 1);
                        let is_active = line_index == active_line_idx;

                        gutter_painter.text(
                            egui::pos2(full_gutter_rect.right() - 8.0, y),
                            egui::Align2::RIGHT_TOP,
                            num,
                            font_id.clone(),
                            if is_active {
                                egui::Color32::from_rgb(220, 220, 220)
                            } else {
                                gutter_text_color
                            },
                        );
                    }
                }

                // Make gutter clickable: clicking a line will move cursor to line start
                if gutter_response.clicked() {
                    if let Some(pos) = ui.ctx().pointer_interact_pos() {
                        let galley_y = pos.y - galley_pos.y;

                        // Use the galley to find the cursor at the clicked position
                        let cursor = galley.cursor_from_pos(egui::vec2(0.0, galley_y));
                        let char_idx = cursor.ccursor.index;
                        let ccursor = egui::text::CCursor::new(char_idx);

                        if let Some(mut te_state) =
                            egui::TextEdit::load_state(ui.ctx(), text_edit_id)
                        {
                            te_state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                            egui::TextEdit::store_state(ui.ctx(), text_edit_id, te_state);
                        }
                    }
                }

                // finally store the response so outer scope can use it
                editor_response = Some(output.response);
            });

            // retrieve the TextEdit response we captured from the horizontal layout
            let output = editor_response.expect("text edit response");

            // Autosave behavior: on any editor change, persist DSL silently,
            // attempt to parse/apply configuration and regenerate preview if parse succeeds.
            if output.changed() {
                // mark edit time so App::update will debounce the actual disk write
                state.last_code_edit_time = Some(ui.ctx().input(|i| i.time));
                state.autosave_pending = true;

                // Parsing and scene/preview updates are debounced and handled in `ui::update`
            }

            output
        });

    if state.completion_popup_open {
        // We still need a repaint for the popup to show up if it's new,
        // but it's better handled in handle_state_and_render now.
        // I'll keep it here just in case but without the spam.
    }

    if is_fullscreen {
        let minimap_rect = egui::Rect::from_min_max(
            egui::pos2(available_rect.max.x - minimap_width, available_rect.min.y),
            available_rect.max,
        );
        render_minimap(
            ui,
            minimap_rect,
            &state.dsl_code,
            scroll_output.state.offset,
            editor_rect.height(),
        );
    }
}

fn render_minimap(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    code: &str,
    scroll_offset: egui::Vec2,
    viewport_height: f32,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(18, 18, 18)); // Minimap bg

    // Minimap Rendering Parameters
    let code_line_height = 14.0; // Assuming monospace(14.0) from highlight_code
    let minimap_scale = 0.2; // 20% size
    let mm_line_height = code_line_height * minimap_scale;
    let mm_char_width = 8.0 * minimap_scale; // Approx char width

    // Draw Code Blocks
    // We can't draw every char as a rect efficiently if the file is huge,
    // but for DSL it's likely small efficiently.
    // Optimization: Draw lines as simplified blocks.

    let start_y = rect.top() + 4.0;

    let mut y = start_y;

    // Very simple highlighting for minimap (simplified parser)
    for line in code.lines() {
        // Skip rendering if out of bounds (optimization)
        if y > rect.bottom() {
            break;
        }

        let mut x = rect.left() + 4.0;

        // Simple "words" tokenizer
        // let mut word_color = egui::Color32::from_gray(100);

        // Heuristic: Color whole words based on first char or context?
        // Let's just iterate chars for "accurate" mini-view
        // If too slow, switch to block words.

        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if x > rect.right() {
                break;
            }

            let color = if c.is_whitespace() {
                None
            } else if c.is_ascii_digit() {
                Some(egui::Color32::from_rgb(181, 206, 168)) // Number green
            } else if "\"".contains(c) {
                Some(egui::Color32::from_rgb(206, 145, 120)) // String
            } else if "()[]{}".contains(c) {
                Some(egui::Color32::from_rgb(255, 200, 50)) // Bracket Gold
            } else if c == '/' && chars.peek() == Some(&'/') {
                // Comment - rest of line
                Some(egui::Color32::from_rgb(90, 120, 90)) // Comment Green
            } else if c.is_lowercase() {
                Some(egui::Color32::from_rgb(86, 156, 214)) // Keyword/Param Blue
            } else if c.is_uppercase() {
                Some(egui::Color32::from_rgb(78, 201, 176)) // Object Teal
            } else {
                Some(egui::Color32::from_gray(120))
            };

            if let Some(col) = color {
                // If it's a comment, draw a long bar
                if col == egui::Color32::from_rgb(80, 100, 80) {
                    let len = line.len() as f32 * mm_char_width; // Approx
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(x, y),
                            egui::vec2(len.min(rect.right() - x), mm_line_height * 0.8),
                        ),
                        1.0,
                        col,
                    );
                    break; // Next line
                }

                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(mm_char_width, mm_line_height * 0.8),
                    ),
                    0.5,
                    col,
                );
            }
            x += mm_char_width;
        }

        y += mm_line_height * 1.5; // Line spacing
    }

    // Draw Viewport Shadow Overlay
    // Map scroll_offset.y to minimap y
    let scroll_y = scroll_offset.y;
    let mm_scroll_y = scroll_y * minimap_scale;
    let mm_viewport_h = viewport_height * minimap_scale;

    let highlight_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), start_y + mm_scroll_y),
        egui::vec2(rect.width(), mm_viewport_h),
    );

    // Draw semi-transparent overlay over the NON-visible parts?
    // Or highlight the visible part. VS Code highlights the visible part with a light hover effect.
    painter.rect_filled(highlight_rect, 0.0, egui::Color32::from_white_alpha(15));
    painter.rect_stroke(
        highlight_rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(30)),
    );
}

fn highlight_code(
    job: &mut egui::text::LayoutJob,
    code: &str,
    defined_names: &std::collections::HashSet<String>,
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
            append_text(
                job,
                &code[start..end],
                &font_id,
                egui::Color32::from_rgb(206, 145, 120),
            ); // VSCode String Color
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
                | "points" | "power" => egui::Color32::from_rgb(220, 220, 170), // Gold (#DCDCAA)

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
