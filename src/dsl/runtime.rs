/// DSL runtime: executes event handler bodies against the current scene.
///
/// Event handlers are extracted from DSL source by [`crate::dsl::generator`]
/// and stored as [`DslHandler`] structs.  At each relevant moment (e.g. on
/// every frame tick) the application calls [`run_handler`] to apply the
/// handler's actions to the scene.
///
/// **Adding a new action:**  
/// 1. Implement a parser in the relevant `shapes/utilities/` module.  
/// 2. Add a `dispatch_action` branch here that calls it.
use super::evaluator::{self, EvalContext};
use crate::scene::Shape;
// shape-specific helpers are accessed via `shapes_manager`

// ─── Handler type ─────────────────────────────────────────────────────────────

/// A top-level event handler extracted from DSL source.
#[derive(Clone, Debug)]
pub struct DslHandler {
    /// Event name, e.g. `"on_time"`.
    pub name: String,
    /// Raw body text; executed line by line by [`run_handler`].
    pub body: String,
    /// Editor highlight color (RGBA).
    pub color: [u8; 4],
}

// ─── Execution ────────────────────────────────────────────────────────────────

/// Execute all actions in `handler` against the scene.
///
/// Returns `true` if at least one action modified the scene.
pub fn run_handler(shapes: &mut [Shape], handler: &DslHandler, ctx: &mut EvalContext) -> bool {
    match exec_block(shapes, &handler.body, ctx) {
        Ok(changed) => changed,
        Err(_) => false,
    }
}

/// Execute a block of DSL lines (handler body or nested block). Returns
/// Ok(true) if at least one action modified the scene.
pub fn exec_block(shapes: &mut [Shape], body: &str, ctx: &mut EvalContext) -> Result<bool, String> {
    let mut changed = false;
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0usize;

    while i < lines.len() {
        let mut line = lines[i].trim();
        i += 1;
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        // Block start? collect until matching '}' (supports nested braces).
        if line.contains('{') {
            let mut brace_count = line.chars().filter(|c| *c == '{').count() as isize
                - line.chars().filter(|c| *c == '}').count() as isize;
            let mut block_lines = vec![line.to_string()];
            while brace_count > 0 && i < lines.len() {
                let nxt = lines[i];
                i += 1;
                brace_count += nxt.chars().filter(|c| *c == '{').count() as isize;
                brace_count -= nxt.chars().filter(|c| *c == '}').count() as isize;
                block_lines.push(nxt.to_string());
            }
            let block_text = block_lines.join("\n");
            if let Ok(modified) = dispatch_action(shapes, &block_text, ctx) {
                if modified {
                    changed = true;
                }
            }
            continue;
        }

        if let Ok(modified) = dispatch_action(shapes, line, ctx) {
            if modified {
                changed = true;
            }
        }
    }

    Ok(changed)
}

// ─── Action dispatcher ────────────────────────────────────────────────────────

/// Route a single action line to the appropriate executor.
///
/// **Add new actions here** following the existing pattern.
fn dispatch_action(
    shapes: &mut [Shape],
    line: &str,
    ctx: &mut EvalContext,
) -> Result<bool, String> {
    // variable declarations / assignments: `let` or `set`
    if line.starts_with("let ") || line.starts_with("set ") {
        // determine RHS type to delegate to correct logic file
        if let Some(eq) = line.find('=') {
            let rhs = line[eq + 1..].trim();
            if rhs.starts_with('"') {
                return crate::logics::string_logic::exec(shapes, line, ctx);
            }
            if rhs.starts_with('[') {
                return crate::logics::list_logic::exec(shapes, line, ctx);
            }
            // otherwise numeric expression
            return crate::logics::number_logic::exec(shapes, line, ctx);
        }
    }

    // for-loops
    if line.trim_start().starts_with("for ") {
        return crate::logics::for_logic::exec(shapes, line, ctx);
    }

    // if / if not
    if line.trim_start().starts_with("if ") || line.trim_start().starts_with("if not ") {
        return crate::logics::if_logic::exec(shapes, line, ctx);
    }

    if line.starts_with("move_element") {
        return exec_move_element(shapes, line, ctx);
    }

    // Allow handler bodies to declare full shape blocks (treated as
    // runtime-spawned/ephemeral shapes). Example: `circle "S" { ... }`.
    if line.trim_start().starts_with("circle")
        || line.trim_start().starts_with("rect")
        || line.trim_start().starts_with("text")
    {
        // Parse the provided block into scene shapes (to obtain animations
        // and defaults), but also re-evaluate any KV expressions inside the
        // handler context (e.g. `x = seconds * 0.1`). This lets users write
        // `circle "C" { x = seconds * 0.1, ... }` inside `on_time`.
        let mut parsed = crate::dsl::parse_dsl(line);

        // Helper: split top-level KV lines from the block body while
        // preserving nested blocks (like `move { ... }`).
        fn top_level_lines(body: &str) -> Vec<String> {
            let mut out = Vec::new();
            let mut depth: i32 = 0;
            let mut cur = String::new();
            for ch in body.chars() {
                if ch == '{' {
                    depth += 1;
                    cur.push(ch);
                    continue;
                }
                if ch == '}' {
                    depth -= 1;
                    cur.push(ch);
                    continue;
                }
                if ch == '\n' && depth == 0 {
                    if !cur.trim().is_empty() {
                        out.push(cur.trim().to_string());
                    }
                    cur.clear();
                    continue;
                }
                cur.push(ch);
            }
            if !cur.trim().is_empty() {
                out.push(cur.trim().to_string());
            }
            out
        }

        // Extract raw inner body (between first '{' and last '}')
        let inner = if let Some(start) = line.find('{') {
            if let Some(end) = line.rfind('}') {
                &line[start + 1..end]
            } else {
                ""
            }
        } else {
            ""
        };

        let raw_lines = top_level_lines(inner);

        // Split a top-level statement into comma-separated KV fragments
        // while ignoring commas inside nested parentheses/brackets/braces.
        fn split_top_level_kvs(s: &str) -> Vec<String> {
            let mut out = Vec::new();
            let mut cur = String::new();
            let mut depth = 0i32;
            for ch in s.chars() {
                match ch {
                    '(' | '{' | '[' => {
                        depth += 1;
                        cur.push(ch);
                    }
                    ')' | '}' | ']' => {
                        depth = (depth - 1).max(0);
                        cur.push(ch);
                    }
                    ',' if depth == 0 => {
                        if !cur.trim().is_empty() {
                            out.push(cur.trim().to_string());
                        }
                        cur.clear();
                    }
                    _ => cur.push(ch),
                }
            }
            if !cur.trim().is_empty() {
                out.push(cur.trim().to_string());
            }
            out
        }

        fn split_kv(s: &str) -> Option<(String, String)> {
            if let Some(eq) = s.find('=') {
                let key = s[..eq].trim().to_string();
                let mut val = s[eq + 1..].trim().to_string();
                // strip trailing comma(s)
                while val.ends_with(',') {
                    val.pop();
                    val = val.trim_end().to_string();
                }
                return if key.is_empty() {
                    None
                } else {
                    Some((key, val))
                };
            }
            None
        }

        // Convert the first parsed shape (if any) or create a default one
        let mut created_shapes: Vec<crate::scene::Shape> = Vec::new();
        if parsed.is_empty() {
            // fallback: instantiate a default by keyword. Delegate to
            // `shapes_manager` so the runtime doesn't match on variants.
            let kw = line.trim_start().split_whitespace().next().unwrap_or("");
            if let Some(s) = crate::shapes::shapes_manager::create_default_by_keyword(kw, "Spawned".into()) {
                created_shapes.push(s);
            }
        } else {
            created_shapes = parsed;
        }

        // For each created shape, override numeric/string props by evaluating
        // any top-level KV expressions found in the handler block.
        for mut s in created_shapes {
            // apply raw KV entries (delegate type-specific updates to
            // `Shape` helpers so this module remains shape-agnostic).
            for raw in &raw_lines {
                // skip nested blocks like `move { ... }`
                if raw.contains('{') {
                    continue;
                }
                // split comma-separated KV fragments on the top-level
                for frag in split_top_level_kvs(raw) {
                    if let Some((key, val)) = split_kv(&frag) {
                        // numeric properties
                        match key.as_str() {
                            "x" | "y" | "radius" | "width" | "w" | "height" | "h" | "size" | "spawn" | "kill" => {
                                let num = evaluator::evaluate(&val, ctx)?;
                                s.apply_kv_number(&key, num);
                                continue;
                            }
                            _ => {}
                        }

                        // color (hex string)
                        if key == "fill" {
                            let sstr = val.trim().trim_matches('"');
                            if let Some(col) = crate::code_panel::utils::parse_hex(sstr) {
                                s.set_fill_color(col);
                            }
                            continue;
                        }

                        // fallback: string properties (name, font, value, ...)
                        let sstr = val.trim().trim_matches('"');
                        s.apply_kv_string(&key, sstr);
                    }
                }
            }

            // mark ephemeral and queue for appending to scene
            s.set_ephemeral(true);
            ctx.push_spawned_shape(s);
        }

        return Ok(true);
    }

    Err(format!("Unknown action: '{}'", line))
}

// ─── Action executors ─────────────────────────────────────────────────────────

fn exec_move_element(shapes: &mut [Shape], line: &str, ctx: &EvalContext) -> Result<bool, String> {
    let action = crate::shapes::utilities::move_element::MoveElement::parse_dsl(line)?;
    // allow `name` to be either a quoted literal or a variable name defined in ctx
    let target_name = if let Some(s) = ctx.get_str(&action.name) {
        s.to_string()
    } else {
        action.name.clone()
    };

    let x = evaluator::evaluate(&action.x_expr, ctx)?;
    let y = evaluator::evaluate(&action.y_expr, ctx)?;
    crate::shapes::utilities::move_element::move_element(shapes, &target_name, x, y)?;
    Ok(true)
}
