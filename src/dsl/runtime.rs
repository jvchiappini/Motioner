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
    crate::shapes::utilities::element_modifiers::move_element(shapes, &target_name, x, y)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::evaluator::EvalContext;

    #[test]
    fn dsl_numeric_variable_and_move() {
        let mut shapes = vec![crate::scene::Shape::Circle(
            crate::shapes::circle::Circle::default(),
        )];

        let mut ctx = EvalContext::new().with_var("seconds", 2.0);

        let handler = DslHandler {
            name: "on_time".to_string(),
            body: "let a = seconds * 0.1\nlet id = \"Circle\"\nmove_element(name = id, x = a, y = 0.25)".to_string(),
            color: [0, 0, 0, 0],
        };

        assert!(run_handler(&mut shapes, &handler, &mut ctx));

        let found = shapes.iter().find(|s| s.name() == "Circle").unwrap();
        match found {
            crate::scene::Shape::Circle(c) => {
                assert!((c.x - 0.2).abs() < 1e-3);
                assert!((c.y - 0.25).abs() < 1e-3);
            }
            _ => panic!("expected circle"),
        }
    }

    #[test]
    fn dsl_string_and_list_and_for_set() {
        let mut shapes = vec![crate::scene::Shape::Circle(
            crate::shapes::circle::Circle::default(),
        )];

        let mut ctx = EvalContext::new();

        let handler = DslHandler {
            name: "on_time".to_string(),
            body: r#"
let nums = [0.1, 0.2, 0.3]
let total = 0.0
for v in nums {
    set total = total + v
}
"#
            .to_string(),
            color: [0, 0, 0, 0],
        };

        // run handler — it shouldn't modify shapes, but should update ctx
        assert!(!run_handler(&mut shapes, &handler, &mut ctx));

        // total should be 0.6 (sum of list)
        let total = ctx.get_number("total").unwrap();
        assert!((total - 0.6).abs() < 1e-6);
    }

    #[test]
    fn dsl_if_not_executes_when_false() {
        let mut shapes = vec![crate::scene::Shape::Circle(
            crate::shapes::circle::Circle::default(),
        )];
        let mut ctx = EvalContext::new().with_var("seconds", 0.0);

        let handler = DslHandler {
            name: "on_time".to_string(),
            body: "if not seconds { let flag = 1.0 }".to_string(),
            color: [0, 0, 0, 0],
        };

        assert!(!run_handler(&mut shapes, &handler, &mut ctx));
        let f = ctx.get_number("flag").unwrap();
        assert!((f - 1.0).abs() < 1e-6);
    }
}
