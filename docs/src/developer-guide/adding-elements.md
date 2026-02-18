# Adding New Elements to Motioner

This guide explains the step-by-step process of adding a new primitive element (shape) to Motioner using the **Trait-Based Shape System**.

## 1. Create a New Shape Module

Create a new file in `src/shapes/` (e.g., `src/shapes/star.rs`).

Define a struct that holds all the data for your shape:

```rust
// src/shapes/star.rs

use serde::{Deserialize, Serialize};
use crate::scene::Animation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Star {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub points: u32,
    pub color: [u8; 4],
    pub spawn_time: f32,
    #[serde(default)]
    pub animations: Vec<Animation>,
    #[serde(default = "crate::shapes::shapes_manager::default_visible")]
    pub visible: bool,
}

impl Default for Star {
    fn default() -> Self {
        Self {
            name: "Star".to_string(),
            x: 0.5,
            y: 0.5,
            inner_radius: 0.05,
            outer_radius: 0.1,
            points: 5,
            color: [255, 255, 0, 255],
            spawn_time: 0.0,
            animations: Vec::new(),
            visible: true,
        }
    }
}
```

## 2. Implement the `ShapeDescriptor` Trait

The `ShapeDescriptor` trait (defined in `src/shapes/mod.rs`) is the core of the element system. It encapsulates the DSL keyword, the icon, the UI modifiers, and the creation logic.

```rust
use crate::shapes::ShapeDescriptor;
use crate::app_state::AppState;
use eframe::egui;

impl ShapeDescriptor for Star {
    fn dsl_keyword(&self) -> &'static str { "star" }
    fn icon(&self) -> &'static str { "⭐" }

    fn draw_modifiers(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.label("Position");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.x).speed(0.01).prefix("X: "));
            ui.add(egui::DragValue::new(&mut self.y).speed(0.01).prefix("Y: "));
        });

        ui.add(egui::Slider::new(&mut self.inner_radius, 0.0..=1.0).text("Inner Radius"));
        ui.add(egui::Slider::new(&mut self.outer_radius, 0.0..=1.0).text("Outer Radius"));
        ui.add(egui::Slider::new(&mut self.points, 3..=20).text("Points"));

        // Add more controls as needed (color picker, spawn time, etc.)
    }

    fn create_default(name: String) -> crate::shapes::shapes_manager::Shape {
        let mut s = Self::default();
        s.name = name;
        crate::shapes::shapes_manager::Shape::Star(s)
    }
}
```

## 3. Register the Shape in `Shape` Enum

Add your new struct as a variant in `src/shapes/shapes_manager.rs`:

```rust
// src/shapes/shapes_manager.rs

pub enum Shape {
    Circle(crate::shapes::circle::Circle),
    Rect(crate::shapes::rect::Rect),
    Text(crate::shapes::text::Text),
    Star(crate::shapes::star::Star), // Add this line
    Group { ... },
}
```

Also, update the `descriptor()` and `descriptor_mut()` methods in the same file to return `Some(s)` for your new variant.

## 4. Update Core Systems

Because shapes are still stored in a flat enum for serialization and ownership, you need to update a few match patterns:

1.  **Rendering (`src/canvas/ui.rs`)**: Update `draw_shapes_recursive` and `fill_gpu_shapes` to draw your new shape.
2.  **Rasterization (`src/canvas/rasterizer.rs`)**: Update `sample_color_at` for high-quality renders/exports.
3.  **Animations (`src/animations/animations_manager.rs`)**: Update `animated_xy_for` so move animations work.
4.  **DSL Parsing (`src/dsl/mod.rs`)**: Update `parse_dsl_impl` to recognize your keyword and `update_shape_from_kv` to parse its specific properties.
5.  **Scene Graph (`src/scene_graph.rs`)**: Add a button to the "Add Elements" menu that calls `Star::create_default()`.

## Summary of the Trait-Based Advantages

- **Simplified UI**: The Element Modifiers modal automatically uses `draw_modifiers()`, so you don't need to touch that file.
- **Consistent Icons**: The Scene Graph and other UI parts use the `icon()` method.
- **Centralized Logic**: Most shape-specific logic (except for rendering math and parsing) is now contained within the shape's own module.

By following this architecture, adding new elements becomes a matter of implementing a single trait and plugging it into the existing systems.
