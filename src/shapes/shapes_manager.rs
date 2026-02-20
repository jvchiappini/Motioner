use crate::shapes::ShapeDescriptor;
use serde::{Deserialize, Serialize};

pub fn default_visible() -> bool {
    true
}

// Combined WGSL shader source for the composition pipeline.
// Add per-shape WGSL snippets here (one file per shape). When adding a
// new Shape variant you should:
//  1) add its Rust type/enum variant in this file, and
//  2) create a `src/shapes/shaders/<name>.wgsl` file that implements
//     `fn shape_<name>(in: VertexOutput, effective_uv: vec2<f32>) -> vec4<f32>`
//  3) append an `include_str!` entry below so the snippet is compiled
//     into the shader module (this keeps WGSL close to the Shape impl).
pub const COMBINED_WGSL: &str = concat!(
    include_str!("../composition.wgsl"),
    include_str!("shaders/circle.wgsl"),
    include_str!("shaders/rect.wgsl"),
    include_str!("shaders/text.wgsl"),
);

/// Shape enum moved from `scene.rs` to `src/shapes/shapes_manager.rs`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Shape {
    Circle(crate::shapes::circle::Circle),
    Rect(crate::shapes::rect::Rect),
    Text(crate::shapes::text::Text),
    /// Non-visual group that can contain other shapes.
    Group {
        name: String,
        children: Vec<Shape>,
        #[serde(default = "default_visible")]
        visible: bool,
    },
}

impl Shape {
    pub fn descriptor(&self) -> Option<&dyn crate::shapes::ShapeDescriptor> {
        match self {
            Shape::Circle(c) => Some(c),
            Shape::Rect(r) => Some(r),
            Shape::Text(t) => Some(t),
            Shape::Group { .. } => None,
        }
    }

    pub fn descriptor_mut(&mut self) -> Option<&mut dyn crate::shapes::ShapeDescriptor> {
        match self {
            Shape::Circle(c) => Some(c),
            Shape::Rect(r) => Some(r),
            Shape::Text(t) => Some(t),
            Shape::Group { .. } => None,
        }
    }
    pub fn is_visible(&self) -> bool {
        match self {
            Shape::Circle(c) => c.visible,
            Shape::Rect(r) => r.visible,
            Shape::Text(t) => t.visible,
            Shape::Group { visible, .. } => *visible,
        }
    }

    pub fn set_visible(&mut self, v: bool) {
        match self {
            Shape::Circle(c) => c.visible = v,
            Shape::Rect(r) => r.visible = v,
            Shape::Text(t) => t.visible = v,
            Shape::Group { visible, .. } => *visible = v,
        }
    }

    pub fn to_dsl(&self, indent: &str) -> String {
        // If indent is empty, we use our internal to_dsl_impl(0) logic.
        // When an indent string is provided, accept both tab-based
        // indentation and legacy 4-space groups.
        let indent_level = if indent.contains('\t') {
            indent.chars().filter(|c| *c == '\t').count()
        } else {
            indent.len() / 4
        };
        self.to_dsl_impl(indent_level)
    }

    fn to_dsl_impl(&self, indent_level: usize) -> String {
        // Use tabs for indentation in generated DSL (one tab == one level).
        let indent = "\t".repeat(indent_level);
        match self {
            Shape::Circle(c) => {
                let mut out = c.to_dsl(&indent);
                // Only append nested animation blocks when this shape is not at
                // the top-level (indent_level > 0). Top-level animation
                // blocks are emitted by `dsl::generate_dsl()` so emitting them
                // here would duplicate them.
                if indent_level > 0 {
                    for anim in &c.animations {
                        if let Some(ma) =
                            crate::animations::move_animation::MoveAnimation::from_scene(anim)
                        {
                            out.push_str(&ma.to_dsl_block(None, &indent));
                            out.push('\n');
                        }
                    }
                }
                out
            }
            Shape::Rect(r) => {
                let mut out = r.to_dsl(&indent);
                if indent_level > 0 {
                    for anim in &r.animations {
                        if let Some(ma) =
                            crate::animations::move_animation::MoveAnimation::from_scene(anim)
                        {
                            out.push_str(&ma.to_dsl_block(None, &indent));
                            out.push('\n');
                        }
                    }
                }
                out
            }
            Shape::Text(t) => {
                let mut out = t.to_dsl(&indent);
                if indent_level > 0 {
                    for anim in &t.animations {
                        if let Some(ma) =
                            crate::animations::move_animation::MoveAnimation::from_scene(anim)
                        {
                            out.push_str(&ma.to_dsl_block(None, &indent));
                            out.push('\n');
                        }
                    }
                }
                out
            }
            Shape::Group { name, children, .. } => {
                let mut items: Vec<String> = Vec::new();
                for c in children {
                    items.push(c.to_dsl_impl(indent_level + 1));
                }
                if items.is_empty() {
                    format!("{}group(name = \"{}\") {{}}", indent, name)
                } else {
                    format!(
                        "{}group(name = \"{}\") {{\n{}\n{}}}",
                        indent,
                        name,
                        items.join("\n"),
                        indent
                    )
                }
            }
        }
    }

    pub fn sample_scene() -> Vec<Shape> {
        vec![
            Shape::Circle(crate::shapes::circle::Circle::default()),
            Shape::Text(crate::shapes::text::Text::default()),
        ]
    }

    pub fn name(&self) -> &str {
        match self {
            Shape::Circle(c) => &c.name,
            Shape::Rect(r) => &r.name,
            Shape::Text(t) => &t.name,
            Shape::Group { name, .. } => name,
        }
    }

    pub fn set_name(&mut self, new_name: String) {
        match self {
            Shape::Circle(c) => c.name = new_name,
            Shape::Rect(r) => r.name = new_name,
            Shape::Text(t) => t.name = new_name,
            Shape::Group { name, .. } => *name = new_name,
        }
    }

    /// Mark or un-mark a shape as ephemeral (created at runtime).
    pub fn set_ephemeral(&mut self, v: bool) {
        match self {
            Shape::Circle(c) => c.ephemeral = v,
            Shape::Rect(r) => r.ephemeral = v,
            Shape::Text(t) => t.ephemeral = v,
            Shape::Group { .. } => {}
        }
    }

    /// Apply a numeric KV to the shape (used by DSL/runtime). Unknown keys
    /// are silently ignored so callers don't need to match on variants.
    pub fn apply_kv_number(&mut self, key: &str, value: f32) {
        // Delegate to the per-shape `ShapeDescriptor` implementation so
        // concrete shape modules own the mapping of keys -> fields.
        if let Some(desc) = self.descriptor_mut() {
            desc.apply_kv_number(key, value);
        }
    }

    /// Apply a string KV to the shape (name, font, value, etc.). Unknown
    /// keys are ignored.
    pub fn apply_kv_string(&mut self, key: &str, val: &str) {
        if let Some(desc) = self.descriptor_mut() {
            desc.apply_kv_string(key, val);
        }
    }

    /// Set the fill/color for shapes that support it.
    pub fn set_fill_color(&mut self, col: [u8; 4]) {
        match self {
            Shape::Circle(c) => c.color = col,
            Shape::Rect(r) => r.color = col,
            Shape::Text(t) => t.color = col,
            Shape::Group { .. } => {}
        }
    }

    /// Return the current X/Y position for the shape (normalized 0..1).
    /// Returns (0.0, 0.0) for non-visual/group nodes.
    pub fn xy(&self) -> (f32, f32) {
        match self {
            Shape::Circle(c) => (c.x, c.y),
            Shape::Rect(r) => (r.x, r.y),
            Shape::Text(t) => (t.x, t.y),
            Shape::Group { .. } => (0.0, 0.0),
        }
    }

    /// Compute a `FrameProps` containing only the fields that differ between
    /// the current shape state and an optional `orig` sample. This extracts
    /// the minimal set of properties that must be inserted as hold
    /// keyframes when the shape was mutated at runtime.
    pub fn changed_frame_props(
        &self,
        orig: Option<&crate::shapes::element_store::FrameProps>,
    ) -> crate::shapes::element_store::FrameProps {
        // Delegate to the per-shape `ShapeDescriptor` implementation when
        // available so shape-specific logic lives with the concrete shape.
        if let Some(desc) = self.descriptor() {
            desc.changed_frame_props(orig)
        } else {
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
    }

    /// Return a slice of animations attached to this shape (empty by default
    /// for groups). This delegates to the per-shape `ShapeDescriptor` where
    /// available so callers don't need to match on the enum.
    pub fn animations(&self) -> &[crate::scene::Animation] {
        self.descriptor().map_or(&[], |d| d.animations())
    }

    /// Append an animation to this shape in a shape-agnostic way.
    ///
    /// This avoids callers having to match on `Shape::Circle/Rect/Text` when
    /// they only need to attach an animation to an existing shape.
    pub fn push_animation(&mut self, anim: crate::scene::Animation) {
        match self {
            Shape::Circle(c) => c.animations.push(anim),
            Shape::Rect(r) => r.animations.push(anim),
            Shape::Text(t) => t.animations.push(anim),
            Shape::Group { .. } => {
                // Groups don't store animations directly — ignore.
            }
        }
    }

    pub fn spawn_time(&self) -> f32 {
        match self {
            Shape::Circle(c) => c.spawn_time,
            Shape::Rect(r) => r.spawn_time,
            Shape::Text(t) => t.spawn_time,
            Shape::Group { children, .. } => {
                let mut min_t = f32::INFINITY;
                for child in children {
                    let child_t = child.spawn_time();
                    if child_t < min_t {
                        min_t = child_t;
                    }
                }
                if min_t == f32::INFINITY {
                    0.0
                } else {
                    min_t
                }
            }
        }
    }

    /// Optional explicit kill time for the shape (None => no kill time)
    pub fn kill_time(&self) -> Option<f32> {
        match self {
            Shape::Circle(c) => c.kill_time,
            Shape::Rect(r) => r.kill_time,
            Shape::Text(t) => t.kill_time,
            Shape::Group { .. } => None,
        }
    }

    /// Whether this shape was created at runtime (ephemeral) and should be
    /// excluded from generated DSL output.
    pub fn is_ephemeral(&self) -> bool {
        match self {
            Shape::Circle(c) => c.ephemeral,
            Shape::Rect(r) => r.ephemeral,
            Shape::Text(t) => t.ephemeral,
            Shape::Group { .. } => false,
        }
    }

    /// Recursively flattens the scene graph into a list of visual primitives (Circles, Rects).
    /// Inherits spawn_time from parents: a child is only visible if current_time >= parent_spawn and current_time >= child_spawn.
    pub fn flatten(&self, parent_spawn: f32) -> Vec<(Shape, f32)> {
        if !self.is_visible() {
            return Vec::new();
        }
        let mut flattened = Vec::new();

        match self {
            Shape::Group { children, .. } => {
                // Groups don't have their own spawn_time anymore,
                // they just pass through the parent constraint.
                for child in children {
                    flattened.extend(child.flatten(parent_spawn));
                }
            }
            _ => {
                let my_spawn = self.spawn_time().max(parent_spawn);
                flattened.push((self.clone(), my_spawn));
            }
        }
        flattened
    }
}

/// Helper: build a `Shape` from `ElementKeyframes` by delegating to the
/// per-shape modules. This centralizes the single dispatch point so the
/// shape-specific logic lives in the shape implementation files.
pub fn from_element_keyframes(
    ek: &crate::shapes::element_store::ElementKeyframes,
    frame: crate::shapes::element_store::FrameIndex,
    fps: u32,
) -> Option<Shape> {
    // Lookup in the registry populated by each shape module via `inventory`.
    for factory in inventory::iter::<ElementKeyframesFactory> {
        if factory.kind == ek.kind.as_str() {
            return (factory.constructor)(ek, frame, fps);
        }
    }
    None
}

/// Create a default `Shape` instance for a DSL keyword (used by the
/// runtime when handler bodies spawn ephemeral shapes). Centralises the
/// keyword -> concrete type mapping so callers don't need to match on
/// variants.
pub fn create_default_by_keyword(keyword: &str, name: String) -> Option<Shape> {
    match keyword {
        "circle" => Some(crate::shapes::circle::Circle::create_default(name)),
        "rect" => Some(crate::shapes::rect::Rect::create_default(name)),
        "text" => Some(crate::shapes::text::Text::create_default(name)),
        _ => None,
    }
}

/// Registry entry type for ElementKeyframes -> Shape constructors.
pub struct ElementKeyframesFactory {
    pub kind: &'static str,
    pub constructor: fn(
        &crate::shapes::element_store::ElementKeyframes,
        crate::shapes::element_store::FrameIndex,
        u32,
    ) -> Option<Shape>,
}

inventory::collect!(ElementKeyframesFactory);

pub type Scene = Vec<Shape>;

pub fn get_shape_mut<'a>(scene: &'a mut [Shape], path: &[usize]) -> Option<&'a mut Shape> {
    if path.is_empty() {
        return None;
    }
    let mut current = scene.get_mut(path[0])?;
    for &idx in &path[1..] {
        current = match current {
            Shape::Group { children, .. } => children.get_mut(idx)?,
            _ => return None,
        };
    }
    Some(current)
}

pub fn get_shape<'a>(scene: &'a [Shape], path: &[usize]) -> Option<&'a Shape> {
    if path.is_empty() {
        return None;
    }
    let mut current = scene.get(path[0])?;
    for &idx in &path[1..] {
        current = match current {
            Shape::Group { children, .. } => children.get(idx)?,
            _ => return None,
        };
    }
    Some(current)
}

/// Find a hit path in `shapes` for the given `pos` (painter's-algorithm order).
/// Returns a path (Vec<usize>) from root -> hit node or `None` if nothing hit.
pub fn find_hit_path(
    shapes: &[Shape],
    pos: eframe::egui::Pos2,
    composition_rect: eframe::egui::Rect,
    _zoom: f32,
    current_time: f32,
    parent_spawn: f32,
    render_height: u32,
) -> Option<Vec<usize>> {
    for (i, shape) in shapes.iter().enumerate().rev() {
        let actual_spawn = shape.spawn_time().max(parent_spawn);
        if current_time < actual_spawn {
            continue;
        }

        match shape {
            Shape::Circle(c) => {
                let center = composition_rect.left_top()
                    + eframe::egui::vec2(
                        c.x * composition_rect.width(),
                        c.y * composition_rect.height(),
                    );
                if pos.distance(center) <= c.radius * composition_rect.width() {
                    return Some(vec![i]);
                }
            }
            Shape::Rect(r) => {
                let min = composition_rect.left_top()
                    + eframe::egui::vec2(
                        r.x * composition_rect.width(),
                        r.y * composition_rect.height(),
                    );
                let rect = eframe::egui::Rect::from_min_size(
                    min,
                    eframe::egui::vec2(r.w * composition_rect.width(), r.h * composition_rect.height()),
                );
                if rect.contains(pos) {
                    return Some(vec![i]);
                }
            }
            Shape::Text(t) => {
                let min = composition_rect.left_top()
                    + eframe::egui::vec2(t.x * composition_rect.width(), t.y * composition_rect.height());
                let height_px = t.size * composition_rect.height();
                let width_px = t.value.len() as f32 * height_px * 0.5; // approximate
                let rect = eframe::egui::Rect::from_min_size(min, eframe::egui::vec2(width_px, height_px));
                if rect.contains(pos) {
                    return Some(vec![i]);
                }
            }
            Shape::Group { children, .. } => {
                if let Some(mut cp) = find_hit_path(children, pos, composition_rect, _zoom, current_time, actual_spawn, render_height) {
                    let mut path = vec![i];
                    path.append(&mut cp);
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Draw highlight rectangle/circle for a shape (recurses into groups).
pub fn draw_highlight_recursive(
    painter: &eframe::egui::Painter,
    shape: &Shape,
    composition_rect: eframe::egui::Rect,
    stroke: eframe::egui::Stroke,
    current_time: f32,
    parent_spawn: f32,
    render_height: u32,
) {
    let actual_spawn = shape.spawn_time().max(parent_spawn);
    if current_time < actual_spawn {
        return;
    }
    match shape {
        Shape::Circle(c) => {
            let center = composition_rect.left_top()
                + eframe::egui::vec2(c.x * composition_rect.width(), c.y * composition_rect.height());
            painter.circle_stroke(center, c.radius * composition_rect.width(), stroke);
        }
        Shape::Rect(r) => {
            let min = composition_rect.left_top()
                + eframe::egui::vec2(r.x * composition_rect.width(), r.y * composition_rect.height());
            painter.rect_stroke(
                eframe::egui::Rect::from_min_size(
                    min,
                    eframe::egui::vec2(r.w * composition_rect.width(), r.h * composition_rect.height()),
                ),
                0.0,
                stroke,
            );
        }
        Shape::Text(t) => {
            let min = composition_rect.left_top()
                + eframe::egui::vec2(t.x * composition_rect.width(), t.y * composition_rect.height());
            let height_px = t.size * composition_rect.height();
            let width_px = t.value.len() as f32 * height_px * 0.5;
            painter.rect_stroke(
                eframe::egui::Rect::from_min_size(min, eframe::egui::vec2(width_px, height_px)),
                0.0,
                stroke,
            );
        }
        Shape::Group { children, .. } => {
            for child in children {
                draw_highlight_recursive(painter, child, composition_rect, stroke, current_time, actual_spawn, render_height);
            }
        }
    }
}

pub fn move_node(
    scene: &mut Scene,
    from: &[usize],
    to_parent: &[usize],
    to_index: usize,
) -> Option<Vec<usize>> {
    if from.is_empty() {
        return None;
    }

    // Check if to_parent is a child of 'from' - abort if so
    if to_parent.len() >= from.len() && &to_parent[..from.len()] == from {
        return None;
    }

    // 1. Get and clone the source node
    let source_node = match get_shape(scene, from) {
        Some(s) => s.clone(),
        None => return None,
    };

    // Prepare an adjusted to_parent in case it's affected by the removal of 'from'
    let mut actual_to_parent = to_parent.to_vec();

    // Find where the paths diverge
    let mut divergence_idx = 0;
    while divergence_idx < from.len()
        && divergence_idx < to_parent.len()
        && from[divergence_idx] == to_parent[divergence_idx]
    {
        divergence_idx += 1;
    }

    // If they diverge at some index, and at that index from[divergence_idx] < to_parent[divergence_idx],
    // then removing 'from' will decrement the index at divergence_idx in to_parent.
    if divergence_idx < from.len()
        && divergence_idx < to_parent.len()
        && from[divergence_idx] < to_parent[divergence_idx]
    {
        actual_to_parent[divergence_idx] -= 1;
    }

    // 2. Remove the source node
    // We do this by traversing to the parent of 'from'
    if from.len() == 1 {
        let idx = from[0];
        if idx < scene.len() {
            scene.remove(idx);
        }
    } else {
        let parent_path = &from[..from.len() - 1];
        let last_idx = from[from.len() - 1];
        if let Some(Shape::Group { children, .. }) = get_shape_mut(scene, parent_path) {
            if last_idx < children.len() {
                children.remove(last_idx);
            }
        }
    }

    // 3. Re-calculate target index if the removal shifted it
    // If 'from' parent is same as 'to_parent' (using adjusted) and 'from' index < 'to_index',
    // the removal shifts the target index back by 1.
    let mut actual_to_index = to_index;
    if from.len() == actual_to_parent.len() + 1
        && from[..actual_to_parent.len()] == actual_to_parent
        && from[from.len() - 1] < to_index
    {
        actual_to_index = actual_to_index.saturating_sub(1);
    }

    // 4. Insert at target
    let mut final_path = actual_to_parent.clone();
    if actual_to_parent.is_empty() {
        let insert_at = actual_to_index.min(scene.len());
        scene.insert(insert_at, source_node);
        final_path.push(insert_at);
    } else if let Some(Shape::Group { children, .. }) = get_shape_mut(scene, &actual_to_parent) {
        let insert_at = actual_to_index.min(children.len());
        children.insert(insert_at, source_node);
        final_path.push(insert_at);
    } else {
        return None;
    }

    Some(final_path)
}
