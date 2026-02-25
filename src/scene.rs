use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Easing {
    Linear,
    Step,
    EaseIn { power: f32 },
    EaseOut { power: f32 },
    EaseInOut { power: f32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Animation {
    Move {
        to_x: f32,
        to_y: f32,
        start: f32,
        end: f32,
        easing: Easing,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Shape {
    Rect {
        name: String,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 4],
    },
}

impl Shape {
    pub fn name(&self) -> &str {
        match self {
            Shape::Rect { name, .. } => name,
        }
    }

    pub fn to_dsl(&self, indent: &str) -> String {
        match self {
            Shape::Rect {
                name,
                x,
                y,
                w,
                h,
                color,
            } => {
                format!(
                    "{}rect \"{}\" {{\n\t{}x = {:.3},\n\t{}y = {:.3},\n\t{}w = {:.3},\n\t{}h = {:.3},\n\t{}color = \"#{:02x}{:02x}{:02x}\",\n{}}}\n",
                    indent, name, indent, x, indent, y, indent, w, indent, h, indent, color[0], color[1], color[2], indent
                )
            }
        }
    }
}
