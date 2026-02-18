use serde::{Deserialize, Serialize};
use crate::shapes::ShapeDescriptor;

pub fn default_visible() -> bool {
    true
}

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
        // If indent is empty, we use our internal to_dsl_impl(0) logic
        // but if it's not empty, we might want to respect it.
        // For simplicity with existing code, let's just use to_dsl_impl.
        self.to_dsl_impl(indent.len() / 4)
    }

    fn to_dsl_impl(&self, indent_level: usize) -> String {
        let indent = "    ".repeat(indent_level);
        match self {
            Shape::Circle(c) => {
                let mut out = c.to_dsl(&indent);
                // Only append nested animation blocks when this shape is not at
                // the top-level (indent_level > 0). Top-level animation
                // blocks are emitted by `dsl::generate_dsl()` so emitting them
                // here would duplicate them.
                if indent_level > 0 {
                    for anim in &c.animations {
                        if let Some(ma) = crate::animations::move_animation::MoveAnimation::from_scene(anim) {
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
                        if let Some(ma) = crate::animations::move_animation::MoveAnimation::from_scene(anim) {
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
                        if let Some(ma) = crate::animations::move_animation::MoveAnimation::from_scene(anim) {
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
