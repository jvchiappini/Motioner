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
pub mod validator;

// --- Re-exports ---------------------------------------------------------------

pub use generator::{extract_event_handlers, generate};
pub use parser::{method_color, parse_config};
pub use runtime::DslHandler;
pub use validator::{validate, Diagnostic};

// --- Legacy shims (keep existing call-sites compiling) -----------------------

use crate::scene::Shape;

/// Convenience wrapper: generate DSL from a scene.
/// Prefer calling [`generate`] directly.
#[inline]
pub fn generate_dsl(scene: &[Shape], width: u32, height: u32, fps: u32, duration: f32) -> String {
    generate(scene, width, height, fps, duration)
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
    use crate::shapes::text::TextSpan as SceneTextSpan;
    use crate::shapes::{circle::Circle, rect::Rect, text::Text};
    use ast::Statement;

    let stmts = parser::parse(src);
    let mut shapes: Vec<Shape> = Vec::new();
    let mut pending_moves: Vec<(String, ast::MoveBlock)> = Vec::new();

    for stmt in stmts {
        match stmt {
            Statement::Circle(n) => {
                let mut c = Circle::default();
                c.name = n.name;
                c.x = n.x;
                c.y = n.y;
                c.radius = n.radius;
                c.spawn_time = n.spawn;
                c.kill_time = n.kill;
                c.z_index = n.z_index;
                if let Some(col) = n.fill {
                    c.color = col.to_array();
                }
                for mv in n.animations {
                    c.animations.push(ast_move_to_scene(&mv));
                }
                shapes.push(Shape::Circle(c));
            }
            Statement::Rect(n) => {
                let mut r = Rect::default();
                r.name = n.name;
                r.x = n.x;
                r.y = n.y;
                r.w = n.w;
                r.h = n.h;
                r.spawn_time = n.spawn;
                r.kill_time = n.kill;
                r.z_index = n.z_index;
                if let Some(col) = n.fill {
                    r.color = col.to_array();
                }
                for mv in n.animations {
                    r.animations.push(ast_move_to_scene(&mv));
                }
                shapes.push(Shape::Rect(r));
            }
            Statement::Text(n) => {
                let mut t = Text::default();
                t.name = n.name;
                t.x = n.x;
                t.y = n.y;
                t.size = n.size;
                t.font = n.font;
                t.value = n.value;
                t.spawn_time = n.spawn;
                t.kill_time = n.kill;
                t.z_index = n.z_index;
                if let Some(col) = n.fill {
                    t.color = col.to_array();
                }
                t.spans = n
                    .spans
                    .into_iter()
                    .map(|sp| SceneTextSpan {
                        text: sp.text,
                        font: sp.font,
                        size: sp.size,
                        color: sp.color.to_array(),
                    })
                    .collect();
                for mv in n.animations {
                    t.animations.push(ast_move_to_scene(&mv));
                }
                shapes.push(Shape::Text(t));
            }
            Statement::Move(mv) => {
                if let Some(el) = mv.element.clone() {
                    pending_moves.push((el, mv));
                }
            }
            // Header and event handlers are not scene shapes.
            Statement::Header(_) | Statement::EventHandler(_) => {}
        }
    }

    // Attach top-level move blocks to their target shapes.
    for (target, mv) in pending_moves {
        if let Some(shape) = shapes.iter_mut().find(|s| s.name() == target) {
            let anim = ast_move_to_scene(&mv);
            match shape {
                Shape::Circle(c) => c.animations.push(anim),
                Shape::Rect(r) => r.animations.push(anim),
                Shape::Text(t) => t.animations.push(anim),
                _ => {}
            }
        }
    }

    shapes
}

// ─── AST → scene conversion helpers ─────────────────────────────────────────

fn ast_move_to_scene(mv: &ast::MoveBlock) -> crate::scene::Animation {
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
