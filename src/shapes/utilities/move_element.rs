use crate::dsl::evaluator::EvalContext;
use crate::scene::Shape;
use crate::shapes::ShapeDescriptor;

/// Apply an evaluated x/y to the named element in `shapes`.
/// Kept here so the DSL `MoveElement` and runtime can call a single
/// implementation — this replaces the old `element_modifiers::move_element`.
pub fn move_element(shapes: &mut [Shape], name: &str, x: f32, y: f32) -> Result<(), String> {
    let mut found = false;
    for sh in shapes.iter_mut() {
        if sh.name() == name {
            sh.apply_kv_number("x", x);
            sh.apply_kv_number("y", y);
            found = true;
        }
    }

    if found {
        Ok(())
    } else {
        Err(format!("Element '{}' not found", name))
    }
}

/// Representation of a `move_element(...)` DSL action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveElement {
    pub name: String,
    /// X/Y are stored as expressions (not yet evaluated) so they can contain `seconds`, etc.
    pub x_expr: String,
    pub y_expr: String,
    /// Display color for this action (RGBA). Not currently serialized in DSL string.
    pub color: [u8; 4],
}

impl MoveElement {
    /// Parse a `move_element(...)` action and return a `MoveElement` struct.
    /// Accepts the full call text (e.g. `move_element(name = "C", x = seconds * 0.1, y = 0.25)`).
    pub fn parse_dsl(s: &str) -> Result<MoveElement, String> {
        let start = s.find('(').ok_or("move_element: missing '('")?;
        let end = s.rfind(')').ok_or("move_element: missing ')'")?;
        let inner = &s[start + 1..end];

        let mut name_target: Option<String> = None;
        let mut x_expr: Option<String> = None;
        let mut y_expr: Option<String> = None;
        let mut color_val: Option<[u8; 4]> = None;

        for part in inner.split(',') {
            let p = part.trim();
            if let Some(eq) = p.find('=') {
                let key = p[..eq].trim();
                let val = p[eq + 1..].trim();
                match key {
                    "name" => name_target = Some(val.trim_matches('"').to_string()),
                    "x" => x_expr = Some(val.to_string()),
                    "y" => y_expr = Some(val.to_string()),
                    "color" => {
                        // accept strings like "#RRGGBB" or "#RRGGBBAA"
                        let vs = val.trim().trim_matches('"');
                        if let Some(c) = crate::code_panel::utils::parse_hex(vs) {
                            color_val = Some(c);
                        }
                    }
                    _ => {}
                }
            }
        }

        let name = name_target.ok_or("move_element: missing 'name'")?;
        let x_expr = x_expr.ok_or("move_element: missing 'x'")?;
        let y_expr = y_expr.ok_or("move_element: missing 'y'")?;

        // Default color matches "object teal" used by the code panel highlighter
        let color = color_val.unwrap_or([78, 201, 176, 255]);
        Ok(MoveElement {
            name,
            x_expr,
            y_expr,
            color,
        })
    }
}
