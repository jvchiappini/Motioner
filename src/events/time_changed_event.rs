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

    let mut ctx = EvalContext::new()
        .with_var("seconds", seconds)
        .with_var("frame", frame as f32);

    // 1) materialize shapes + keep original sampled props for diffing
    let mut originals: Vec<Option<FrameProps>> = Vec::new();
    let mut shapes: Vec<crate::scene::Shape> = Vec::new();
    for elem in elements_vec.iter() {
        let frame_idx = crate::shapes::element_store::seconds_to_frame(seconds, fps);
        originals.push(elem.sample(frame_idx, fps));
        if let Some(s) = elem.to_shape_at_frame(frame_idx, fps) {
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
            let frame_idx = crate::shapes::element_store::seconds_to_frame(seconds, fps);
            let orig = originals.get(i).and_then(|o| o.clone());
            // extract current props from shape (delegate per-shape logic into `Shape`)
            let new_props = shape.changed_frame_props(orig.as_ref());

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
            if let Some(ek) =
                crate::shapes::element_store::ElementKeyframes::from_shape_at_spawn(&s, fps)
            {
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

    let mut ctx = EvalContext::new()
        .with_var("seconds", seconds)
        .with_var("frame", frame as f32);

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
