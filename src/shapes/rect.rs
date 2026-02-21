use crate::app_state::AppState;
use crate::shapes::ShapeDescriptor;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [u8; 4],
    pub spawn_time: f32,
    #[serde(default)]
    pub kill_time: Option<f32>,
    /// Runtime-created ephemeral shapes are not emitted to generated DSL
    #[serde(default)]
    pub ephemeral: bool,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub animations: Vec<crate::scene::Animation>,
    #[serde(default = "super::shapes_manager::default_visible")]
    pub visible: bool,
}

impl Default for Rect {
    fn default() -> Self {
        Self {
            name: "Rect".to_string(),
            x: 0.4,
            y: 0.4,
            w: 0.2,
            h: 0.2,
            color: [200, 120, 120, 255],
            spawn_time: 0.0,
            kill_time: None,
            ephemeral: false,
            z_index: 0,
            animations: Vec::new(),
            visible: true,
        }
    }
}

impl ShapeDescriptor for Rect {
    fn dsl_keyword(&self) -> &'static str {
        "rect"
    }
    fn icon(&self) -> &'static str {
        "⬛"
    }

    fn dsl_color(&self) -> egui::Color32 {
        // use a warm coral colour by default
        egui::Color32::from_rgb(200, 120, 120)
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
        ui.add(egui::Slider::new(&mut self.w, 0.0..=1.0).text("Width"));
        ui.add(egui::Slider::new(&mut self.h, 0.0..=1.0).text("Height"));

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
        // optional kill time
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
                "{}rect \"{}\" {{\n{}\tx = {:.3},\n{}\ty = {:.3},\n{}\twidth = {:.3},\n{}\theight = {:.3},\n{}\tfill = \"#{:02x}{:02x}{:02x}\",\n{}\tspawn = {:.2},\n{}\tkill = {:.2}\n{}}}\n",
                indent,
                self.name,
                indent,
                self.x,
                indent,
                self.y,
                indent,
                self.w,
                indent,
                self.h,
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
                "{}rect \"{}\" {{\n{}\tx = {:.3},\n{}\ty = {:.3},\n{}\twidth = {:.3},\n{}\theight = {:.3},\n{}\tfill = \"#{:02x}{:02x}{:02x}\",\n{}\tspawn = {:.2}\n{}}}\n",
                indent,
                self.name,
                indent,
                self.x,
                indent,
                self.y,
                indent,
                self.w,
                indent,
                self.h,
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
        let mut r = Self::default();
        r.name = name;
        super::shapes_manager::Shape::Rect(r)
    }

    fn animations(&self) -> &[crate::scene::Animation] {
        &self.animations
    }

    fn push_animation(&mut self, anim: crate::scene::Animation) {
        self.animations.push(anim);
    }

    fn spawn_time(&self) -> f32 {
        self.spawn_time
    }
    fn kill_time(&self) -> Option<f32> {
        self.kill_time
    }
    fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }
    fn set_ephemeral(&mut self, v: bool) {
        self.ephemeral = v;
    }
    fn xy(&self) -> (f32, f32) {
        (self.x, self.y)
    }
    fn is_visible(&self) -> bool {
        self.visible
    }
    fn set_visible(&mut self, v: bool) {
        self.visible = v;
    }
    fn set_fill_color(&mut self, col: [u8; 4]) {
        self.color = col;
    }

    fn to_element_keyframes(&self, fps: u32) -> crate::shapes::element_store::ElementKeyframes {
        use crate::shapes::element_store::{ElementKeyframes, Keyframe};
        let mut ek = ElementKeyframes::new(self.name.clone(), "rect".into());
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
        ek.w.push(Keyframe {
            frame: spawn,
            value: self.w,
            easing: crate::animations::easing::Easing::Linear,
        });
        ek.h.push(Keyframe {
            frame: spawn,
            value: self.h,
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
        if orig.and_then(|p| p.w).unwrap_or(f32::NAN) != self.w {
            new_props.w = Some(self.w);
        }
        if orig.and_then(|p| p.h).unwrap_or(f32::NAN) != self.h {
            new_props.h = Some(self.h);
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
            "width" | "w" => self.w = value,
            "height" | "h" => self.h = value,
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
        if let crate::scene::Shape::Rect(r) = scene_shape {
            if !r.visible {
                return;
            }
            let (x, y) = crate::animations::animations_manager::animated_xy_for(
                scene_shape,
                current_time,
                duration,
            );

            let w_px = r.w * rw;
            let h_px = r.h * rh;
            let x_px = x * rw;
            let y_px = y * rh;

            // `x`/`y` are interpreted as the shape's centre-of-mass.  The
            // previous implementation added half the width/height here to
            // convert from a top-left coordinate; with the GLSL adjustment we
            // made earlier this translation is no longer necessary.  Circles
            // already followed the centre convention, so rectangles now do as
            // well.
            out.push(crate::canvas::gpu::GpuShape {
                pos: [x_px, y_px],
                size: [w_px / 2.0, h_px / 2.0],
                color: [
                    crate::canvas::gpu::srgb_to_linear(r.color[0]),
                    crate::canvas::gpu::srgb_to_linear(r.color[1]),
                    crate::canvas::gpu::srgb_to_linear(r.color[2]),
                    r.color[3] as f32 / 255.0,
                ],
                shape_type: 1,
                spawn_time: spawn,
                p1: 0,
                p2: 0,
                uv0: [0.0, 0.0],
                uv1: [0.0, 0.0],
            });
        }
    }
}

/// Reconstruct a `Shape::Rect` from `ElementKeyframes` sampled at `frame`.
pub fn from_element_keyframes(
    ek: &crate::shapes::element_store::ElementKeyframes,
    frame: crate::shapes::element_store::FrameIndex,
    fps: u32,
) -> Option<super::shapes_manager::Shape> {
    let props = ek.sample(frame, fps)?;
    let mut r = Rect::default();
    r.name = ek.name.clone();
    if let Some(x) = props.x {
        r.x = x;
    }
    if let Some(y) = props.y {
        r.y = y;
    }
    if let Some(w) = props.w {
        r.w = w;
    }
    if let Some(h) = props.h {
        r.h = h;
    }
    if let Some(col) = props.color {
        r.color = col;
    }
    if let Some(v) = props.visible {
        r.visible = v;
    }
    if let Some(z) = props.z_index {
        r.z_index = z;
    }
    r.spawn_time = frame as f32 / fps as f32;
    if let Some(kf) = ek.kill_frame {
        r.kill_time = Some(kf as f32 / fps as f32);
    }
    r.ephemeral = ek.ephemeral;
    Some(super::shapes_manager::Shape::Rect(r))
}

inventory::submit! {
    crate::shapes::shapes_manager::ElementKeyframesFactory {
        kind: "rect",
        constructor: crate::shapes::rect::from_element_keyframes,
    }
}

pub(crate) fn parse_dsl_block(block: &[String]) -> Option<Rect> {
    let header = block.first()?;
    let name = crate::dsl::parser::extract_name(header)
        .unwrap_or_else(|| format!("Rect_{}", crate::dsl::parser::fastrand_usize()));

    let mut node = Rect::default();
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
                "width" | "w" => node.w = val.parse().unwrap_or(node.w),
                "height" | "h" => node.h = val.parse().unwrap_or(node.h),
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

fn parse_rect_wrapper(block: &[String]) -> Option<crate::shapes::shapes_manager::Shape> {
    parse_dsl_block(block).map(|r| crate::shapes::shapes_manager::Shape::Rect(r))
}

inventory::submit! {
    crate::shapes::shapes_manager::ShapeParserFactory {
        kind: "rect",
        parser: parse_rect_wrapper,
    }
}

inventory::submit! {
    crate::shapes::shapes_manager::CreateDefaultFactory {
        kind: "rect",
        constructor: |name| Rect::create_default(name),
    }
}
