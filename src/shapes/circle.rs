use crate::app_state::AppState;
use crate::shapes::ShapeDescriptor;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Circle {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub color: [u8; 4],
    pub spawn_time: f32,
    /// Optional explicit kill time (shape is invisible at time >= kill_time)
    #[serde(default)]
    pub kill_time: Option<f32>,
    #[serde(default)]
    pub ephemeral: bool,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub animations: Vec<crate::scene::Animation>,
    #[serde(default = "super::shapes_manager::default_visible")]
    pub visible: bool,
}

impl Default for Circle {
    fn default() -> Self {
        Self {
            name: "Circle".to_string(),
            x: 0.5,
            y: 0.5,
            radius: 0.1,
            color: [120, 200, 255, 255],
            spawn_time: 0.0,
            kill_time: None,
            ephemeral: false,
            z_index: 0,
            animations: Vec::new(),
            visible: true,
        }
    }
}

// (conversion from removed AST node types was moved into the parser)

impl ShapeDescriptor for Circle {
    fn dsl_keyword(&self) -> &'static str {
        "circle"
    }
    fn icon(&self) -> &'static str {
        "⭕"
    }

    fn draw_modifiers(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                state.request_dsl_update();
            }
        });

        ui.checkbox(&mut self.visible, "Visible");

        ui.add(egui::Slider::new(&mut self.x, 0.0..=1.0).text("X"));
        ui.add(egui::Slider::new(&mut self.y, 0.0..=1.0).text("Y"));
        ui.add(egui::Slider::new(&mut self.radius, 0.0..=1.0).text("Radius"));

        ui.horizontal(|ui| {
            ui.label("Color:");
            let mut color_f32 = [
                self.color[0] as f32 / 255.0,
                self.color[1] as f32 / 255.0,
                self.color[2] as f32 / 255.0,
                self.color[3] as f32 / 255.0,
            ];
            if ui
                .color_edit_button_rgba_unmultiplied(&mut color_f32)
                .changed()
            {
                self.color = [
                    (color_f32[0] * 255.0) as u8,
                    (color_f32[1] * 255.0) as u8,
                    (color_f32[2] * 255.0) as u8,
                    (color_f32[3] * 255.0) as u8,
                ];
                state.request_dsl_update();
            }
        });

        ui.add(
            egui::DragValue::new(&mut self.spawn_time)
                .speed(0.1)
                .prefix("Spawn: "),
        );
        // Kill time (optional)
        ui.horizontal(|ui| {
            let mut k = self.kill_time.unwrap_or(f32::NAN);
            let changed = ui
                .add(egui::DragValue::new(&mut k).speed(0.1).prefix("Kill: "))
                .changed();
            if changed {
                if k.is_nan() {
                    self.kill_time = None;
                } else {
                    self.kill_time = Some(k);
                }
                state.request_dsl_update();
            }
        });
    }

    fn to_dsl(&self, indent: &str) -> String {
        if let Some(k) = self.kill_time {
            format!(
                "{}circle \"{}\" {{\n{}\tx = {:.3},\n{}\ty = {:.3},\n{}\tradius = {:.3},\n{}\tfill = \"#{:02x}{:02x}{:02x}\",\n{}\tspawn = {:.2},\n{}\tkill = {:.2}\n{}}}\n",
                indent,
                self.name,
                indent,
                self.x,
                indent,
                self.y,
                indent,
                self.radius,
                indent,
                self.color[0],
                self.color[1],
                self.color[2],
                indent,
                self.spawn_time,
                indent,
                k,
                indent
            )
        } else {
            format!(
                "{}circle \"{}\" {{\n{}\tx = {:.3},\n{}\ty = {:.3},\n{}\tradius = {:.3},\n{}\tfill = \"#{:02x}{:02x}{:02x}\",\n{}\tspawn = {:.2}\n{}}}\n",
                indent,
                self.name,
                indent,
                self.x,
                indent,
                self.y,
                indent,
                self.radius,
                indent,
                self.color[0],
                self.color[1],
                self.color[2],
                indent,
                self.spawn_time,
                indent
            )
        }
    }

    fn create_default(name: String) -> super::shapes_manager::Shape {
        let mut c = Self::default();
        c.name = name;
        super::shapes_manager::Shape::Circle(c)
    }

    fn animations(&self) -> &[crate::scene::Animation] {
        &self.animations
    }

    fn to_element_keyframes(&self, fps: u32) -> crate::shapes::element_store::ElementKeyframes {
        use crate::shapes::element_store::{ElementKeyframes, Keyframe};
        let mut ek = ElementKeyframes::new(self.name.clone(), "circle".into());
        let spawn = crate::shapes::element_store::seconds_to_frame(self.spawn_time, fps);
        ek.spawn_frame = spawn;
        ek.kill_frame = self
            .kill_time
            .map(|k| crate::shapes::element_store::seconds_to_frame(k, fps));
        ek.x.push(Keyframe {
            frame: spawn,
            value: self.x,
            easing: crate::animations::easing::Easing::Linear,
        });
        ek.y.push(Keyframe {
            frame: spawn,
            value: self.y,
            easing: crate::animations::easing::Easing::Linear,
        });
        ek.radius.push(Keyframe {
            frame: spawn,
            value: self.radius,
            easing: crate::animations::easing::Easing::Linear,
        });
        ek.color.push(Keyframe {
            frame: spawn,
            value: self.color,
            easing: crate::animations::easing::Easing::Linear,
        });
        ek.visible.push(Keyframe {
            frame: spawn,
            value: self.visible,
            easing: crate::animations::easing::Easing::Linear,
        });
        ek.z_index.push(Keyframe {
            frame: spawn,
            value: self.z_index,
            easing: crate::animations::easing::Easing::Linear,
        });
        ek.ephemeral = self.ephemeral;
        ek.animations = self.animations.clone();
        ek
    }

    fn changed_frame_props(
        &self,
        orig: Option<&crate::shapes::element_store::FrameProps>,
    ) -> crate::shapes::element_store::FrameProps {
        let mut new_props = crate::shapes::element_store::FrameProps {
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
        };

        if orig.and_then(|p| p.x).unwrap_or(f32::NAN) != self.x {
            new_props.x = Some(self.x);
        }
        if orig.and_then(|p| p.y).unwrap_or(f32::NAN) != self.y {
            new_props.y = Some(self.y);
        }
        if orig.and_then(|p| p.radius).unwrap_or(f32::NAN) != self.radius {
            new_props.radius = Some(self.radius);
        }
        if orig.and_then(|p| p.color) != Some(self.color) {
            new_props.color = Some(self.color);
        }
        if orig.and_then(|p| p.visible) != Some(self.visible) {
            new_props.visible = Some(self.visible);
        }

        new_props
    }

    fn apply_kv_number(&mut self, key: &str, value: f32) {
        match key {
            "x" => self.x = value,
            "y" => self.y = value,
            "radius" => self.radius = value,
            "spawn" => self.spawn_time = value,
            "kill" => self.kill_time = Some(value),
            _ => {}
        }
    }

    fn apply_kv_string(&mut self, key: &str, val: &str) {
        match key {
            "name" => self.name = val.to_string(),
            _ => {}
        }
    }

    fn append_gpu_shapes(
        &self,
        scene_shape: &crate::scene::Shape,
        out: &mut Vec<crate::canvas::gpu::GpuShape>,
        current_time: f32,
        duration: f32,
        spawn: f32,
        rw: f32,
        rh: f32,
    ) {
        // Use the sampled `scene_shape` values (not `self`) so dynamic
        // properties from ElementKeyframes are respected at runtime.
        if let crate::scene::Shape::Circle(c) = scene_shape {
            if !c.visible {
                return;
            }
            let (x, y) = crate::animations::animations_manager::animated_xy_for(
                scene_shape,
                current_time,
                duration,
            );

            let radius_px = c.radius * rw;
            let x_px = x * rw;
            let y_px = y * rh;

            out.push(crate::canvas::gpu::GpuShape {
                pos: [x_px, y_px],
                size: [radius_px, radius_px],
                color: [
                    crate::canvas::gpu::srgb_to_linear(c.color[0]),
                    crate::canvas::gpu::srgb_to_linear(c.color[1]),
                    crate::canvas::gpu::srgb_to_linear(c.color[2]),
                    c.color[3] as f32 / 255.0,
                ],
                shape_type: 0,
                spawn_time: spawn,
                p1: 0,
                p2: 0,
                uv0: [0.0, 0.0],
                uv1: [0.0, 0.0],
            });
        }
    }
}

/// Reconstruct a `Shape::Circle` from `ElementKeyframes` sampled at `frame`.
pub fn from_element_keyframes(
    ek: &crate::shapes::element_store::ElementKeyframes,
    frame: crate::shapes::element_store::FrameIndex,
    fps: u32,
) -> Option<super::shapes_manager::Shape> {
    let props = ek.sample(frame)?;
    let mut c = Circle::default();
    c.name = ek.name.clone();
    if let Some(x) = props.x {
        c.x = x;
    }
    if let Some(y) = props.y {
        c.y = y;
    }
    if let Some(radius) = props.radius {
        c.radius = radius;
    }
    if let Some(col) = props.color {
        c.color = col;
    }
    if let Some(v) = props.visible {
        c.visible = v;
    }
    if let Some(z) = props.z_index {
        c.z_index = z;
    }
    c.spawn_time = frame as f32 / fps as f32;
    if let Some(kf) = ek.kill_frame {
        c.kill_time = Some(kf as f32 / fps as f32);
    }
    c.ephemeral = ek.ephemeral;
    c.animations = ek.animations.clone();
    Some(super::shapes_manager::Shape::Circle(c))
}

// Register converter for decentralized lookup (no central changes required
// when adding new shapes).
inventory::submit! {
    crate::shapes::shapes_manager::ElementKeyframesFactory {
        kind: "circle",
        constructor: crate::shapes::circle::from_element_keyframes,
    }
}

// === DSL block parser for `circle { ... }` ----------------------------------
/// Parse a collected DSL block (lines including header + body) into a
/// `Circle`. This keeps DSL-specific parsing for circles next to the
/// `Circle` type.
pub(crate) fn parse_dsl_block(block: &[String]) -> Option<Circle> {
    let header = block.first()?;
    let name = crate::dsl::parser::extract_name(header)
        .unwrap_or_else(|| format!("Circle_{}", crate::dsl::parser::fastrand_usize()));

    let mut node = Circle::default();
    node.name = name;

    let body = crate::dsl::parser::body_lines(block);
    let mut iter = body.iter().peekable();
    while let Some(line) = iter.next() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if line.starts_with("move") && line.contains('{') {
            let sub = crate::dsl::parser::collect_sub_block(line, &mut iter);
            if let Some(mv) = crate::dsl::parser::parse_move_block_lines(&sub) {
                node.animations.push(crate::dsl::ast_move_to_scene(&mv));
            }
            continue;
        }

        if let Some((key, val)) = crate::dsl::parser::split_kv(line) {
            match key.as_str() {
                "x" => node.x = val.parse().unwrap_or(node.x),
                "y" => node.y = val.parse().unwrap_or(node.y),
                "radius" => node.radius = val.parse().unwrap_or(node.radius),
                "spawn" => node.spawn_time = val.parse().unwrap_or(node.spawn_time),
                "kill" => node.kill_time = val.parse().ok(),
                "z" | "z_index" => node.z_index = val.parse().unwrap_or(node.z_index),
                "fill" => {
                    if let Some(c) = crate::dsl::ast::Color::from_hex(&val) {
                        node.color = c.to_array();
                    }
                }
                _ => {}
            }
        }
    }

    Some(node)
}

fn parse_circle_wrapper(block: &[String]) -> Option<crate::shapes::shapes_manager::Shape> {
    parse_dsl_block(block).map(|c| crate::shapes::shapes_manager::Shape::Circle(c))
}

inventory::submit! {
    crate::shapes::shapes_manager::ShapeParserFactory {
        kind: "circle",
        parser: parse_circle_wrapper,
    }
}
