use crate::scene::{Scene, Shape};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{atomic::AtomicUsize, Arc};
use sysinfo::{Pid, System};

#[derive(PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum PanelTab {
    SceneGraph,
    Code,
}

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub fps: u32,
    pub duration_secs: f32,
    // final render resolution (pixels)
    pub render_width: u32,
    pub render_height: u32,
    pub preview_multiplier: f32, // Multiplier: 0.125, 0.25, 0.5, 1.0, etc.
    pub preview_fps: u32,

    #[cfg(feature = "wgpu")]
    #[serde(skip)]
    pub wgpu_renderer: Option<std::sync::Arc<egui::mutex::Mutex<crate::canvas::GpuResources>>>,

    #[serde(skip)]
    pub preview_texture: Option<egui::TextureHandle>,

    pub playing: bool,
    pub time: f32,
    pub export_in_progress: bool,
    #[serde(skip, default = "default_progress")]
    pub export_progress: Arc<AtomicUsize>,
    pub last_export_path: Option<PathBuf>,

    pub scene: Scene,
    pub selected: Option<usize>,
    /// Selected node path in the scene graph. Example: `[0]` (top-level), `[0,2]` (child index 2 of top 0)
    pub selected_node_path: Option<Vec<usize>>,
    pub show_dsl: bool,
    pub show_settings: bool,
    pub active_tab: Option<PanelTab>,
    pub last_active_tab: PanelTab, // To keep track of content during closing animation
    pub transition_source_tab: Option<PanelTab>, // For switching animation
    pub tab_switch_time: Option<f64>,
    pub dsl_code: String, // Added this field

    /// Move operation: (from_path, to_parent_path, to_index)
    pub move_request: Option<(Vec<usize>, Vec<usize>, usize)>,

    // System monitoring
    #[serde(skip, default = "System::new")]
    pub system: System,
    #[serde(skip, default = "default_pid")]
    pub pid: Pid,
    pub last_update: f32, // to throttle updates

    // Timeline state
    pub timeline_zoom: f32,  // pixels per second
    pub timeline_pan_x: f32, // horizontal scroll offset (pixels)
    pub timeline_pan_y: f32, // vertical scroll offset (pixels)

    // Canvas state
    pub canvas_zoom: f32,
    pub canvas_pan_x: f32,
    pub canvas_pan_y: f32,

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

    // Color Picker & Magnifier
    pub picker_active: bool,
    pub picker_color: [u8; 4],

    // Drag-and-drop state
    pub drag_start_time: Option<f64>,
    pub potential_drag_path: Option<Vec<usize>>,

    pub renaming_path: Option<Vec<usize>>,
    pub rename_buffer: String,
    // Expanded nodes for scene-graph tree. Keys are path strings like "0", "0.1", "0.1.2"
    pub expanded_nodes: HashSet<String>,

    pub modifier_active_path: Option<Vec<usize>>,

    // UI: Elements modal (floating palette from Scene Graph)
    pub show_elements_modal: bool,

    // UI Sidebar State
    pub sidebar_width: f32,
    /// Previous timeline root path (used for breadcrumb animation)
    #[serde(skip)]
    pub timeline_prev_root_path: Option<Vec<usize>>,
    /// Breadcrumb animation progress (0.0 -> transition start, 1.0 -> settled)
    #[serde(skip)]
    pub timeline_breadcrumb_anim_t: f32,
    /// If set, timeline shows only the children of this scene path (e.g. [2, 1])
    pub timeline_root_path: Option<Vec<usize>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToastType {
    Info,
    Error,
    Success,
}

fn default_progress() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

fn default_pid() -> Pid {
    sysinfo::get_current_pid().unwrap_or(Pid::from(0))
}

impl Default for AppState {
    fn default() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        let pid = sysinfo::get_current_pid().unwrap_or(Pid::from(0));

        Self {
            fps: 60,
            duration_secs: 5.0,
            render_width: 1280,
            render_height: 720,
            preview_multiplier: 1.0,
            preview_fps: 60,
            #[cfg(feature = "wgpu")]
            wgpu_renderer: None,
            preview_texture: None,
            playing: false,
            time: 0.0,
            export_in_progress: false,
            export_progress: Arc::new(AtomicUsize::new(0)),
            last_export_path: None,
            scene: Shape::sample_scene(),
            selected: None,
            selected_node_path: None,
            show_dsl: false,
            show_settings: false,
            active_tab: None,
            last_active_tab: PanelTab::SceneGraph,
            transition_source_tab: None,
            tab_switch_time: None,
            dsl_code: String::new(),
            move_request: None,
            system,
            pid,
            last_update: 0.0,
            timeline_zoom: 100.0,
            timeline_pan_x: 0.0,
            timeline_pan_y: 0.0,
            canvas_zoom: 1.0,
            canvas_pan_x: 0.0,
            canvas_pan_y: 0.0,
            settings_open_time: None,
            settings_is_closing: false,
            toast_message: None,
            toast_type: ToastType::Info,
            toast_deadline: 0.0,
            last_synced_settings: (60, 5.0, 1280, 720),
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
            picker_active: false,
            picker_color: [255, 255, 255, 255],
            drag_start_time: None,
            potential_drag_path: None,
            renaming_path: None,
            rename_buffer: String::new(),
            expanded_nodes: HashSet::new(),
            modifier_active_path: None,
            show_elements_modal: false,
            sidebar_width: 250.0,
            timeline_root_path: None,
            timeline_prev_root_path: None,
            timeline_breadcrumb_anim_t: 1.0,
        }
    }
}

#[cfg(feature = "wgpu")]
pub struct WgpuRenderer {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    // We'll store a buffer for shapes
}
