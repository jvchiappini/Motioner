/// Dibuja la cuadrícula de fondo y los ejes del canvas.

use eframe::egui;

/// Dibuja la cuadrícula infinita basada en el nivel de zoom actual.
pub fn draw_grid(painter: &egui::Painter, rect: egui::Rect, zoom: f32, pan: egui::Vec2) {
    let center = rect.center();
    let mut base_step = 100.0;
    while base_step * zoom > 200.0 { base_step /= 10.0; }
    while base_step * zoom < 20.0 { base_step *= 10.0; }

    let step = base_step * zoom;
    let grid_origin = center + pan;

    let start_x = rect.left() + (grid_origin.x - rect.left()) % step - step;
    let start_y = rect.top() + (grid_origin.y - rect.top()) % step - step;

    let grid_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 40));
    let origin_stroke_x = egui::Stroke::new(2.0, egui::Color32::from_rgb(150, 50, 50));
    let origin_stroke_y = egui::Stroke::new(2.0, egui::Color32::from_rgb(50, 150, 50));

    // Líneas verticales
    let mut x = start_x;
    while x <= rect.right() + step {
        if x >= rect.left() {
            painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], grid_stroke);
        }
        x += step;
    }

    // Líneas horizontales
    let mut y = start_y;
    while y <= rect.bottom() + step {
        if y >= rect.top() {
            painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], grid_stroke);
        }
        y += step;
    }

    // Dibujar ejes principales
    if grid_origin.x >= rect.left() && grid_origin.x <= rect.right() {
        painter.line_segment([egui::pos2(grid_origin.x, rect.top()), egui::pos2(grid_origin.x, rect.bottom())], origin_stroke_y);
    }
    if grid_origin.y >= rect.top() && grid_origin.y <= rect.bottom() {
        painter.line_segment([egui::pos2(rect.left(), grid_origin.y), egui::pos2(rect.right(), grid_origin.y)], origin_stroke_x);
    }
}
