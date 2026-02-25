use crate::states::autosave::AutosaveState;
use crate::states::dslstate::DslState;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::scene::Shape;

#[derive(PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum PanelTab {
    SceneGraph,
    Code,
}

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub fps: u32,
    pub duration_secs: f32,
    pub render_width: u32,
    pub render_height: u32,

    pub playing: bool,
    pub time: f32,
    pub dsl: DslState,
    pub dsl_code: String,

    pub scene: Vec<Shape>,
    pub selected: Option<usize>,
    pub selected_node_path: Option<Vec<usize>>,
    
    pub active_tab: Option<PanelTab>,
    pub last_active_tab: PanelTab,

    #[serde(skip)]
    pub autosave: AutosaveState,
    #[serde(skip)]
    pub last_scene_parse_time: f64,

    // Canvas panning/zoom
    pub canvas_pan_x: f32,
    pub canvas_pan_y: f32,
    pub canvas_zoom: f32,

    // Timeline panning/zoom
    pub timeline_pan_x: f32,
    pub timeline_pan_y: f32,
    pub timeline_zoom: f32,
    
    pub timeline_root_path: Option<Vec<usize>>,
    pub timeline_prev_root_path: Option<Vec<usize>>,
    pub timeline_breadcrumb_anim_t: f32,

    pub scene_version: u32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            fps: 60,
            duration_secs: 5.0,
            render_width: 1280,
            render_height: 720,
            playing: false,
            time: 0.0,
            dsl: DslState::default(),
            dsl_code: String::new(),
            scene: Vec::new(),
            selected: None,
            selected_node_path: None,
            active_tab: Some(PanelTab::Code),
            last_active_tab: PanelTab::Code,
            autosave: AutosaveState::default(),
            last_scene_parse_time: 0.0,
            canvas_pan_x: 0.0,
            canvas_pan_y: 0.0,
            canvas_zoom: 1.0,
            timeline_pan_x: 0.0,
            timeline_pan_y: 0.0,
            timeline_zoom: 100.0,
            timeline_root_path: None,
            timeline_prev_root_path: None,
            timeline_breadcrumb_anim_t: 1.0,
            scene_version: 1,
        }
    }
}

impl AppState {
    pub fn initialize_with_context(&mut self, _cc: &eframe::CreationContext<'_>) {}

    pub fn tick(&mut self, now: f64) -> bool {
        crate::states::autosave::tick(self, now);
        self.debounced_parse(now)
    }

    pub fn debounced_parse(&mut self, now: f64) -> bool {
        let parse_debounce = 0.12_f64;
        if let Some(last_edit) = self.autosave.last_edit_time {
            if now - last_edit > parse_debounce && now - self.last_scene_parse_time > parse_debounce {
                if let Ok(config) = crate::dsl::parse_config(&self.dsl_code) {
                    self.fps = config.fps;
                    self.duration_secs = config.duration;
                    self.render_width = config.width;
                    self.render_height = config.height;
                }

                self.scene = crate::dsl::parse_dsl(&self.dsl_code);
                self.scene_version += 1;
                self.last_scene_parse_time = now;
                return true;
            }
        }
        false
    }

    pub fn set_time(&mut self, seconds: f32) {
        self.time = seconds.clamp(0.0, self.duration_secs);
    }

    pub fn request_dsl_update(&mut self) {
        // Simple mock for now if needed
    }
}
