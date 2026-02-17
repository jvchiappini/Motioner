use crate::app_state::AppState;
use crate::dsl::evaluator::EvalContext;
use crate::dsl::runtime::{self, DslHandler};

/// Event emitted when the current playhead time changes.
/// Carries `seconds` (project time in seconds) and `frame` (rounded frame index).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeChangedEvent {
    pub seconds: f32,
    pub frame: u32,
}

impl TimeChangedEvent {
    /// Convenience handler called by the UI/timeline when time changes.
    pub fn on_time_changed(state: &mut AppState, seconds: f32, frame: u32) {
        state.last_time_changed = Some((seconds, frame));
        // Immediately dispatch any registered `on_time` DSL handlers so tests
        // and callers that directly call `on_time_changed` observe the
        // side-effects on `state.scene` synchronously.
        let _ = apply_on_time_handlers(&mut state.scene, &state.dsl_event_handlers, seconds, frame);
    }
}

/// Dispatches all registered DSL event handlers that match "on_time".
pub fn apply_on_time_handlers(
    scene: &mut [crate::scene::Shape],
    handlers: &[DslHandler],
    seconds: f32,
    frame: u32,
) -> bool {
    let mut changed = false;

    let ctx = EvalContext::new()
        .with_var("seconds", seconds)
        .with_var("frame", frame as f32);

    for handler in handlers {
        #[allow(clippy::collapsible_if)]
        if handler.name == "on_time" || handler.name == "time_changed" {
            if runtime::run_handler(scene, handler, &ctx) {
                changed = true;
            }
        }
    }
    changed
}

#[cfg(test)]
mod handler_tests {
    use super::*;
    use crate::app_state::AppState;

    #[test]
    fn dsl_move_element_executes() {
        let mut state = AppState::default();
        // ensure sample scene has a Circle named "Circle"
        assert!(state.scene.iter().any(|s| s.name() == "Circle"));

        // register a simple on_time handler that moves the Circle depending on seconds
        state.dsl_event_handlers = vec![DslHandler {
            name: "on_time".to_string(),
            body: "move_element(name = \"Circle\", x = seconds * 0.1, y = 0.25)".to_string(),
            color: [78, 201, 176, 255],
        }];

        // call event with seconds = 2.0 → x should become 0.2
        TimeChangedEvent::on_time_changed(&mut state, 2.0, 0);

        let found = state.scene.iter().find(|s| s.name() == "Circle").unwrap();
        match found {
            crate::scene::Shape::Circle { x, y, .. } => {
                assert!(((*x) - 0.2).abs() < 1e-3);
                assert!(((*y) - 0.25).abs() < 1e-3);
            }
            _ => panic!("expected circle"),
        }
    }
}
