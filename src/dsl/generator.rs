/// DSL code generator: converts a scene back into DSL source text.
use crate::scene::Shape;

/// Generate DSL directly from `Shape` objects.
pub fn generate_dsl_from_elements(
    elements: &[Shape],
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
) -> String {
    let mut out = String::new();

    // Header
    out.push_str(&format!(
        "size({}, {})\ntimeline(fps = {}, duration = {:.2})\n\n",
        width, height, fps, duration
    ));

    // Shape definitions
    for shape in elements {
        out.push_str(&shape.to_dsl(""));
        out.push('\n');
    }

    out
}

/// Convert leading groups of 4 spaces into tab characters for every line.
pub fn normalize_tabs(src: &str) -> String {
    let mut out = String::with_capacity(src.len());

    for segment in src.split_inclusive('\n') {
        if segment == "\n" {
            out.push('\n');
            continue;
        }

        let has_newline = segment.ends_with('\n');
        let line = if has_newline {
            &segment[..segment.len() - 1]
        } else {
            segment
        };

        let mut i = 0usize;
        let bytes = line.as_bytes();
        let mut leading = String::new();
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == '\t' {
                leading.push('\t');
                i += 1;
            } else if c == ' ' {
                let mut count = 0usize;
                while i + count < bytes.len() && bytes[i + count] == b' ' {
                    count += 1;
                }
                let tabs = count / 4;
                let rem = count % 4;
                for _ in 0..tabs {
                    leading.push('\t');
                }
                for _ in 0..rem {
                    leading.push(' ');
                }
                i += count;
            } else {
                break;
            }
        }
        out.push_str(&leading);
        out.push_str(&line[i..]);
        if has_newline {
            out.push('\n');
        }
    }

    out
}
