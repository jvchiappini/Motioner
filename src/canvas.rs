use crate::app_state::AppState;
use eframe::egui;

pub mod ui;
pub use ui::show;

pub fn poll_preview_results(_state: &mut AppState, _ctx: &egui::Context) {}
pub fn request_preview_frames(_state: &mut AppState, _time: f32) {}
