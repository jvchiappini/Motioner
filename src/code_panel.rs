use crate::app_state::{AppState, ToastType};
use crate::dsl;
use crate::autocomplete; // Added this
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        if ui.button("💾 Save").clicked() {
            match dsl::parse_config(&state.dsl_code) {
                Ok(config) => {
                    state.fps = config.fps;
                    state.duration_secs = config.duration;
                    state.render_width = config.width;
                    state.render_height = config.height;
                    // Attempt to parse DSL into scene shapes (best-effort)
                    let parsed = dsl::parse_dsl(&state.dsl_code);
                    if !parsed.is_empty() {
                        state.scene = parsed;
                        // regenerate preview for current playhead
                        crate::canvas::request_preview_frames(state, state.time, crate::canvas::PreviewMode::Single);
                    }
                    
                    state.toast_message = Some("Configuration updated successfully!".to_string());
                    state.toast_type = ToastType::Success;
                    state.toast_deadline = ui.input(|i| i.time) + 3.0;
                }
                Err(e) => {
                    state.toast_message = Some(format!("Error: {}", e));
                    state.toast_type = ToastType::Error;
                    state.toast_deadline = ui.input(|i| i.time) + 5.0;
                }
            }
        }

        if ui.button(if state.code_fullscreen { "📉 Minimize" } else { "📈 Maximize" }).clicked() {
            state.code_fullscreen = !state.code_fullscreen;
            // state.code_fullscreen_time = Some(ui.input(|i| i.time)); // No longer used
        }

        ui.label(egui::RichText::new("Edit code to update project settings").italics().weak());
    });
    ui.separator();

    let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
        let mut layout_job = egui::text::LayoutJob::default();
        highlight_code(&mut layout_job, string); // custom highlighter
        layout_job.wrap.max_width = wrap_width; // no wrapping
        ui.fonts(|f| f.layout_job(layout_job))
    };

    let available_rect = ui.available_rect_before_wrap();
    // Paint the background for the whole scroll area to look like the editor
    ui.painter().rect_filled(available_rect, 0.0, egui::Color32::from_rgb(10, 10, 10));

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

            let text_edit = egui::TextEdit::multiline(&mut state.dsl_code)
                    .id(text_edit_id) // Explicit ID
                    .font(egui::TextStyle::Monospace) // for cursor height
                    .code_editor()
                    .frame(false) // Transparent frame so background shows through
                    .desired_width(f32::INFINITY)
                    .lock_focus(true)
                    .layouter(&mut layouter);
            
            let output = ui.add(text_edit);
            
            // 2. Update State & Render Popup AFTER TextEdit
            autocomplete::handle_state_and_render(ui, &output, state);

            output
        });
        
    // If we just inserted text, we might want to force focus back or update cursor?
    // In immediate mode, the state.dsl_code update usually reflects next frame.
    if state.completion_popup_open {
        ui.ctx().request_repaint();
    }

    if is_fullscreen {
        let minimap_rect = egui::Rect::from_min_max(
            egui::pos2(available_rect.max.x - minimap_width, available_rect.min.y),
            available_rect.max,
        );
        render_minimap(ui, minimap_rect, &state.dsl_code, scroll_output.state.offset, editor_rect.height());
    }
}

fn render_minimap(ui: &mut egui::Ui, rect: egui::Rect, code: &str, scroll_offset: egui::Vec2, viewport_height: f32) {
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
        if y > rect.bottom() { break; }
        
        let mut x = rect.left() + 4.0;
        
        // Simple "words" tokenizer
        // let mut word_color = egui::Color32::from_gray(100);
        
        // Heuristic: Color whole words based on first char or context?
        // Let's just iterate chars for "accurate" mini-view
        // If too slow, switch to block words.
        
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
             if x > rect.right() { break; }

             let color = if c.is_whitespace() {
                 None
             } else if c.is_ascii_digit() {
                 Some(egui::Color32::from_rgb(150, 200, 150)) // Number green
             } else if "\"".contains(c) {
                 Some(egui::Color32::from_rgb(200, 140, 120)) // String red/orange
             } else if "()[]{}".contains(c) {
                 Some(egui::Color32::from_rgb(255, 200, 50)) // Bracket Gold
             } else if c == '/' && chars.peek() == Some(&'/') {
                 // Comment - rest of line
                 Some(egui::Color32::from_rgb(80, 100, 80)) // Comment Green
             } else if c.is_uppercase() {
                 Some(egui::Color32::from_rgb(70, 200, 170)) // Type
             } else if c == 'f' || c == 'l' || c == 'p' { 
                 // Weak heuristic for keywords (fn, let, pub, project, etc)
                  Some(egui::Color32::from_rgb(80, 150, 210)) // Keyword Blue-ish
             } else {
                 Some(egui::Color32::from_gray(120))
             };
             
             if let Some(col) = color {
                 // If it's a comment, draw a long bar
                 if col == egui::Color32::from_rgb(80, 100, 80) {
                      let len = line.len() as f32 * mm_char_width; // Approx
                      painter.rect_filled(
                          egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(len.min(rect.right() - x), mm_line_height * 0.8)),
                          1.0,
                          col
                      );
                      break; // Next line
                 }
                 
                 painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(mm_char_width, mm_line_height * 0.8)),
                    0.5,
                    col
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
        egui::vec2(rect.width(), mm_viewport_h)
    );
    
    // Draw semi-transparent overlay over the NON-visible parts? 
    // Or highlight the visible part. VS Code highlights the visible part with a light hover effect.
    painter.rect_filled(
        highlight_rect, 
        0.0, 
        egui::Color32::from_white_alpha(15)
    );
    painter.rect_stroke(
        highlight_rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(30))
    );
}

fn highlight_code(job: &mut egui::text::LayoutJob, code: &str) {
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
        // 1. Whitespace - just append
        if c.is_whitespace() {
            // Check if we need to flush previous token? (Handled inside token blocks usually)
            continue; 
        }

        // 2. Comments (// ...)
        if c == '/' {
             if let Some((_, '/')) = chars.peek() {
                 // Consume until newline
                 chars.next(); // eat second slash
                 let start = idx;
                 let mut end = idx + 2;
                 while let Some((i, next_c)) = chars.peek() {
                     if *next_c == '\n' { break; }
                     end = *i + 1;
                     chars.next();
                 }
                 append_text(job, &code[last_idx..start], &font_id, egui::Color32::LIGHT_GRAY);
                 append_text(job, &code[start..end], &font_id, egui::Color32::from_rgb(90, 120, 90)); // Greenish comment
                 last_idx = end;
                 continue;
             }
        }

        // 3. Strings ("...")
        if c == '"' {
             // flush preamble
             append_text(job, &code[last_idx..idx], &font_id, egui::Color32::LIGHT_GRAY);
             
             let start = idx;
             let mut end = idx + 1;
             while let Some((i, next_c)) = chars.next() {
                 end = i + 1;
                 if next_c == '"' { break; } // TODO: Handle escaped quotes
             }
             append_text(job, &code[start..end], &font_id, egui::Color32::from_rgb(206, 145, 120)); // VSCode String Color
             last_idx = end;
             continue;
        }

        // 4. Brackets (Rainbow)
        if "()[]{}".contains(c) {
             append_text(job, &code[last_idx..idx], &font_id, egui::Color32::LIGHT_GRAY);
             
             let color_idx = if ")]}".contains(c) {
                 if bracket_depth > 0 { bracket_depth -= 1; }
                 bracket_depth
             } else {
                 let d = bracket_depth;
                 bracket_depth += 1;
                 d
             };
             
             let color = rainbow_colors[color_idx % rainbow_colors.len()];
             append_text(job, &code[idx..idx+1], &font_id, color);
             last_idx = idx + 1;
             continue;
        }

        // 5. Keywords and Identifiers (Alpha start)
        if c.is_alphabetic() || c == '_' {
            // flush preamble
             if idx > last_idx {
                 append_text(job, &code[last_idx..idx], &font_id, egui::Color32::LIGHT_GRAY);
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
                 "fn" | "let" | "mut" | "if" | "else" | "match" | "return" | 
                 "struct" | "impl" | "pub" | "use" | "mod" | "crate" | "for" | "while" | "loop" 
                 => egui::Color32::from_rgb(86, 156, 214), // Blue keyword
                 "true" | "false" => egui::Color32::from_rgb(86, 156, 214),
                 "i32" | "f32" | "f64" | "u32" | "usize" | "String" | "str" | "bool" 
                 => egui::Color32::from_rgb(78, 201, 176), // Type teal
                 _ => {
                     // Heuristic for simple function calls (followed by '(')? 
                     // Hard without lookahead of tokens, but basic colored ident is fine.
                     if word.starts_with(char::is_uppercase) {
                         egui::Color32::from_rgb(78, 201, 176) // Class/Type-like
                     } else {
                         egui::Color32::LIGHT_GRAY // Standard variable
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
                 append_text(job, &code[last_idx..idx], &font_id, egui::Color32::LIGHT_GRAY);
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
            append_text(job, &code[start..end], &font_id, egui::Color32::from_rgb(181, 206, 168)); // Light Green number
            last_idx = end;
            continue;
         }

    }
    
    // Flush remaining
    if last_idx < code.len() {
        append_text(job, &code[last_idx..], &font_id, egui::Color32::LIGHT_GRAY);
    }
}

fn append_text(job: &mut egui::text::LayoutJob, text: &str, font_id: &egui::FontId, color: egui::Color32) {
    if text.is_empty() { return; }
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
