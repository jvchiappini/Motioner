pub mod circle;
pub mod element_store;
pub mod fonts;
pub mod rect;
pub mod shapes_manager;
pub mod text;
pub mod utilities;

use crate::app_state::AppState;
use eframe::egui;

/// Trait that all shapes must implement to be fully integrated into the system automatically.
pub trait ShapeDescriptor {
    /// The keyword used in the DSL (e.g., "circle", "rect").
    fn dsl_keyword(&self) -> &'static str;

    /// Visual icon used in the Scene Graph and toolbars.
    fn icon(&self) -> &'static str;

    /// Render the property editor in the Element Modifiers modal.
    fn draw_modifiers(&mut self, ui: &mut egui::Ui, state: &mut AppState);

    /// Generate the DSL representation for this shape.
    fn to_dsl(&self, indent: &str) -> String;

    /// Convert this concrete Shape instance into an `ElementKeyframes`
    /// representation (anchor at `spawn_time`). Implementations should
    /// populate `spawn_frame`, `kill_frame`, tracks and metadata.
    fn to_element_keyframes(&self, fps: u32) -> crate::shapes::element_store::ElementKeyframes;

    /// Generate a default instance for the "Add Element" menu.
    fn create_default(name: String) -> shapes_manager::Shape
    where
        Self: Sized;

    /// Append one or more `GpuShape` entries for this shape into `out`.
    ///
    /// Default implementation is a no-op so existing shapes don't need to
    /// implement GPU-specific behaviour unless they support GPU composition.
    fn append_gpu_shapes(
        &self,
        _scene_shape: &crate::scene::Shape,
        _out: &mut Vec<crate::canvas::gpu::GpuShape>,
        _current_time: f32,
        _duration: f32,
        _spawn: f32,
        _rw: f32,
        _rh: f32,
    ) {
        // no-op by default
    }

    /// Return the attached animations for this shape (default: empty).
    ///
    /// Implementations that store animations should return a slice into
    /// their internal Vec so callers can iterate without matching on variants.
    fn animations(&self) -> &[crate::scene::Animation] {
        &[]
    }

    /// Append an animation to this shape's internal list.
    fn push_animation(&mut self, anim: crate::scene::Animation);

    // ── Spawn / kill ──────────────────────────────────────────────────────

    /// Time (seconds) at which this shape becomes visible.
    fn spawn_time(&self) -> f32;

    /// Optional explicit kill time (shape hidden at `t >= kill_time`).
    fn kill_time(&self) -> Option<f32>;

    /// Whether this shape was created at runtime and should be excluded from
    /// generated DSL output.
    fn is_ephemeral(&self) -> bool;

    /// Mark or un-mark this shape as ephemeral.
    fn set_ephemeral(&mut self, v: bool);

    // ── Position ─────────────────────────────────────────────────────────

    /// Normalised (0..1) canvas position.
    fn xy(&self) -> (f32, f32);

    // ── Visibility ───────────────────────────────────────────────────────

    /// Whether this shape is currently visible.
    fn is_visible(&self) -> bool;

    /// Set visibility.
    fn set_visible(&mut self, v: bool);

    // ── Fill colour ───────────────────────────────────────────────────────

    /// Override the fill / primary colour.  Default: no-op.
    fn set_fill_color(&mut self, _col: [u8; 4]) {}

    // ── Frame props ───────────────────────────────────────────────────────

    /// Compute which `FrameProps` fields differ between this concrete
    /// shape instance and an optional sampled `orig` props. This is used
    /// when runtime handlers mutate `Shape` instances so the system can
    /// insert hold keyframes for changed properties. Default
    /// implementation returns an empty `FrameProps` (no changes).
    fn changed_frame_props(
        &self,
        _orig: Option<&crate::shapes::element_store::FrameProps>,
    ) -> crate::shapes::element_store::FrameProps {
        crate::shapes::element_store::FrameProps {
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
        }
    }

    // ── KV application ────────────────────────────────────────────────────

    /// Apply a numeric key/value directly to the concrete shape instance.
    /// Default implementation is a no-op; concrete shapes should override
    /// to accept numeric properties like `x`, `y`, `radius`, `spawn`, `kill`.
    fn apply_kv_number(&mut self, _key: &str, _value: f32) {}

    /// Apply a string key/value directly to the concrete shape instance.
    /// Default implementation is a no-op; concrete shapes should override
    /// to accept string properties like `name`, `value`, `font`.
    fn apply_kv_string(&mut self, _key: &str, _val: &str) {}
}
