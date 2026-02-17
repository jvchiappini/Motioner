use super::evaluator::{self, EvalContext};
use crate::scene::Shape;

/// Structured representation of an event handler extracted from DSL code.
#[derive(Clone, Debug)]
pub struct DslHandler {
    pub name: String,
    pub body: String,
}

/// Dispatches actions for a given event.
/// This is the central point to extend the DSL with new actions or triggers.
pub fn run_handler(shapes: &mut [Shape], handler: &DslHandler, ctx: &EvalContext) -> bool {
    let mut changed = false;

    // Split body into lines and process each as an action
    for line in handler.body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if dispatch_action(shapes, line, ctx).is_ok() {
            changed = true;
        }
    }

    changed
}

/// Recognizes and executes individual actions within a handler block.
/// Add new commands here (e.g., scale_element, set_color, etc.)
fn dispatch_action(shapes: &mut [Shape], line: &str, ctx: &EvalContext) -> Result<(), String> {
    if line.starts_with("move_element") {
        return exec_move_element(shapes, line, ctx);
    }

    // Future actions can be added here easily:
    // if line.starts_with("set_opacity") { ... }

    Err(format!("Unknown action: {}", line))
}

fn exec_move_element(shapes: &mut [Shape], line: &str, ctx: &EvalContext) -> Result<(), String> {
    // Parse the DSL action using the centralized `MoveElement` parser,
    // then evaluate expressions and delegate mutation to the element_modifiers manager.
    let action = crate::shapes::utilities::move_element::MoveElement::parse_dsl(line)?;
    let xv = evaluator::evaluate(&action.x_expr, ctx)?;
    let yv = evaluator::evaluate(&action.y_expr, ctx)?;
    crate::shapes::utilities::element_modifiers::move_element(shapes, &action.name, xv, yv)
}
