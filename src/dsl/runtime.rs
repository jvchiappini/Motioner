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
use crate::shapes::ShapeDescriptor;
// shape-specific helpers are accessed via `shapes_manager`
use crate::dsl::utils;

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
        let line = lines[i].trim();
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

// dispatch_action was accidentally merged into exec_block during earlier
// refactoring; restore it here as its own helper.  The implementation is
// essentially the original body from before the utils extraction, but it
// entrusts low-level helpers to `dsl::utils` where appropriate.

fn dispatch_action(
    shapes: &mut [Shape],
    line: &str,
    ctx: &mut EvalContext,
) -> Result<bool, String> {
    // 'for' loops are handled specially; fall through to other checks if the
    // line is not a loop header.
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
    // The check is driven by the registry — no hard-coded keyword list.
    let first_word = line.trim_start().split_whitespace().next().unwrap_or("");
    if crate::shapes::shapes_manager::create_default_by_keyword(first_word, String::new()).is_some()
        || crate::shapes::shapes_manager::parse_shape_block(&[line
            .split('{')
            .next()
            .unwrap_or("")
            .trim()
            .to_string()])
        .is_some()
        || crate::shapes::shapes_manager::registered_shape_keywords().contains(&first_word)
    {
        // Parse the provided block into scene shapes (to obtain animations
        // and defaults), but also re-evaluate any KV expressions inside the
        // handler context (e.g. `x = seconds * 0.1`). This lets users write
        // `circle "C" { x = seconds * 0.1, ... }` inside `on_time`.
        let parsed = crate::dsl::parse_dsl(line);

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

        let raw_lines = utils::top_level_lines(inner);

        // Convert the first parsed shape (if any) or create a default one
        let mut created_shapes: Vec<crate::scene::Shape> = Vec::new();
        if parsed.is_empty() {
            // fallback: instantiate a default by keyword. Delegate to
            // `shapes_manager` so the runtime doesn't match on variants.
            let kw = line.trim_start().split_whitespace().next().unwrap_or("");
            if let Some(s) =
                crate::shapes::shapes_manager::create_default_by_keyword(kw, "Spawned".into())
            {
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
                for frag in utils::split_top_level_kvs(raw) {
                    if let Some((key, val)) = utils::split_kv(&frag) {
                        // numeric properties
                        match key.as_str() {
                            "x" | "y" | "radius" | "width" | "w" | "height" | "h" | "size"
                            | "spawn" | "kill" => {
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
