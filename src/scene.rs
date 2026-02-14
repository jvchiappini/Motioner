// Scene model: shapes and helpers
#[derive(Clone, Debug)]
pub enum Shape {
    Circle {
        x: f32,
        y: f32,
        radius: f32,
        color: [u8; 4],
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 4],
    },
}

impl Shape {
    pub fn to_dsl(&self) -> String {
        match self {
            Shape::Circle { x, y, radius, color } => format!(
                "circle(x = {:.3}, y = {:.3}, radius = {:.1}, fill = \"#{:02x}{:02x}{:02x}\")",
                x, y, radius, color[0], color[1], color[2]
            ),
            Shape::Rect { x, y, w, h, color } => format!(
                "rect(x = {:.3}, y = {:.3}, width = {:.1}, height = {:.1}, fill = \"#{:02x}{:02x}{:02x}\")",
                x, y, w, h, color[0], color[1], color[2]
            ),
        }
    }

    pub fn sample_scene() -> Vec<Shape> {
        vec![Shape::Circle {
            x: 0.1,
            y: 0.5,
            radius: 60.0,
            color: [120, 200, 255, 255],
        }]
    }
}

pub type Scene = Vec<Shape>;
