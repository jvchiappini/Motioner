use serde::{Deserialize, Serialize};

fn default_visible() -> bool {
    true
}

/// Shape enum moved from `scene.rs` to `src/shapes/shapes_manager.rs`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Shape {
    Circle {
        name: String,
        x: f32,
        y: f32,
        radius: f32,
        color: [u8; 4],
        spawn_time: f32,
        #[serde(default)]
        animations: Vec<crate::scene::Animation>,
        #[serde(default = "default_visible")]
        visible: bool,
    },
    Rect {
        name: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 4],
        spawn_time: f32,
        #[serde(default)]
        animations: Vec<crate::scene::Animation>,
        #[serde(default = "default_visible")]
        visible: bool,
    },
    /// Non-visual group that can contain other shapes.
    Group {
        name: String,
        children: Vec<Shape>,
        #[serde(default = "default_visible")]
        visible: bool,
    },
}

impl Shape {
    pub fn is_visible(&self) -> bool {
        match self {
            Shape::Circle { visible, .. } => *visible,
            Shape::Rect { visible, .. } => *visible,
            Shape::Group { visible, .. } => *visible,
        }
    }

    pub fn set_visible(&mut self, v: bool) {
        match self {
            Shape::Circle { visible, .. } => *visible = v,
            Shape::Rect { visible, .. } => *visible = v,
            Shape::Group { visible, .. } => *visible = v,
        }
    }

    pub fn to_dsl(&self) -> String {
        self.to_dsl_impl(0)
    }

    fn to_dsl_impl(&self, indent_level: usize) -> String {
        let indent = "    ".repeat(indent_level);
        match self {
            Shape::Circle {
                name,
                x,
                y,
                radius,
                color,
                spawn_time,
                animations,
                ..
            } => {
                crate::shapes::circle::to_dsl_with_animations(
                    name,
                    *x,
                    *y,
                    *radius,
                    *color,
                    *spawn_time,
                    animations,
                    &indent,
                )
            }
            Shape::Rect {
                name,
                x,
                y,
                w,
                h,
                color,
                spawn_time,
                animations,
                ..
            } => {
                crate::shapes::rect::to_dsl_with_animations(
                    name,
                    *x,
                    *y,
                    *w,
                    *h,
                    *color,
                    *spawn_time,
                    animations,
                    &indent,
                )
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
        vec![Shape::Circle {
            name: "Circle".to_string(),
            x: 0.5,
            y: 0.5,
            radius: 0.1,
            color: crate::shapes::circle::default_color(),
            spawn_time: 0.0,
            animations: Vec::new(),
            visible: true,
        }]
    }
    

    pub fn name(&self) -> &str {
        match self {
            Shape::Circle { name, .. } => name,
            Shape::Rect { name, .. } => name,
            Shape::Group { name, .. } => name,
        }
    }

    pub fn spawn_time(&self) -> f32 {
        match self {
            Shape::Circle { spawn_time, .. } => *spawn_time,
            Shape::Rect { spawn_time, .. } => *spawn_time,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_to_dsl_includes_color_and_spawn() {
        let s = Shape::Circle {
            name: "C".to_string(),
            x: 0.5,
            y: 0.5,
            radius: 0.1,
            color: [17, 34, 51, 255],
            spawn_time: 0.25,
            animations: Vec::new(),
            visible: true,
        };
        let d = s.to_dsl();
        assert!(d.contains("fill = \"#112233\""));
        assert!(d.contains("spawn = 0.25"));
    }
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
    if divergence_idx < from.len() && divergence_idx < to_parent.len() {
        if from[divergence_idx] < to_parent[divergence_idx] {
            actual_to_parent[divergence_idx] -= 1;
        }
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
        && &from[..actual_to_parent.len()] == actual_to_parent
    {
        if from[from.len() - 1] < to_index {
            if actual_to_index > 0 {
                actual_to_index -= 1;
            }
        }
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
