//! Gestiona el rasterizado de formas en el canvas.
//! Actualmente sirve como punto de entrada para el muestreo de color.

use crate::app_state::AppState;
use eframe::egui;

/// Muestrea el color en una coordenada normalizada (0..1) del papel.
/// Respeta la resolución de vista previa y el orden de las formas.
pub fn sample_color_at(_state: &AppState, _paper_uv: egui::Pos2, _time: f32) -> [u8; 4] {
    // PREVIEW RENDERING: disabled — return plain white for preview/color-picker.
    // TODO: reimplement the preview rasterizer (use ElementKeyframes for sampling).
    [255u8, 255u8, 255u8, 255u8]
}
