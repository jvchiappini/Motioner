/// AST node types for the Motioner DSL.
///
/// Every piece of DSL source is represented as a typed node in this tree.
/// The parser produces `Vec<Statement>` from raw source text; downstream
/// modules (evaluator, validator, generator) operate on these nodes rather
/// than raw strings.

// ─── Primitive value types ────────────────────────────────────────────────────

/// A 2-tuple of floats, used for coordinates and bezier control points.
pub type Point2 = (f32, f32);

/// A color expressed as `#rrggbb` (alpha always 255) or as four u8 components.
#[derive(Clone, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim_start_matches('#');
        if s.len() < 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        let a = if s.len() >= 8 {
            u8::from_str_radix(&s[6..8], 16).ok()?
        } else {
            255
        };
        Some(Self { r, g, b, a })
    }

    pub fn to_array(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

// ─── Easing ───────────────────────────────────────────────────────────────────

/// All supported easing curves for `move {}` animations.
/// Mirrors `crate::scene::Easing` but lives in the DSL layer so the AST
/// has no dependency on scene internals.
#[derive(Clone, Debug, PartialEq)]
pub enum EasingKind {
    Linear,
    EaseIn {
        power: f32,
    },
    EaseOut {
        power: f32,
    },
    EaseInOut {
        power: f32,
    },
    Sine,
    Expo,
    Circ,
    Bezier {
        p1: Point2,
        p2: Point2,
    },
    Spring {
        damping: f32,
        stiffness: f32,
        mass: f32,
    },
    Elastic {
        amplitude: f32,
        period: f32,
    },
    Bounce {
        bounciness: f32,
    },
    Custom {
        points: Vec<Point2>,
    },
    CustomBezier {
        points: Vec<BezierPoint>,
    },
}

/// A single control point used by the `CustomBezier` easing.
#[derive(Clone, Debug, PartialEq)]
pub struct BezierPoint {
    pub pos: Point2,
    pub handle_left: Point2,
    pub handle_right: Point2,
}

// ─── Text ─────────────────────────────────────────────────────────────────────

// `TextSpan` moved to `src/shapes/text.rs`; the parser now constructs
// `shapes::text::TextSpan` directly so the AST no longer defines a
// duplicate `TextSpan` type.

// ─── Animations ───────────────────────────────────────────────────────────────

/// A `move {}` animation block, either inline inside a shape or at top-level.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveBlock {
    /// When at top-level the target element must be named here.
    pub element: Option<String>,
    /// Destination coordinates in normalised canvas space (0.0 – 1.0).
    pub to: Point2,
    /// `start -> end` time range in seconds.
    pub during: (f32, f32),
    pub easing: EasingKind,
}

// ─── Shape elements ───────────────────────────────────────────────────────────

// ─── Header ───────────────────────────────────────────────────────────────────

/// The project-level header directives.
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration: f32,
}

// ─── Event handlers ───────────────────────────────────────────────────────────

/// A top-level event handler block, e.g. `on_time { move_element(...) }`.
#[derive(Clone, Debug, PartialEq)]
pub struct EventHandlerNode {
    pub event: String,
    /// Raw body text (not parsed further here; runtime executes it line by line).
    pub body: String,
    /// Display color for the editor highlighter (RGBA).
    pub color: [u8; 4],
}

// ─── Top-level statement ──────────────────────────────────────────────────────

/// Every top-level item that can appear in a Motioner DSL file.
///
/// # Adding a new shape
/// Shape variants are **not** listed individually here. Instead, any parsed
/// concrete shape is wrapped in `Statement::Shape`. This means adding a new
/// shape type requires **zero changes** to the AST or the DSL pipeline — only
/// the shape module itself needs to register a `ShapeParserFactory`.
#[derive(Clone, Debug)]
pub enum Statement {
    Header(HeaderConfig),
    /// Any concrete visual shape (Circle, Rect, Text, …).
    /// The variant is determined at parse time by the registered shape parsers.
    Shape(crate::shapes::shapes_manager::Shape),
    /// A top-level `move {}` block that references an element by name.
    Move(MoveBlock),
    EventHandler(EventHandlerNode),
}
