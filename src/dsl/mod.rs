/// Motioner DSL - public module facade.
///
/// The DSL pipeline is split into focused sub-modules:
///
/// | Module          | Responsibility                                          |
/// |-----------------|---------------------------------------------------------|
/// | [`ast`]         | All AST node types (no logic, pure data)                |
/// | [`lexer`]       | Tokeniser: source text -> Vec<SpannedToken>             |
/// | [`parser`]      | Parser: source text -> Vec<Statement> + config parser   |
/// | [`validator`]   | Diagnostics: source text -> Vec<Diagnostic>             |
/// | [`generator`]   | Code-gen: scene -> DSL string + event handler extraction|
/// | [`evaluator`]   | Expression evaluator for runtime actions                |
/// | [`runtime`]     | Handler executor: runs actions against the scene        |
///
/// Callers should import from this module; the sub-module layout is an
/// implementation detail and may change.
pub mod ast;
pub mod evaluator;
pub mod generator;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod utils;
pub mod validator;

// --- Re-exports ---------------------------------------------------------------

pub use generator::{extract_event_handlers, generate, generate_from_elements};
pub use parser::{method_color, parse_config};
pub use runtime::DslHandler;
pub use validator::{validate, Diagnostic};

/// Container for transient state derived from the DSL source.
///
/// This holds the list of diagnostics produced by validation and the set of
/// event handlers that have been extracted from the current `dsl_code` text.
/// Previously these vectors lived directly on `AppState`, but they are more
/// logically part of the DSL subsystem itself, so they have been moved here.
///
/// The application state maintains a single `dsl: DslState` field; helpers in
/// `states::autosave` and `app_state` access these subfields as needed.
// Re-export the state type from the `states` module.  The concrete definition
// lives in `src/states/dslstate.rs` as that location better reflects the data's
// role as transient application state rather than part of the parsing logic.
pub use crate::states::dslstate::DslState;

// --- Legacy shims (keep existing call-sites compiling) -----------------------

use crate::scene::Shape;

/// Convenience wrapper: generate DSL from a scene.
/// Prefer calling [`generate_from_elements`] directly when the scene is stored as `ElementKeyframes`.
#[inline]
pub fn generate_dsl(scene: &[Shape], width: u32, height: u32, fps: u32, duration: f32) -> String {
    generate(scene, width, height, fps, duration)
}

/// Generate DSL directly from `ElementKeyframes` — no intermediate `Vec<Shape>` clone needed.
#[inline]
pub fn generate_dsl_from_elements(
    elements: &[crate::shapes::element_store::ElementKeyframes],
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
) -> String {
    generate_from_elements(elements, width, height, fps, duration)
}

/// Convenience wrapper: validate DSL and return diagnostics.
/// Prefer calling [`validate`] directly.
#[inline]
pub fn validate_dsl(src: &str) -> Vec<Diagnostic> {
    validate(src)
}

/// Convenience wrapper: extract event handlers from DSL source.
/// Prefer calling [`extract_event_handlers`] directly.
#[inline]
pub fn extract_event_handlers_structured(src: &str) -> Vec<DslHandler> {
    extract_event_handlers(src)
}

/// Find the byte index of the `)` matching the `(` at `open_pos` inside `s`.
/// Used by the code panel to locate function-call argument list boundaries.
pub fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
    lexer::find_matching_close(s, open_pos, '(', ')')
}

/// Parse DSL source and return a scene as a `Vec<Shape>`.
///
/// This converts the typed AST produced by [`parser::parse`] into the concrete
/// scene types used by the rest of the application.  Unknown or malformed
/// constructs are silently skipped so the editor can show a partial scene while
/// the user is still typing.
pub fn parse_dsl(src: &str) -> Vec<Shape> {
    // conversion delegated to `shapes_manager::from_dsl_statement`
    use ast::Statement;

    let stmts = parser::parse(src);
    let mut shapes: Vec<Shape> = Vec::new();
    let mut pending_moves: Vec<(String, ast::MoveBlock)> = Vec::new();

    for stmt in stmts {
        if let Statement::Shape(s) = stmt {
            shapes.push(s);
            continue;
        }

        match stmt {
            Statement::Move(mv) => {
                if let Some(el) = mv.element.clone() {
                    pending_moves.push((el, mv));
                }
            }
            // Header and event handlers are not scene shapes.
            Statement::Header(_) | Statement::EventHandler(_) => {}
            // All shape cases are handled by the Statement::Shape arm above.
            Statement::Shape(_) => unreachable!(),
        }
    }

    // Attach top-level move blocks to their target shapes. Use the
    // `Shape::push_animation` helper so this module doesn't need to match on
    // concrete shape variants.  We don't convert these to keyframes here; the
    // downstream `parse_dsl_into_elements` call will handle moves separately.
    for (target, mv) in pending_moves {
        if let Some(shape) = shapes.iter_mut().find(|s| s.name() == target) {
            let anim = ast_move_to_scene(&mv);
            shape.push_animation(anim);
        }
    }

    shapes
}

// ─── DSL → ElementKeyframes conversion ───────────────────────────────────────

/// Parse a DSL source string and return a `Vec<ElementKeyframes>` — the
/// canonical in-memory representation used by the runtime and GPU pipeline.
///
/// **Pipeline:** `code_panel → dsl → memory → GPU compute`
///
/// 1. Lex + parse the DSL.
/// 2. Each shape block → one `ElementKeyframes` with spawn-time keyframes.
/// 3. Each top-level `move {}` block → two boundary keyframes on the x/y
///    tracks (one at `start_frame`, one at `end_frame`) with the easing from
///    the DSL stored on the keyframe. The GPU compute shader reads these two
///    keyframes and interpolates every intermediate frame on the fly — no
///    per-frame positions are ever stored in RAM.
///
/// The easing value on each keyframe tells the compute shader which curve to
/// use between that keyframe and the next one:
///
/// ```text
/// x: [ {frame:0, value:0.5, easing:Linear},
///       {frame:200, value:0.2, easing:EaseIn},
///       {frame:400, value:0.5, easing:Linear} ]
/// ```
///
/// All easing maths stay in `animations::move_animation` — this function only
/// resolves positions and writes keyframes.
pub fn parse_dsl_into_elements(
    src: &str,
    fps: u32,
) -> Vec<crate::shapes::element_store::ElementKeyframes> {
    use crate::animations::move_animation::MoveAnimation;
    use crate::shapes::element_store::{seconds_to_frame, ElementKeyframes, Keyframe};
    use ast::Statement;

    let stmts = parser::parse(src);

    // ── Pass 1: shapes → ElementKeyframes ────────────────────────────────────
    let mut elements: Vec<ElementKeyframes> = stmts
        .iter()
        .filter_map(|stmt| {
            if let Statement::Shape(shape) = stmt {
                let mut ek = ElementKeyframes::from_shape_at_spawn(shape, fps)?;

                // Process inline animations already attached to the shape.  Each
                // move animation is converted immediately into two boundary
                // keyframes on the x/y tracks; the helper takes care of sampling
                // the existing tracks so sequences of animations chain correctly.
                for anim in shape.animations() {
                    if let Some(ma) = MoveAnimation::from_scene(anim) {
                        apply_move_to_ek(&mut ek, &ma, fps);
                    }
                }

                Some(ek)
            } else {
                None
            }
        })
        .collect();

    // ── Pass 2: top-level move blocks ────────────────────────────────────────
    for stmt in &stmts {
        let Statement::Move(mv) = stmt else { continue };
        let Some(target_name) = mv.element.as_deref() else {
            continue;
        };
        let Some(elem) = elements.iter_mut().find(|e| e.name == target_name) else {
            continue;
        };

        let scene_anim = ast_move_to_scene(mv);
        if let Some(ma) = MoveAnimation::from_scene(&scene_anim) {
            apply_move_to_ek(elem, &ma, fps);
        }
    }

    elements
}

/// Helper that converts a `MoveAnimation` into explicit keyframes on the
/// element's x/y tracks.  The algorithm mirrors the semantics previously
/// implemented by the GPU loop, so chained or overlapping moves behave
/// identically.
fn apply_move_to_ek(
    ek: &mut crate::shapes::element_store::ElementKeyframes,
    ma: &crate::animations::move_animation::MoveAnimation,
    fps: u32,
) {
    use crate::shapes::element_store::{seconds_to_frame, Keyframe, FrameIndex};

    let start_frame = seconds_to_frame(ma.start, fps);
    let end_frame = seconds_to_frame(ma.end, fps);

    // Determine the element's current position at the start of the move.  The
    // public `sample` helper already handles interpolation of existing tracks
    // so we can use it instead of re‑implementing that logic here.
    let (start_x, start_y) = if let Some(props) = ek.sample(start_frame, fps) {
        (props.x.unwrap_or(0.5), props.y.unwrap_or(0.5))
    } else {
        (0.5, 0.5)
    };

    // insert start keyframes; easing goes on the first keyframe so that the
    // interpolation to the subsequent frame uses the DSL-specified curve.
    // helper that either updates an existing keyframe at `frame` or pushes a
    // new one.  this prevents duplicate entries when a move starts/ends on a
    // frame that already has a keyframe (either from a previous move or a
    // manual frame inserted by the user).
    fn upsert_kf(track: &mut Vec<Keyframe<f32>>, frame: FrameIndex, value: f32, easing: crate::animations::easing::Easing) {
        if let Some(existing) = track.iter_mut().find(|kf| kf.frame == frame) {
            existing.value = value;
            existing.easing = easing;
        } else {
            track.push(Keyframe { frame, value, easing });
        }
    }

    upsert_kf(&mut ek.x, start_frame, start_x, ma.easing.clone());
    upsert_kf(&mut ek.y, start_frame, start_y, ma.easing.clone());

    // insert end keyframes (linear easing by default – easing is only used for
    // interpolation *from* this keyframe to the next one).
    upsert_kf(&mut ek.x, end_frame, ma.to_x, crate::animations::easing::Easing::Linear);
    upsert_kf(&mut ek.y, end_frame, ma.to_y, crate::animations::easing::Easing::Linear);

    // keep tracks sorted by frame for correct sampling performance
    ek.x.sort_by_key(|kf| kf.frame);
    ek.y.sort_by_key(|kf| kf.frame);
}

pub(crate) fn ast_move_to_scene(mv: &ast::MoveBlock) -> crate::scene::Animation {
    use crate::scene::Animation;

    let easing = ast_easing_to_scene(&mv.easing);
    Animation::Move {
        to_x: mv.to.0,
        to_y: mv.to.1,
        start: mv.during.0,
        end: mv.during.1,
        easing,
    }
}

fn ast_easing_to_scene(kind: &ast::EasingKind) -> crate::scene::Easing {
    use crate::scene::{BezierPoint, Easing};
    use ast::EasingKind;

    match kind {
        EasingKind::Linear => Easing::Linear,
        EasingKind::EaseIn { power } => Easing::EaseIn { power: *power },
        EasingKind::EaseOut { power } => Easing::EaseOut { power: *power },
        EasingKind::EaseInOut { power } => Easing::EaseInOut { power: *power },
        EasingKind::Sine => Easing::Sine,
        EasingKind::Expo => Easing::Expo,
        EasingKind::Circ => Easing::Circ,
        EasingKind::Bezier { p1, p2 } => Easing::Bezier { p1: *p1, p2: *p2 },
        EasingKind::Spring {
            damping,
            stiffness,
            mass,
        } => Easing::Spring {
            damping: *damping,
            stiffness: *stiffness,
            mass: *mass,
        },
        EasingKind::Elastic { amplitude, period } => Easing::Elastic {
            amplitude: *amplitude,
            period: *period,
        },
        EasingKind::Bounce { bounciness } => Easing::Bounce {
            bounciness: *bounciness,
        },
        EasingKind::Custom { points } => Easing::Custom {
            points: points.clone(),
        },
        EasingKind::CustomBezier { points } => Easing::CustomBezier {
            points: points
                .iter()
                .map(|p| BezierPoint {
                    pos: p.pos,
                    handle_left: p.handle_left,
                    handle_right: p.handle_right,
                })
                .collect(),
        },
    }
}

// ─── tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::element_store::seconds_to_frame;

    #[test]
    fn move_blocks_convert_to_keyframes() {
        let src = r#"
        rect(name="foo", x=0.0, y=0.0)
        move {
            element = "foo",
            to = (1.0, 2.0),
            during = 0.0 -> 1.0,
            ease = linear
        }
        "#;

        let fps = 30;
        let elements = parse_dsl_into_elements(src, fps);
        assert_eq!(elements.len(), 1);
        let ek = &elements[0];
        assert_eq!(ek.kind, "rect");
        assert_eq!(ek.x.len(), 2);
        assert_eq!(ek.y.len(), 2);
        assert_eq!(ek.x[0].frame, seconds_to_frame(0.0, fps));
        assert_eq!(ek.x[1].frame, seconds_to_frame(1.0, fps));
        assert_eq!(ek.x[1].value, 1.0);
        assert_eq!(ek.y[1].value, 2.0);
    }

    #[test]
    fn move_overwrites_existing_keyframe() {
        // start frame already has a manual x keyframe (0.0); the move should
        // replace that easing/value rather than append a duplicate entry.
        let src = r#"
        rect(name="foo", x=0.1, y=0.0)
        move {
            element = "foo",
            to = (1.0, 0.0),
            during = 0.0 -> 1.0,
            ease = ease_in(power = 2.0)
        }
        "#;

        let fps = 10;
        let elements = parse_dsl_into_elements(src, fps);
        let ek = &elements[0];
        assert_eq!(ek.x.len(), 2);
        // first keyframe should use the easing from the move, not Linear
        assert_eq!(ek.x[0].frame, seconds_to_frame(0.0, fps));
        assert_eq!(ek.x[0].value, 0.1);
        assert!(matches!(ek.x[0].easing, crate::animations::easing::Easing::EaseIn { .. }));
    }
}
