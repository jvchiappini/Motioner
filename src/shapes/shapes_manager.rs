use serde::{Deserialize, Serialize};

pub fn default_visible() -> bool {
    true
}

// Combined WGSL shader source for the render pipeline.
// Add per-shape WGSL snippets here (one file per shape). When adding a
// new Shape variant you should:
//  1) add its Rust type/enum variant in this file, and
//  2) create a `src/shapes/shaders/<name>.wgsl` file that implements
//     `fn shape_<name>(in: VertexOutput, effective_uv: vec2<f32>) -> vec4<f32>`
//  3) append an `include_str!` entry below so the snippet is compiled
//     into the shader module (this keeps WGSL close to the Shape impl).
pub const COMBINED_WGSL: &str = concat!(
    include_str!("../shaders/render.wgsl"),
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
    /// Return the `ShapeDescriptor` for a concrete shape, if available.
    /// Groups are not descriptors themselves and therefore return `None`.
    pub fn descriptor(&self) -> Option<&dyn crate::shapes::ShapeDescriptor> {
        match self {
            Shape::Circle(c) => Some(c),
            Shape::Rect(r) => Some(r),
            Shape::Text(t) => Some(t),
            Shape::Group { .. } => None,
        }
    }

    /// Mutable variant of [`Shape::descriptor`].
    pub fn descriptor_mut(&mut self) -> Option<&mut dyn crate::shapes::ShapeDescriptor> {
        match self {
            Shape::Circle(c) => Some(c),
            Shape::Rect(r) => Some(r),
            Shape::Text(t) => Some(t),
            Shape::Group { .. } => None,
        }
    }

    /// Convenience accessor for the shape's name.  Groups expose their
    /// own `name` field, while concrete shapes forward to the descriptor
    /// implementation.
    pub fn name(&self) -> &str {
        match self {
            Shape::Circle(c) => &c.name,
            Shape::Rect(r) => &r.name,
            Shape::Text(t) => &t.name,
            Shape::Group { name, .. } => name,
        }
    }

    /// Mark or unmark the shape as ephemeral (runtime-only).  Only
    /// concrete shapes support this; groups are ignored.
    pub fn set_ephemeral(&mut self, v: bool) {
        if let Some(d) = self.descriptor_mut() {
            d.set_ephemeral(v);
        }
    }

    /// Query visibility.  Groups maintain their own `visible` flag, other
    /// shapes defer to the descriptor.
    pub fn is_visible(&self) -> bool {
        match self {
            Shape::Group { visible, .. } => *visible,
            _ => self.descriptor().is_some_and(|d| d.is_visible()),
        }
    }

    /// Render the shape (or group) as DSL text.  Groups are rendered by
    /// iterating their children with increased indentation; concrete
    /// shapes delegate to their descriptor.
    pub fn to_dsl(&self, indent: &str) -> String {
        match self {
            Shape::Group {
                children,
                name: _,
                visible: _,
            } => {
                // groups aren't part of the DSL but we render children
                let mut out = String::new();
                for child in children {
                    out.push_str(&child.to_dsl(indent));
                }
                out
            }
            _ => self
                .descriptor()
                .map_or(String::new(), |d| d.to_dsl(indent)),
        }
    }

    /// Return a small sample scene used during state initialization.
    /// Previously this lived in `scene.rs`.  Keep it simple: one circle
    /// so the editor isn't completely empty.
    pub fn sample_scene() -> Vec<Shape> {
        vec![Shape::Circle(crate::shapes::circle::Circle::default())]
    }
    pub fn set_fill_color(&mut self, col: [u8; 4]) {
        if let Some(d) = self.descriptor_mut() {
            d.set_fill_color(col);
        }
        // Groups have no fill — no-op.
    }

    /// Return the current X/Y position for the shape (normalized 0..1).
    /// Returns `(0.0, 0.0)` for Group nodes.
    pub fn xy(&self) -> (f32, f32) {
        self.descriptor().map_or((0.0, 0.0), |d| d.xy())
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
            crate::shapes::element_store::FrameProps::default()
        }
    }

    /// Return a slice of animations attached to this shape (empty for groups).
    /// Delegates to `ShapeDescriptor::animations()`.
    pub fn animations(&self) -> &[crate::scene::Animation] {
        self.descriptor().map_or(&[], |d| d.animations())
    }

    /// Append an animation to this shape in a shape-agnostic way.
    /// Groups ignore this call (they carry no animations).
    pub fn push_animation(&mut self, anim: crate::scene::Animation) {
        if let Some(d) = self.descriptor_mut() {
            d.push_animation(anim);
        }
    }

    /// Time (seconds) at which this shape first becomes visible.
    /// For groups, returns the minimum spawn time of their children (0.0 if empty).
    pub fn spawn_time(&self) -> f32 {
        if let Some(d) = self.descriptor() {
            return d.spawn_time();
        }
        // Group: minimum spawn time of children.
        if let Shape::Group { children, .. } = self {
            let min = children
                .iter()
                .map(|c| c.spawn_time())
                .fold(f32::INFINITY, f32::min);
            if min.is_finite() {
                min
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Optional explicit kill time for the shape (`None` ⇒ lives forever).
    #[allow(dead_code)]
    pub fn kill_time(&self) -> Option<f32> {
        self.descriptor().and_then(|d| d.kill_time())
        // Groups have no kill time.
    }

    /// Whether this shape was created at runtime and should be excluded from
    /// generated DSL output.
    #[allow(dead_code)]
    pub fn is_ephemeral(&self) -> bool {
        self.descriptor().is_some_and(|d| d.is_ephemeral())
    }

    /// Recursively flattens the scene graph into a list of visual primitives (Circles, Rects).
    /// Inherits spawn_time from parents: a child is only visible if current_time >= parent_spawn and current_time >= child_spawn.
    #[allow(dead_code)]
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
} // end impl Shape

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
/// runtime when handler bodies spawn ephemeral shapes).
///
/// Driven entirely by the `CreateDefaultFactory` registry — no hard-coded
/// keyword list here. Adding a new shape only requires an
/// `inventory::submit!` call in the shape module.
pub fn create_default_by_keyword(keyword: &str, name: String) -> Option<Shape> {
    for factory in inventory::iter::<CreateDefaultFactory> {
        if factory.kind == keyword {
            return Some((factory.constructor)(name));
        }
    }
    None
}

/// Return a slice of all DSL keywords registered by shape modules.
///
/// Used by the validator and runtime to enumerate known keywords without
/// a hard-coded list. The slice is heap-allocated on every call (it is
/// only called during validation, not on the hot path).
pub fn registered_shape_keywords() -> Vec<&'static str> {
    inventory::iter::<CreateDefaultFactory>
        .into_iter()
        .map(|f| f.kind)
        .collect()
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

/// Registry entry type for parsing a DSL `block` into a concrete `Shape`.
///
/// Shape modules should `inventory::submit!` a `ShapeParserFactory` so the
/// central DSL parser can delegate block parsing to the corresponding shape
/// implementation (keeps shape-specific parsing logic next to the shape).
pub struct ShapeParserFactory {
    pub kind: &'static str,
    pub parser: fn(block: &[String]) -> Option<Shape>,
}

inventory::collect!(ShapeParserFactory);

/// Registry entry type for creating a default `Shape` instance by keyword.
///
/// Shape modules should `inventory::submit!` a `CreateDefaultFactory` so
/// `create_default_by_keyword` can delegate without a hard-coded match.
///
/// # Adding a new shape
/// In your shape module add:
/// ```ignore
/// inventory::submit! {
///     crate::shapes::shapes_manager::CreateDefaultFactory {
///         kind: "myshape",
///         constructor: |name| MyShape::create_default(name),
///     }
/// }
/// ```
pub struct CreateDefaultFactory {
    /// DSL keyword, e.g. `"circle"`.
    pub kind: &'static str,
    /// Produce a default `Shape` with the given name.
    pub constructor: fn(String) -> Shape,
}

inventory::collect!(CreateDefaultFactory);

/// Return the editor highlight colour for a DSL keyword, if it corresponds to
/// a registered shape.  This looks up the `CreateDefaultFactory` registry,
/// constructs a temporary instance and queries its `ShapeDescriptor` for
/// `dsl_color()`.  The result is cached by callers in the highlighter; this
/// helper keeps the lookup logic centralized.
pub fn keyword_color(keyword: &str) -> Option<egui::Color32> {
    for factory in inventory::iter::<CreateDefaultFactory> {
        if factory.kind == keyword {
            // create a dummy shape (name doesn't matter) and ask for its
            // descriptor.  `descriptor()` will never return `None` for concrete
            // shapes.
            let shape = (factory.constructor)(String::new());
            if let Some(desc) = shape.descriptor() {
                return Some(desc.dsl_color());
            }
        }
    }
    None
}

/// Try to parse a collected DSL `block` using the registered shape parsers.
pub fn parse_shape_block(block: &[String]) -> Option<Shape> {
    let header = block.first()?;
    // first identifier in the header is the keyword (e.g. "circle")
    let mut ident = String::new();
    for ch in header.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            ident.push(ch);
        } else {
            break;
        }
    }

    for factory in inventory::iter::<ShapeParserFactory> {
        if factory.kind == ident.as_str() {
            return (factory.parser)(block);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// implement ShapeDescriptor for the enum itself so callers can treat a
// `Shape` uniformly without having to manually call `descriptor()` every
// time.  This avoids having to update dozens of call sites such as the
// DSL runtime which previously assumed the enum implemented the trait.
// Concrete shapes already implement the trait; the enum implementation
// simply forwards to the inner descriptor when available and supplies
// reasonable defaults for groups.

impl crate::shapes::ShapeDescriptor for Shape {
    fn dsl_keyword(&self) -> &'static str {
        self.descriptor().map_or("", |d| d.dsl_keyword())
    }
    fn icon(&self) -> &'static str {
        self.descriptor().map_or("", |d| d.icon())
    }
    fn draw_modifiers(
        &mut self,
        ui: &mut eframe::egui::Ui,
        state: &mut crate::app_state::AppState,
    ) {
        if let Some(d) = self.descriptor_mut() {
            d.draw_modifiers(ui, state);
        }
    }
    fn to_dsl(&self, indent: &str) -> String {
        // use the inherent method we already defined above
        self.to_dsl(indent)
    }
    fn to_element_keyframes(&self, fps: u32) -> crate::shapes::element_store::ElementKeyframes {
        self.descriptor().map_or(
            crate::shapes::element_store::ElementKeyframes::new(String::new(), String::new()),
            |d| d.to_element_keyframes(fps),
        )
    }
    fn create_default(name: String) -> Shape
    where
        Self: Sized,
    {
        // not very meaningful at the enum level, but provide a circle
        let c = crate::shapes::circle::Circle {
            name,
            ..Default::default()
        };
        Shape::Circle(c)
    }
    fn animations(&self) -> &[crate::scene::Animation] {
        self.descriptor().map_or(&[], |d| d.animations())
    }
    fn push_animation(&mut self, anim: crate::scene::Animation) {
        if let Some(d) = self.descriptor_mut() {
            d.push_animation(anim);
        }
    }
    fn spawn_time(&self) -> f32 {
        self.descriptor().map_or(0.0, |d| d.spawn_time())
    }
    fn kill_time(&self) -> Option<f32> {
        self.descriptor().and_then(|d| d.kill_time())
    }
    fn is_ephemeral(&self) -> bool {
        self.descriptor().is_some_and(|d| d.is_ephemeral())
    }
    fn set_ephemeral(&mut self, v: bool) {
        if let Some(d) = self.descriptor_mut() {
            d.set_ephemeral(v);
        }
    }
    fn xy(&self) -> (f32, f32) {
        self.descriptor().map_or((0.0, 0.0), |d| d.xy())
    }
    fn is_visible(&self) -> bool {
        self.is_visible()
    }
    fn set_visible(&mut self, v: bool) {
        if let Some(d) = self.descriptor_mut() {
            d.set_visible(v);
        }
    }
    fn set_fill_color(&mut self, col: [u8; 4]) {
        if let Some(d) = self.descriptor_mut() {
            d.set_fill_color(col);
        }
    }
    fn changed_frame_props(
        &self,
        orig: Option<&crate::shapes::element_store::FrameProps>,
    ) -> crate::shapes::element_store::FrameProps {
        if let Some(d) = self.descriptor() {
            d.changed_frame_props(orig)
        } else {
            crate::shapes::element_store::FrameProps::default()
        }
    }
    fn apply_kv_number(&mut self, key: &str, value: f32) {
        if let Some(d) = self.descriptor_mut() {
            d.apply_kv_number(key, value);
        }
    }
    fn apply_kv_string(&mut self, key: &str, value: &str) {
        if let Some(d) = self.descriptor_mut() {
            d.apply_kv_string(key, value);
        }
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
    _render_height: u32,
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
                // GPU rendering treats the rectangle's `x`/`y` as the centre
                // of mass (see `shapes/rect.rs::append_gpu_shapes`).  Hit
                // testing must use the same convention, otherwise the clicked
                // area will be offset from what the user sees on screen.
                let centre = composition_rect.left_top()
                    + eframe::egui::vec2(
                        r.x * composition_rect.width(),
                        r.y * composition_rect.height(),
                    );
                let w_px = r.w * composition_rect.width();
                let h_px = r.h * composition_rect.height();
                let rect =
                    eframe::egui::Rect::from_center_size(centre, eframe::egui::vec2(w_px, h_px));
                if rect.contains(pos) {
                    return Some(vec![i]);
                }
            }
            Shape::Text(t) => {
                // text also uses centre-of-mass coordinates when rendered.
                let centre = composition_rect.left_top()
                    + eframe::egui::vec2(
                        t.x * composition_rect.width(),
                        t.y * composition_rect.height(),
                    );
                let height_px = t.size * composition_rect.height();
                let width_px = t.value.len() as f32 * height_px * 0.5; // approximate
                let rect = eframe::egui::Rect::from_center_size(
                    centre,
                    eframe::egui::vec2(width_px, height_px),
                );
                if rect.contains(pos) {
                    return Some(vec![i]);
                }
            }
            Shape::Group { children, .. } => {
                if let Some(mut cp) = find_hit_path(
                    children,
                    pos,
                    composition_rect,
                    _zoom,
                    current_time,
                    actual_spawn,
                    _render_height,
                ) {
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
    _render_height: u32,
) {
    let actual_spawn = shape.spawn_time().max(parent_spawn);
    if current_time < actual_spawn {
        return;
    }
    match shape {
        Shape::Circle(c) => {
            let center = composition_rect.left_top()
                + eframe::egui::vec2(
                    c.x * composition_rect.width(),
                    c.y * composition_rect.height(),
                );
            painter.circle_stroke(center, c.radius * composition_rect.width(), stroke);
        }
        Shape::Rect(r) => {
            // rectangle coordinates are centred, so compute centre and draw
            // highlight around it rather than treating `x`/`y` as top-left.
            let centre = composition_rect.left_top()
                + eframe::egui::vec2(
                    r.x * composition_rect.width(),
                    r.y * composition_rect.height(),
                );
            let w_px = r.w * composition_rect.width();
            let h_px = r.h * composition_rect.height();
            painter.rect_stroke(
                eframe::egui::Rect::from_center_size(centre, eframe::egui::vec2(w_px, h_px)),
                0.0,
                stroke,
            );
        }
        Shape::Text(t) => {
            // text uses a central anchor as well; highlight around centre.
            let centre = composition_rect.left_top()
                + eframe::egui::vec2(
                    t.x * composition_rect.width(),
                    t.y * composition_rect.height(),
                );
            let height_px = t.size * composition_rect.height();
            let width_px = t.value.len() as f32 * height_px * 0.5;
            painter.rect_stroke(
                eframe::egui::Rect::from_center_size(
                    centre,
                    eframe::egui::vec2(width_px, height_px),
                ),
                0.0,
                stroke,
            );
        }
        Shape::Group { children, .. } => {
            for child in children {
                draw_highlight_recursive(
                    painter,
                    child,
                    composition_rect,
                    stroke,
                    current_time,
                    actual_spawn,
                    _render_height,
                );
            }
        }
    }
}

#[allow(dead_code)]
pub fn move_node(
    scene: &mut Vec<Shape>,
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
    // `get_shape` works on slices so we coerce the Vec to a slice here.
    let source_node = match get_shape(&*scene, from) {
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
    } else if let Some(Shape::Group { children, .. }) =
        get_shape_mut(&mut scene[..], &actual_to_parent)
    {
        let insert_at = actual_to_index.min(children.len());
        children.insert(insert_at, source_node);
        final_path.push(insert_at);
    } else {
        return None;
    }

    Some(final_path)
}
