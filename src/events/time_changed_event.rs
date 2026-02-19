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
        // side-effects on `state.scene` synchronously. Also collect any
        // shapes spawned by handlers and append them to the live scene.
        let _ = apply_on_time_handlers_collect_spawns_elements(
            &mut state.scene,
            &state.dsl_event_handlers,
            seconds,
            frame,
            state.fps,
        );
    }
}

/// ElementKeyframes-aware variant of `apply_on_time_handlers_collect_spawns`.
///
/// - Materializes each `ElementKeyframes` as a `Shape` sampled at `seconds`.
/// - Runs DSL handlers (which operate on `Shape` instances).
/// - Propagates any property changes back into the originating
///   `ElementKeyframes` by inserting hold keyframes at the current frame.
/// - Converts any spawned `Shape` instances into `ElementKeyframes` and
///   appends them to `elements_vec` (using `fps` to compute spawn frames).
pub fn apply_on_time_handlers_collect_spawns_elements(
    elements_vec: &mut Vec<crate::shapes::element_store::ElementKeyframes>,
    handlers: &[DslHandler],
    seconds: f32,
    frame: u32,
    fps: u32,
) -> bool {
    use crate::shapes::element_store::FrameProps;

    let mut changed = false;

    let mut ctx = EvalContext::new().with_var("seconds", seconds).with_var("frame", frame as f32);

    // 1) materialize shapes + keep original sampled props for diffing
    let mut originals: Vec<Option<FrameProps>> = Vec::new();
    let mut shapes: Vec<crate::scene::Shape> = Vec::new();
    for elem in elements_vec.iter() {
        let frame_idx = crate::shapes::element_store::seconds_to_frame(seconds, elem.fps);
        originals.push(elem.sample(frame_idx));
        if let Some(s) = elem.to_shape_at_frame(frame_idx) {
            shapes.push(s);
        }
    }

    // 2) run handlers on the working copy of shapes
    for handler in handlers {
        if handler.name == "on_time" || handler.name == "time_changed" {
            if runtime::run_handler(&mut shapes[..], handler, &mut ctx) {
                changed = true;
            }
        }
    }

    // 3) propagate changes back into ElementKeyframes by inserting hold keyframes
    for (i, shape) in shapes.iter().enumerate() {
        // find matching element by name
        let maybe_elem = elements_vec.iter_mut().find(|e| e.name == shape.name());
        if let Some(elem) = maybe_elem {
            let frame_idx = crate::shapes::element_store::seconds_to_frame(seconds, elem.fps);
            let orig = originals.get(i).and_then(|o| o.clone());
            // extract current props from shape
            let mut new_props = FrameProps {
                x: None,
                y: None,
                radius: None,
                w: None,
                h: None,
                size: None,
                value: None,
                color: None,
                visible: None,
                z_index: None,
            };
            match shape {
                crate::scene::Shape::Circle(c) => {
                    if orig.as_ref().and_then(|p| p.x).unwrap_or(f32::NAN) != c.x {
                        new_props.x = Some(c.x);
                    }
                    if orig.as_ref().and_then(|p| p.y).unwrap_or(f32::NAN) != c.y {
                        new_props.y = Some(c.y);
                    }
                    if orig.as_ref().and_then(|p| p.radius).unwrap_or(f32::NAN) != c.radius {
                        new_props.radius = Some(c.radius);
                    }
                    if orig.as_ref().and_then(|p| p.color) != Some(c.color) {
                        new_props.color = Some(c.color);
                    }
                    if orig.as_ref().and_then(|p| p.visible) != Some(c.visible) {
                        new_props.visible = Some(c.visible);
                    }
                }
                crate::scene::Shape::Rect(r) => {
                    if orig.as_ref().and_then(|p| p.x).unwrap_or(f32::NAN) != r.x {
                        new_props.x = Some(r.x);
                    }
                    if orig.as_ref().and_then(|p| p.y).unwrap_or(f32::NAN) != r.y {
                        new_props.y = Some(r.y);
                    }
                    if orig.as_ref().and_then(|p| p.w).unwrap_or(f32::NAN) != r.w {
                        new_props.w = Some(r.w);
                    }
                    if orig.as_ref().and_then(|p| p.h).unwrap_or(f32::NAN) != r.h {
                        new_props.h = Some(r.h);
                    }
                    if orig.as_ref().and_then(|p| p.color) != Some(r.color) {
                        new_props.color = Some(r.color);
                    }
                    if orig.as_ref().and_then(|p| p.visible) != Some(r.visible) {
                        new_props.visible = Some(r.visible);
                    }
                }
                crate::scene::Shape::Text(t) => {
                    if orig.as_ref().and_then(|p| p.x).unwrap_or(f32::NAN) != t.x {
                        new_props.x = Some(t.x);
                    }
                    if orig.as_ref().and_then(|p| p.y).unwrap_or(f32::NAN) != t.y {
                        new_props.y = Some(t.y);
                    }
                    if orig.as_ref().and_then(|p| p.size).unwrap_or(f32::NAN) != t.size {
                        new_props.size = Some(t.size);
                    }
                    if orig.as_ref().and_then(|p| p.value.clone()) != Some(t.value.clone()) {
                        new_props.value = Some(t.value.clone());
                    }
                    if orig.as_ref().and_then(|p| p.color) != Some(t.color) {
                        new_props.color = Some(t.color);
                    }
                    if orig.as_ref().and_then(|p| p.visible) != Some(t.visible) {
                        new_props.visible = Some(t.visible);
                    }
                }
                _ => {}
            }

            // if any field changed, insert a hold keyframe at current frame
            let any = new_props.x.is_some()
                || new_props.y.is_some()
                || new_props.radius.is_some()
                || new_props.w.is_some()
                || new_props.h.is_some()
                || new_props.size.is_some()
                || new_props.value.is_some()
                || new_props.color.is_some()
                || new_props.visible.is_some()
                || new_props.z_index.is_some();

            if any {
                elem.insert_frame(frame as usize, new_props);
                changed = true;
            }
        }
    }

    // 4) append spawned shapes (convert to ElementKeyframes using provided fps)
    let spawned = ctx.take_spawned_shapes();
    if !spawned.is_empty() {
        for s in spawned {
            if let Some(ek) = crate::shapes::element_store::ElementKeyframes::from_shape_at_spawn(&s, fps) {
                elements_vec.push(ek);
            }
        }
        changed = true;
    }

    changed
}

/// Dispatches all registered DSL event handlers that match "on_time".
pub fn apply_on_time_handlers(
    scene: &mut [crate::scene::Shape],
    handlers: &[DslHandler],
    seconds: f32,
    frame: u32,
) -> bool {
    let mut changed = false;

    let mut ctx = EvalContext::new()
        .with_var("seconds", seconds)
        .with_var("frame", frame as f32);

    for handler in handlers {
        #[allow(clippy::collapsible_if)]
        if handler.name == "on_time" || handler.name == "time_changed" {
            if runtime::run_handler(scene, handler, &mut ctx) {
                changed = true;
            }
        }
    }
    changed
}

/// Same as `apply_on_time_handlers` but collects any shapes queued by
/// handler execution (via `spawn_*` actions) and appends them to the
/// provided `scene_vec` (useful when `scene_vec` is the real application
/// scene and new runtime-created elements must become visible in the UI).
pub fn apply_on_time_handlers_collect_spawns(
    scene_vec: &mut Vec<crate::scene::Shape>,
    handlers: &[DslHandler],
    seconds: f32,
    frame: u32,
) -> bool {
    let mut changed = false;

    let mut ctx = EvalContext::new().with_var("seconds", seconds).with_var("frame", frame as f32);

    for handler in handlers {
        if handler.name == "on_time" || handler.name == "time_changed" {
            if runtime::run_handler(&mut scene_vec[..], handler, &mut ctx) {
                changed = true;
            }
        }
    }

    // Append any spawned shapes requested by handlers.
    let spawned = ctx.take_spawned_shapes();
    if !spawned.is_empty() {
        for s in spawned {
            scene_vec.push(s);
        }
        changed = true;
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
        assert!(state.scene.iter().any(|e| e.name == "Circle"));

        // register a simple on_time handler that moves the Circle depending on seconds
        state.dsl_event_handlers = vec![DslHandler {
            name: "on_time".to_string(),
            body: "move_element(name = \"Circle\", x = seconds * 0.1, y = 0.25)".to_string(),
            color: [78, 201, 176, 255],
        }];

        // call event with seconds = 2.0 → x should become 0.2
        TimeChangedEvent::on_time_changed(&mut state, 2.0, 0);

        let found = state.scene.iter().find(|e| e.name == "Circle").unwrap();
        let frame = crate::shapes::element_store::seconds_to_frame(2.0, found.fps);
        let props = found.sample(frame).unwrap();
        assert!((props.x.unwrap() - 0.2).abs() < 1e-3);
        assert!((props.y.unwrap() - 0.25).abs() < 1e-3);
    }

    #[test]
    fn spawn_circle_via_handler_is_appended() {
        let mut scene: Vec<crate::scene::Shape> = Vec::new();

        let handlers = vec![DslHandler {
            name: "on_time".to_string(),
            body: "circle \"S1\" { x = seconds * 0.1, y = 0.25, radius = 0.05, fill = \"#78c8ff\", spawn = seconds, kill = seconds + 1.0 }".to_string(),
            color: [0, 0, 0, 0],
        }];

        let changed = apply_on_time_handlers_collect_spawns(&mut scene, &handlers, 2.0, 0);
        assert!(changed);
        assert_eq!(scene.len(), 1);
        match &scene[0] {
            crate::scene::Shape::Circle(c) => {
                assert_eq!(c.name, "S1");
                assert!((c.x - 0.2).abs() < 1e-6);
                assert!((c.y - 0.25).abs() < 1e-6);
                assert!((c.radius - 0.05).abs() < 1e-6);
                assert_eq!(c.ephemeral, true);
                assert_eq!(c.kill_time, Some(3.0));
            }
            _ => panic!("expected circle"),
        }
    }
}
