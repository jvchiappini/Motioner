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
    // input buffer for duration editing in settings
    #[serde(skip)]
    pub duration_input_buffer: String,
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
    /// Cached preview frames around the current playhead (time, image)
    #[serde(skip)]
    pub preview_frame_cache: Vec<(f32, egui::ColorImage)>,
    /// When GPU previews are enabled we may store cached preview frames as
    /// textures (VRAM) to avoid holding large ColorImage buffers in RAM.
    #[serde(skip)]
    pub preview_texture_cache: Vec<(f32, egui::TextureHandle, usize)>,
    /// VRAM disponible estimada (en bytes)
    #[serde(skip)]
    pub estimated_vram_bytes: usize,
    /// Si true, prioriza cache en VRAM sobre RAM
    pub prefer_vram_cache: bool,
    /// Máximo porcentaje de VRAM a usar para cache (default: 50%)
    pub vram_cache_max_percent: f32,
    /// Optional compressed preview cache (PNG bytes) used when
    /// `compress_preview_cache` is enabled and GPU texture caching is not used.
    #[serde(skip)]
    pub preview_compressed_cache: Vec<(f32, Vec<u8>, (usize, usize))>,
    /// Center time of the cached preview frames, if any
    #[serde(skip)]
    pub preview_cache_center_time: Option<f32>,
    /// Background preview worker channels (job sender + result receiver)
    #[serde(skip)]
    pub preview_worker_tx: Option<std::sync::mpsc::Sender<crate::canvas::PreviewJob>>,
    #[serde(skip)]
    pub preview_worker_rx: Option<std::sync::mpsc::Receiver<crate::canvas::PreviewResult>>,
    /// True when a background preview generation job was issued and not yet observed in results.
    #[serde(skip)]
    pub preview_job_pending: bool,
    /// If true the background preview worker will attempt to use a headless GPU
    /// renderer for faster preview generation; when false the worker always
    /// uses the CPU rasterizer. Exposed in Settings → Performance.
    pub preview_worker_use_gpu: bool,
    /// Automatically trim preview caches when they exceed `preview_cache_max_mb`.
    pub preview_cache_auto_clean: bool,
    /// Maximum preview cache size (MB) before warning/auto-clean triggers.
    pub preview_cache_max_mb: usize,
    /// If true, compress preview frames in RAM (PNG) to reduce working set.
    pub compress_preview_cache: bool,

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
    /// Last computed composition (paper) rect in screen coordinates. Used to
    /// position modals (e.g. Project Settings) over the canvas.
    #[serde(skip)]
    pub last_composition_rect: Option<egui::Rect>,

    /// Optional precomputed position cache (per-frame, flattened scene).
    /// When present, rendering uses these precomputed positions instead of
    /// re-evaluating animations on every paint. Cleared when the scene or
    /// timing changes.
    #[serde(skip)]
    pub position_cache: Option<crate::canvas::PositionCache>,

    /// If a background position-cache build is running, this is true.
    #[serde(skip)]
    pub position_cache_build_in_progress: bool,
    /// Receiver for background-built PositionCache (one-shot)
    #[serde(skip)]
    pub position_cache_build_rx: Option<std::sync::mpsc::Receiver<crate::canvas::PositionCache>>,

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

    // Animations modal: selected top-level element index (temporary UI state)
    pub anim_modal_target_idx: usize,

    pub modifier_active_path: Option<Vec<usize>>,

    // UI: Elements modal (floating palette from Scene Graph)
    pub show_elements_modal: bool,
    // UI: Animations modal (floating palette from Scene Graph)
    pub show_animations_modal: bool,

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
            preview_frame_cache: Vec::new(),
            preview_texture_cache: Vec::new(),
            estimated_vram_bytes: 0,     // Se detecta en runtime
            prefer_vram_cache: true,     // Por defecto usar VRAM
            vram_cache_max_percent: 0.6, // Usar hasta 60% de VRAM para caché (3.6GB en RTX 4050)
            preview_compressed_cache: Vec::new(),
            preview_cache_center_time: None,
            preview_worker_tx: None,
            preview_worker_rx: None,
            preview_job_pending: false,
            preview_worker_use_gpu: true,
            preview_cache_auto_clean: true,
            preview_cache_max_mb: 512,
            compress_preview_cache: false,
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
            last_composition_rect: None,
            position_cache: None,
            position_cache_build_in_progress: false,
            position_cache_build_rx: None,
            settings_open_time: None,
            settings_is_closing: false,
            duration_input_buffer: "5.0".to_string(), // Initialize with default duration
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
            anim_modal_target_idx: 0,
            show_elements_modal: false,
            show_animations_modal: false,
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
