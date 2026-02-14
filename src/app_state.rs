use crate::scene::{Scene, Shape};
use std::path::PathBuf;
use std::sync::{atomic::AtomicUsize, Arc};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Pid, RefreshKind, System};

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum PanelTab {
    SceneGraph,
    Code,
}

pub struct AppState {
    pub fps: u32,
    pub duration_secs: f32,
    // final render resolution (pixels)
    pub render_width: u32,
    pub render_height: u32,
    pub preview_scale: u32,
    pub playing: bool,
    pub time: f32,
    pub export_in_progress: bool,
    pub export_progress: Arc<AtomicUsize>,
    pub last_export_path: Option<PathBuf>,

    pub scene: Scene,
    pub selected: Option<usize>,
    pub show_dsl: bool,
    pub show_settings: bool,
    pub active_tab: Option<PanelTab>,
    pub last_active_tab: PanelTab, // To keep track of content during closing animation
    pub transition_source_tab: Option<PanelTab>, // For switching animation
    pub tab_switch_time: Option<f64>,
    pub dsl_code: String, // Added this field

    // System monitoring
    pub system: System,
    pub pid: Pid,
    pub last_update: f32, // to throttle updates

    // Timeline state
    pub timeline_zoom: f32,  // pixels per second
    pub timeline_pan_x: f32, // horizontal scroll offset (pixels)
    pub timeline_pan_y: f32, // vertical scroll offset (pixels)

    // UI Animation State
    pub settings_open_time: Option<f64>,
    pub settings_is_closing: bool,

    // Toast Notification State
    pub toast_message: Option<String>,
    pub toast_type: ToastType, // "error" or "success", handled as enum or string
    pub toast_deadline: f64,   // Time when toast should disappear

    // Change tracking
    pub last_synced_settings: (u32, f32, u32, u32),

    // Fullscreen Code Editor
    pub code_fullscreen: bool,
    pub code_anim_t: f32, // Manually tracked animation value 0.0 to 1.0

    // Autocomplete State
    pub completion_popup_open: bool,
    pub completion_cursor_idx: usize,
    pub completion_items: Vec<String>,
    pub completion_selected_index: usize,

    // Project State
    pub project_path: Option<PathBuf>,
    pub project_path_input: String,
    pub path_validation_error: Option<String>,
    pub show_welcome: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastType {
    Info,
    Error,
    Success,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            fps: 30,
            duration_secs: 4.0,
            render_width: 1280,
            render_height: 720,
            preview_scale: 100,
            playing: false,
            time: 0.0,
            export_in_progress: false,
            export_progress: Arc::new(AtomicUsize::new(0)),
            last_export_path: None,
            scene: Shape::sample_scene(),
            selected: Some(0),
            show_dsl: true,
            show_settings: false,
            active_tab: Option::None,
            last_active_tab: PanelTab::SceneGraph, // Default
            transition_source_tab: None,
            tab_switch_time: None,
            dsl_code: String::new(),
            system: System::new_with_specifics(
                RefreshKind::new()
                    .with_cpu(CpuRefreshKind::everything())
                    .with_memory(MemoryRefreshKind::everything())
                    .with_processes(sysinfo::ProcessRefreshKind::everything()),
            ),
            pid: sysinfo::get_current_pid().expect("Failed to get PID"),
            last_update: 0.0,
            timeline_zoom: 100.0,
            timeline_pan_x: 0.0,
            timeline_pan_y: 0.0,
            settings_open_time: None,
            settings_is_closing: false,
            toast_message: None,
            toast_type: ToastType::Info,
            toast_deadline: 0.0,
            last_synced_settings: (30, 4.0, 1280, 720),
            code_fullscreen: false,
            code_anim_t: 0.0,
            completion_popup_open: false,
            completion_cursor_idx: 0,
            completion_items: Vec::new(),
            completion_selected_index: 0,

            project_path: None,
            project_path_input: String::new(),
            path_validation_error: None,
            show_welcome: true,
        }
    }
}
