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

/// One styled run of text inside a `text {}` element.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub font: String,
    pub size: f32,
    pub color: Color,
}

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

/// Properties that can appear inside `circle {}`.
#[derive(Clone, Debug, PartialEq)]
pub struct CircleNode {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub fill: Option<Color>,
    pub spawn: f32,
    pub z_index: i32,
    pub animations: Vec<MoveBlock>,
}

/// Properties that can appear inside `rect {}`.
#[derive(Clone, Debug, PartialEq)]
pub struct RectNode {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub fill: Option<Color>,
    pub spawn: f32,
    pub z_index: i32,
    pub animations: Vec<MoveBlock>,
}

/// Properties that can appear inside `text {}`.
#[derive(Clone, Debug, PartialEq)]
pub struct TextNode {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub font: String,
    pub value: String,
    pub fill: Option<Color>,
    pub spawn: f32,
    pub z_index: i32,
    pub spans: Vec<TextSpan>,
    pub animations: Vec<MoveBlock>,
}

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
#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    Header(HeaderConfig),
    Circle(CircleNode),
    Rect(RectNode),
    Text(TextNode),
    /// A top-level `move {}` block that references an element by name.
    Move(MoveBlock),
    EventHandler(EventHandlerNode),
}
