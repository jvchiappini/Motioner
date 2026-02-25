//! Runtime support for DSL event handlers.  Currently the parser is disabled
//! and there are no actionable statements (no `move`, no `rect`, etc.), so
//! the runtime simply provides a minimal API that does nothing.  This keeps
//! the rest of the application compiling while the full implementation is
//! removed.

use super::evaluator::EvalContext;
use crate::scene::Shape;

/// A top-level event handler extracted from DSL source.
#[derive(Clone, Debug)]
pub struct DslHandler {
    pub name: String,
    pub body: String,
    pub color: [u8; 4],
}

/// Execute all actions in `handler` against the scene.
///
/// Always returns `false` because there are no actions to execute.
pub fn run_handler(_shapes: &mut [Shape], _handler: &DslHandler, _ctx: &mut EvalContext) -> bool {
    false
}

/// Stubbed block executor; never modifies the scene.
pub fn exec_block(_shapes: &mut [Shape], _body: &str, _ctx: &mut EvalContext) -> Result<bool, String> {
    Ok(false)
}
