use crate::app_state::AppState;
use crate::shapes::ShapeDescriptor;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
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
}
