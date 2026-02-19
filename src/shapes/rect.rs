use crate::app_state::AppState;
use crate::shapes::ShapeDescriptor;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
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
}
