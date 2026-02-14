use eframe::egui;

pub fn show(ui: &mut egui::Ui, code: &str) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut job = egui::text::LayoutJob::default();
        highlight_code(&mut job, code);
        ui.label(job);
    });
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
