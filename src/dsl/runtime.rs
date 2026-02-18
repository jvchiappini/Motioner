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
pub fn run_handler(shapes: &mut [Shape], handler: &DslHandler, ctx: &EvalContext) -> bool {
    let mut changed = false;
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

// ─── Action dispatcher ────────────────────────────────────────────────────────

/// Route a single action line to the appropriate executor.
///
/// **Add new actions here** following the existing pattern.
fn dispatch_action(shapes: &mut [Shape], line: &str, ctx: &EvalContext) -> Result<(), String> {
    if line.starts_with("move_element") {
        return exec_move_element(shapes, line, ctx);
    }

    Err(format!("Unknown action: '{}'", line))
}

// ─── Action executors ─────────────────────────────────────────────────────────

fn exec_move_element(shapes: &mut [Shape], line: &str, ctx: &EvalContext) -> Result<(), String> {
    let action = crate::shapes::utilities::move_element::MoveElement::parse_dsl(line)?;
    let x = evaluator::evaluate(&action.x_expr, ctx)?;
    let y = evaluator::evaluate(&action.y_expr, ctx)?;
    crate::shapes::utilities::element_modifiers::move_element(shapes, &action.name, x, y)
}
