use crate::app_state::AppState;
use crate::shapes::ShapeDescriptor;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextSpan {
    pub text: String,
    pub font: String,
    pub size: f32,
    pub color: [u8; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Text {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub value: String,  // Default/Legacy
    pub font: String,   // Default/Legacy
    pub size: f32,      // Default/Legacy
    pub color: [u8; 4], // Default/Legacy
    pub spans: Vec<TextSpan>,
    pub spawn_time: f32,
    #[serde(default)]
    pub kill_time: Option<f32>,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub animations: Vec<crate::scene::Animation>,
    #[serde(default)]
    pub ephemeral: bool,
    #[serde(default = "super::shapes_manager::default_visible")]
    pub visible: bool,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            name: "Text".to_string(),
            x: 0.5,
            y: 0.5,
            value: "Hello".to_string(),
            font: "System".to_string(),
            size: 24.0 / 720.0, // Fraction of render_height (escala con la resolución)
            color: [255, 255, 255, 255],
            spans: Vec::new(),
            spawn_time: 0.0,
            kill_time: None,  // Initialize kill_time
            ephemeral: false, // Initialize ephemeral
            z_index: 0,
            animations: Vec::new(),
            visible: true,
        }
    }
}

impl ShapeDescriptor for Text {
    fn dsl_keyword(&self) -> &'static str {
        "text"
    }
    fn icon(&self) -> &'static str {
        "📝"
    }

    fn draw_modifiers(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Name:");
                if ui.text_edit_singleline(&mut self.name).changed() {
                    changed = true;
                }
            });

            if ui.checkbox(&mut self.visible, "Visible").changed() {
                changed = true;
            }

            ui.horizontal(|ui| {
                ui.label("Text:");
                if ui.text_edit_singleline(&mut self.value).changed() {
                    changed = true;
                }
            });

            let mut size_pct = self.size * 100.0;
            if ui
                .add(
                    egui::Slider::new(&mut size_pct, 0.1..=50.0)
                        .suffix("%")
                        .text("Size"),
                )
                .changed()
            {
                self.size = size_pct / 100.0;
                changed = true;
            }

            if ui
                .add(egui::Slider::new(&mut self.x, 0.0..=1.0).text("X"))
                .changed()
            {
                changed = true;
            }
            if ui
                .add(egui::Slider::new(&mut self.y, 0.0..=1.0).text("Y"))
                .changed()
            {
                changed = true;
            }

            ui.horizontal(|ui| {
                ui.label("Font:");
                let mut selected_font = self.font.clone();
                egui::ComboBox::from_id_source("font_selector_sidebar")
                    .selected_text(&selected_font)
                    .width(ui.available_width() - 40.0)
                    .show_ui(ui, |ui| {
                        for font_name in &state.available_fonts {
                            let f_fam = egui::FontFamily::Name(font_name.clone().into());
                            let is_bound = ui
                                .ctx()
                                .fonts(|f| f.families().iter().any(|fam| fam == &f_fam));
                            let text = if is_bound {
                                egui::RichText::new(font_name).family(f_fam)
                            } else {
                                egui::RichText::new(font_name)
                            };
                            if ui
                                .selectable_value(&mut selected_font, font_name.clone(), text)
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });
                self.font = selected_font;
            });

            // Preview Area
            let preview_fam = if self.font == "System" || self.font.is_empty() {
                egui::FontFamily::Proportional
            } else {
                let f_name = egui::FontFamily::Name(self.font.clone().into());
                let is_bound = ui
                    .ctx()
                    .fonts(|f| f.families().iter().any(|fam| fam == &f_name));
                if is_bound {
                    f_name
                } else {
                    egui::FontFamily::Proportional
                }
            };
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.painter().rect_filled(
                    ui.available_rect_before_wrap(),
                    4.0,
                    egui::Color32::from_black_alpha(40),
                );
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("AaBb 123")
                            .font(egui::FontId::new(18.0, preview_fam))
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.add_space(4.0);
                });
            });
            ui.add_space(4.0);

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
                    changed = true;
                }
            });

            if ui
                .add(
                    egui::DragValue::new(&mut self.spawn_time)
                        .speed(0.1)
                        .prefix("Spawn: "),
                )
                .changed()
            {
                changed = true;
            }
            // optional kill time
            ui.horizontal(|ui| {
                let mut k = self.kill_time.unwrap_or(f32::NAN);
                let ch = ui
                    .add(egui::DragValue::new(&mut k).speed(0.1).prefix("Kill: "))
                    .changed();
                if ch {
                    if k.is_nan() {
                        self.kill_time = None;
                    } else {
                        self.kill_time = Some(k);
                    }
                    changed = true;
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Rich Spans").strong());
                if ui.button("➕").clicked() {
                    self.spans.push(TextSpan {
                        text: "new".to_string(),
                        font: self.font.clone(),
                        size: self.size,
                        color: self.color,
                    });
                    changed = true;
                }
            });

            let mut to_remove = None;
            for (i, span) in self.spans.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        if ui.text_edit_singleline(&mut span.text).changed() {
                            changed = true;
                        }
                        if ui.button("🗑").clicked() {
                            to_remove = Some(i);
                        }
                    });

                    ui.horizontal(|ui| {
                        let mut s_font = span.font.clone();
                        egui::ComboBox::from_id_source(format!("sidebar_font_{}", i))
                            .selected_text(&s_font)
                            .width(80.0)
                            .show_ui(ui, |ui| {
                                for f in &state.available_fonts {
                                    if ui.selectable_value(&mut s_font, f.clone(), f).changed() {
                                        changed = true;
                                    }
                                }
                            });
                        span.font = s_font;

                        let mut span_pct = span.size * 100.0;
                        if ui
                            .add(
                                egui::DragValue::new(&mut span_pct)
                                    .speed(0.1)
                                    .clamp_range(0.1..=50.0)
                                    .suffix("%"),
                            )
                            .changed()
                        {
                            span.size = span_pct / 100.0;
                            changed = true;
                        }

                        let mut color_f32 = [
                            span.color[0] as f32 / 255.0,
                            span.color[1] as f32 / 255.0,
                            span.color[2] as f32 / 255.0,
                            span.color[3] as f32 / 255.0,
                        ];
                        if ui
                            .color_edit_button_rgba_unmultiplied(&mut color_f32)
                            .changed()
                        {
                            span.color = [
                                (color_f32[0] * 255.0) as u8,
                                (color_f32[1] * 255.0) as u8,
                                (color_f32[2] * 255.0) as u8,
                                (color_f32[3] * 255.0) as u8,
                            ];
                            changed = true;
                        }
                    });
                });
            }

            if let Some(i) = to_remove {
                self.spans.remove(i);
                changed = true;
            }
        });

        if changed {
            state.request_dsl_update();
        }
    }

    fn to_dsl(&self, indent: &str) -> String {
        let mut out = format!(
            "{}text \"{}\" {{\n{}\tx = {:.3},\n{}\ty = {:.3},\n{}\tvalue = \"{}\",\n{}\tfont = \"{}\",\n{}\tsize = {:.4},\n{}\tfill = \"#{:02x}{:02x}{:02x}\",\n{}\tspawn = {:.2},\n",
            indent,
            self.name,
            indent,
            self.x,
            indent,
            self.y,
            indent,
            self.value.replace('"', "\\\""),
            indent,
            self.font,
            indent,
            self.size,
            indent,
            self.color[0],
            self.color[1],
            self.color[2],
            indent,
            self.spawn_time
        );
        if let Some(k) = self.kill_time {
            out.push_str(&format!("{}\tkill = {:.2},\n", indent, k));
        }
        if !self.spans.is_empty() {
            out.push_str(&format!("{}\tspans = [\n", indent));
            for span in &self.spans {
                out.push_str(&format!(
                    "{}\t\tspan(\"{}\", font=\"{}\", size={:.4}, fill=\"#{:02x}{:02x}{:02x}\"),\n",
                    indent,
                    span.text.replace('"', "\\\""),
                    span.font,
                    span.size,
                    span.color[0],
                    span.color[1],
                    span.color[2]
                ));
            }
            out.push_str(&format!("{}\t]\n", indent));
        }

        out.push_str(&format!("{}}}\n", indent));
        out
    }

    fn create_default(name: String) -> super::shapes_manager::Shape {
        let mut t = Self::default();
        t.name = name;
        super::shapes_manager::Shape::Text(t)
    }

    fn to_element_keyframes(&self, fps: u32) -> crate::shapes::element_store::ElementKeyframes {
        use crate::shapes::element_store::{ElementKeyframes, Keyframe};
        let mut ek = ElementKeyframes::new(self.name.clone(), "text".into());
        let spawn = crate::shapes::element_store::seconds_to_frame(self.spawn_time, fps);
        ek.spawn_frame = spawn;
        ek.kill_frame = self.kill_time.map(|k| crate::shapes::element_store::seconds_to_frame(k, fps));
        ek.x.push(Keyframe { frame: spawn, value: self.x, easing: crate::animations::easing::Easing::Linear });
        ek.y.push(Keyframe { frame: spawn, value: self.y, easing: crate::animations::easing::Easing::Linear });
        ek.size.push(Keyframe { frame: spawn, value: self.size, easing: crate::animations::easing::Easing::Linear });
        ek.value.push(Keyframe { frame: spawn, value: self.value.clone(), easing: crate::animations::easing::Easing::Linear });
        ek.color.push(Keyframe { frame: spawn, value: self.color, easing: crate::animations::easing::Easing::Linear });
        ek.visible.push(Keyframe { frame: spawn, value: self.visible, easing: crate::animations::easing::Easing::Linear });
        ek.z_index.push(Keyframe { frame: spawn, value: self.z_index, easing: crate::animations::easing::Easing::Linear });
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
        if orig.and_then(|p| p.size).unwrap_or(f32::NAN) != self.size {
            new_props.size = Some(self.size);
        }
        if orig.and_then(|p| p.value.clone()) != Some(self.value.clone()) {
            new_props.value = Some(self.value.clone());
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
            "size" => self.size = value,
            "spawn" => self.spawn_time = value,
            "kill" => self.kill_time = Some(value),
            _ => {}
        }
    }

    fn apply_kv_string(&mut self, key: &str, val: &str) {
        match key {
            "name" => self.name = val.to_string(),
            "value" => self.value = val.to_string(),
            "font" => self.font = val.to_string(),
            _ => {}
        }
    }

    fn animations(&self) -> &[crate::scene::Animation] {
        &self.animations
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
        // Use sampled Shape values so animated text (position/size/color)
        // is reflected in the GPU primitive. UVs will be populated by the
        // text rasterizer later when composing the atlas.
        if let crate::scene::Shape::Text(t) = scene_shape {
            if !t.visible {
                return;
            }
            let (x, y) = crate::animations::animations_manager::animated_xy_for(
                scene_shape,
                current_time,
                duration,
            );

            let sz_px = t.size * rh; // size is fraction of render height
            let x_px = x * rw;
            let y_px = y * rh;

            out.push(crate::canvas::gpu::GpuShape {
                pos: [x_px, y_px],
                size: [sz_px, sz_px],
                color: [
                    crate::canvas::gpu::srgb_to_linear(t.color[0]),
                    crate::canvas::gpu::srgb_to_linear(t.color[1]),
                    crate::canvas::gpu::srgb_to_linear(t.color[2]),
                    t.color[3] as f32 / 255.0,
                ],
                shape_type: 2,
                spawn_time: spawn,
                p1: 0,
                p2: 0,
                uv0: [0.0, 0.0],
                uv1: [0.0, 0.0],
            });
        }
    }
}

/// Reconstruct a `Shape::Text` from `ElementKeyframes` sampled at `frame`.
pub fn from_element_keyframes(
    ek: &crate::shapes::element_store::ElementKeyframes,
    frame: crate::shapes::element_store::FrameIndex,
    fps: u32,
) -> Option<super::shapes_manager::Shape> {
    let props = ek.sample(frame)?;
    let mut t = Text::default();
    t.name = ek.name.clone();
    if let Some(x) = props.x {
        t.x = x;
    }
    if let Some(y) = props.y {
        t.y = y;
    }
    if let Some(sz) = props.size {
        t.size = sz;
    }
    if let Some(val) = props.value.clone() {
        t.value = val;
    }
    if let Some(col) = props.color {
        t.color = col;
    }
    if let Some(v) = props.visible {
        t.visible = v;
    }
    if let Some(z) = props.z_index {
        t.z_index = z;
    }
    t.spawn_time = frame as f32 / fps as f32;
    if let Some(kf) = ek.kill_frame {
        t.kill_time = Some(kf as f32 / fps as f32);
    }
    t.ephemeral = ek.ephemeral;
    t.animations = ek.animations.clone();
    Some(super::shapes_manager::Shape::Text(t))
}

inventory::submit! {
    crate::shapes::shapes_manager::ElementKeyframesFactory {
        kind: "text",
        constructor: crate::shapes::text::from_element_keyframes,
    }
}
