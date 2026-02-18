use serde::{Deserialize, Serialize};
use crate::shapes::ShapeDescriptor;
use crate::app_state::AppState;
use eframe::egui;

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
    pub value: String, // Default/Legacy
    pub font: String,  // Default/Legacy
    pub size: f32,    // Default/Legacy
    pub color: [u8; 4], // Default/Legacy
    pub spans: Vec<TextSpan>,
    pub spawn_time: f32,
    #[serde(default)]
    pub animations: Vec<crate::scene::Animation>,
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
            size: 24.0,
            color: [255, 255, 255, 255],
            spans: Vec::new(),
            spawn_time: 0.0,
            animations: Vec::new(),
            visible: true,
        }
    }
}

impl ShapeDescriptor for Text {
    fn dsl_keyword(&self) -> &'static str { "text" }
    fn icon(&self) -> &'static str { "📝" }
    
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
            
            if ui.add(egui::Slider::new(&mut self.size, 1.0..=200.0).text("Size")).changed() {
                changed = true;
            }

            if ui.add(egui::Slider::new(&mut self.x, 0.0..=1.0).text("X")).changed() {
                changed = true;
            }
            if ui.add(egui::Slider::new(&mut self.y, 0.0..=1.0).text("Y")).changed() {
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
                            let is_bound = ui.ctx().fonts(|f| f.families().iter().any(|fam| fam == &f_fam));
                            let text = if is_bound {
                                egui::RichText::new(font_name).family(f_fam)
                            } else {
                                egui::RichText::new(font_name)
                            };
                            if ui.selectable_value(&mut selected_font, font_name.clone(), text).changed() {
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
                let is_bound = ui.ctx().fonts(|f| f.families().iter().any(|fam| fam == &f_name));
                if is_bound { f_name } else { egui::FontFamily::Proportional }
            };
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.painter().rect_filled(ui.available_rect_before_wrap(), 4.0, egui::Color32::from_black_alpha(40));
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("AaBb 123").font(egui::FontId::new(18.0, preview_fam)).color(egui::Color32::LIGHT_GRAY));
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
                if ui.color_edit_button_rgba_unmultiplied(&mut color_f32).changed() {
                    self.color = [
                        (color_f32[0] * 255.0) as u8,
                        (color_f32[1] * 255.0) as u8,
                        (color_f32[2] * 255.0) as u8,
                        (color_f32[3] * 255.0) as u8,
                    ];
                    changed = true;
                }
            });
            
            if ui.add(egui::DragValue::new(&mut self.spawn_time).speed(0.1).prefix("Spawn: ")).changed() {
                changed = true;
            }

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
                        
                        if ui.add(egui::DragValue::new(&mut span.size).speed(1.0).clamp_range(1.0..=500.0)).changed() {
                            changed = true;
                        }

                        let mut color_f32 = [
                            span.color[0] as f32 / 255.0,
                            span.color[1] as f32 / 255.0,
                            span.color[2] as f32 / 255.0,
                            span.color[3] as f32 / 255.0,
                        ];
                        if ui.color_edit_button_rgba_unmultiplied(&mut color_f32).changed() {
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
            "{}text \"{}\" {{\n{}    x = {:.3},\n{}    y = {:.3},\n{}    value = \"{}\",\n{}    font = \"{}\",\n{}    size = {:.1},\n{}    fill = \"#{:02x}{:02x}{:02x}\",\n{}    spawn = {:.2},\n",
            indent, self.name, indent, self.x, indent, self.y, indent, self.value.replace('"', "\\\""), indent, self.font, indent, self.size, indent, self.color[0], self.color[1], self.color[2], indent, self.spawn_time
        );
        
        if !self.spans.is_empty() {
            out.push_str(&format!("{}    spans = [\n", indent));
            for span in &self.spans {
                out.push_str(&format!(
                    "{}        span(\"{}\", font=\"{}\", size={:.1}, fill=\"#{:02x}{:02x}{:02x}\"),\n",
                    indent, span.text.replace('"', "\\\""), span.font, span.size, span.color[0], span.color[1], span.color[2]
                ));
            }
            out.push_str(&format!("{}    ]\n", indent));
        }
        
        out.push_str(&format!("{}}}\n", indent));
        out
    }

    fn create_default(name: String) -> super::shapes_manager::Shape {
        let mut t = Self::default();
        t.name = name;
        super::shapes_manager::Shape::Text(t)
    }
}
