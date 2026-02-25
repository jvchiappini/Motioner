//! AST node types for the Motioner DSL.
//!
//! Every piece of DSL source is represented as a typed node in this tree.
//! The parser produces `Vec<Statement>` from raw source text; downstream
//! modules (evaluator, validator, generator) operate on these nodes rather
//! than raw strings.
// ─── Primitive value types ────────────────────────────────────────────────────

/// A 2-tuple of floats, used for coordinates and bezier control points.
pub type Point2 = (f32, f32);

/// The project-level header directives (size/timeline).
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration: f32,
}

// ─── Easing ───────────────────────────────────────────────────────────────────

// The AST no longer defines its own easing enum.  We re-use the shared
// `crate::scene::Easing` type (which is itself an alias of
// `animations::easing::Easing`) to avoid duplicating logic or variants.  This
// simplifies the pipeline and keeps the DSL grammar thin.

// ─── Text ─────────────────────────────────────────────────────────────────────

// `TextSpan` moved to `src/shapes/text.rs`; the parser now constructs
// `shapes::text::TextSpan` directly so the AST no longer defines a
// duplicate `TextSpan` type.

// ─── Animations ───────────────────────────────────────────────────────────────


// ─── Shape elements ───────────────────────────────────────────────────────────

// ─── Header ───────────────────────────────────────────────────────────────────

// (no additional primitive types needed at the moment)

// ─── Event handlers ───────────────────────────────────────────────────────────

// A top-level event handler block, e.g. `on_time { move_element(...) }`.

// Event handlers are no longer represented in the AST.  The parser still
// recognizes them when scanning source (to assist with editor diagnostics),
// but they are not returned as part of the `Statement` list; instead the
// extracted handler list is managed separately by the runtime.  The
// `EventHandlerNode` struct existed to support the old pipeline and has been
// removed.

// ─── Top-level statement ──────────────────────────────────────────────────────

/// Every top-level item that can appear in a Motioner DSL file.
///
/// The only currently‑supported statement is a concrete visual shape; any
/// parsed element is wrapped in `Statement::Shape`.  Additional variants may be
/// introduced when animation blocks or handlers are re‑added.
#[derive(Clone, Debug)]
pub enum Statement {
    /// Any concrete visual shape (Circle, Text, …).
    Shape(crate::scene::Shape),
}
