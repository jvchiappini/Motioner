use crate::shapes::element_store::ElementKeyframes;
// Separate module to encapsulate autosave-related flags/timestamps.
use crate::states::autosave::AutosaveState;
use crate::states::dslstate::DslState;
use eframe::egui; // bring Pos2/Rect etc into scope for resize helpers
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use sysinfo::{Pid, System};

// Alias used for font refresh worker messages to reduce type complexity.
type FontRefreshMsg = (Vec<String>, std::collections::HashMap<String, PathBuf>);

#[derive(PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum PanelTab {
    SceneGraph,
    Code,
}

/// Small helper representing a completion suggestion and the text to insert.
#[derive(Clone, Debug)]
pub struct CompletionItem {
    pub label: String,
    pub insert_text: String,
    pub is_snippet: bool,
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
    pub wgpu_render_state: Option<eframe::egui_wgpu::RenderState>,

    /// Texture handle containing the application logo (loaded from SVG asset).
    #[serde(skip)]
    pub logo_texture: Option<egui::TextureHandle>,
    #[serde(skip)]
    pub preview_native_texture_id: Option<egui::TextureId>,
    #[serde(skip)]
    #[cfg(feature = "wgpu")]
    pub preview_native_texture_resource: Option<std::sync::Arc<wgpu::Texture>>,
    /// GPU cache of nearby preview frames.  Each entry holds an egui texture
    /// id and a shared reference to the wgpu texture so the resource remains
    /// alive while cached.
    #[serde(skip)]
    pub preview_gpu_cache: Vec<(f32, egui::TextureId, std::sync::Arc<wgpu::Texture>)>,
    // cache fields removed: frame_cache, texture_cache, estimated_vram_bytes,
    // prefer_vram_cache, vram_cache_max_percent, preview_compressed_cache and
    // preview_cache_center_time were part of the old CPU/RAM caching system.
    /// When true, a preview request should be issued once the editor becomes idle.
    #[serde(skip)]
    pub preview_pending_from_code: bool,
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
    // preview cache maintenance flags removed (CPU caching no longer exists)
    pub playing: bool,
    pub time: f32,
    /// Last observed time-changed event (seconds, frame) — for UI/tests. Not serialized.
    #[serde(skip)]
    pub last_time_changed: Option<(f32, u32)>,
    /// Raw DSL event handler blocks parsed from `dsl_code` (e.g. `on_time { ... }`).
    /// These are runtime-only and not serialized.
    #[serde(skip)]
    /// Transient information derived from the current DSL source.
    pub dsl: DslState,
    pub export_in_progress: bool,
    pub last_export_path: Option<PathBuf>,

    pub scene: Vec<ElementKeyframes>,
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

    #[serde(skip)]
    pub color_picker_data: Option<ColorPickerData>,

    #[serde(skip)]
    pub completion_worker_tx: Option<std::sync::mpsc::Sender<String>>,
    #[serde(skip)]
    pub completion_worker_rx: Option<std::sync::mpsc::Receiver<Vec<CompletionItem>>>,

    // Channels used for off‑thread file/folder dialogs.  The native dialog
    // implementations in `rfd` block the current thread, which when run on
    // the egui UI thread causes the UI to stall for a moment.  To avoid that
    // we spawn the dialog in a background thread and forward the result back
    // through these channels so the UI can remain responsive.
    #[serde(skip)]
    pub folder_dialog_tx: Option<std::sync::mpsc::Sender<PathBuf>>,
    #[serde(skip)]
    pub folder_dialog_rx: Option<std::sync::mpsc::Receiver<PathBuf>>,
    #[serde(skip)]
    pub save_dialog_tx: Option<std::sync::mpsc::Sender<PathBuf>>,
    #[serde(skip)]
    pub save_dialog_rx: Option<std::sync::mpsc::Receiver<PathBuf>>,

    // Font refresh worker channels.  Clicking "Start Creating" kicks off a
    // background scan of system/workspace fonts which can take a noticeable
    // amount of time on slow machines.  We perform the scan on another
    // thread and deliver the results via these channels so the GUI remains
    // responsive while the operation completes.
    #[serde(skip)]
    pub font_refresh_tx: Option<std::sync::mpsc::Sender<FontRefreshMsg>>,
    #[serde(skip)]
    pub font_refresh_rx: Option<std::sync::mpsc::Receiver<FontRefreshMsg>>,

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

    // Position cache removed — previously used for precomputing per-frame positions.

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
    // Structured completion item (label + text to insert). Not serialized.
    #[serde(skip)]
    pub completion_items: Vec<CompletionItem>,
    pub completion_selected_index: usize,

    // Snippet/tabstop state for multi-field completions (e.g. `circle { ... }`).
    #[serde(skip)]
    pub completion_snippet_active: bool,
    #[serde(skip)]
    pub completion_snippet_region: Option<(usize, usize)>, // byte indices in `dsl_code`
    #[serde(skip)]
    pub completion_snippet_params: Vec<(usize, usize)>, // absolute byte ranges of editable params
    #[serde(skip)]
    pub completion_snippet_index: Option<usize>,
    // Last completion query (to debounce suggestion updates)
    #[serde(skip)]
    pub last_completion_query: Option<String>,
    #[serde(skip)]
    pub last_completion_query_time: f64,

    // Autosave / Editor state ------------------------------------------------
    #[serde(skip)]
    pub autosave: AutosaveState,
    /// Time when we last parsed `dsl_code` into `scene` (debounced)
    pub last_scene_parse_time: f64,

    // Project State
    pub project_path: Option<PathBuf>,
    pub project_path_input: String,
    pub path_validation_error: Option<String>,
    pub show_welcome: bool,

    // Color Picker & Magnifier
    pub picker_active: bool,
    pub picker_color: [u8; 4],

    /// When true, dragging an element in the canvas will resize it instead of
    /// just selecting.  The toggle is exposed next to the global color picker.
    pub resize_mode: bool,
    /// Temporary state for an active resize drag operation.  Stored so we can
    /// compute the element centre once when the drag begins and then adjust
    /// width/height/radius each frame as the pointer moves.
    #[serde(skip)]
    pub resize_info: Option<ResizeInfo>,
    /// When true dragging anywhere on an element will move it instead of
    /// selecting (similar to `resize_mode`).  The mini-toolbar exposes a
    /// control for toggling this flag.
    pub move_mode: bool,
    /// Temporary information stored while the user is actively dragging an
    /// element in move mode.
    #[serde(skip)]
    pub move_info: Option<MoveInfo>,

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

    // Export Modal
    pub show_export_modal: bool,
    /// 0 = config confirmation, 1 = ffmpeg progress
    pub export_modal_step: u8,
    pub export_modal_fps: u32,
    pub export_modal_width: u32,
    pub export_modal_height: u32,
    pub export_modal_duration: f32,
    /// Output path chosen by user for ffmpeg export
    #[serde(skip)]
    pub export_output_path: Option<std::path::PathBuf>,
    /// Accumulated ffmpeg log lines
    #[serde(skip)]
    pub export_ffmpeg_log: Vec<String>,
    /// Receiver for ffmpeg status messages from background thread
    #[serde(skip)]
    pub export_ffmpeg_rx: Option<std::sync::mpsc::Receiver<crate::modals::export::FfmpegMsg>>,
    /// Whether the ffmpeg export finished (success / error)
    #[serde(skip)]
    pub export_ffmpeg_done: bool,
    #[serde(skip)]
    pub export_ffmpeg_error: Option<String>,
    /// Cancellation flag shared with the export background thread. When set
    /// to true the background exporter should stop rendering / kill ffmpeg.
    #[serde(skip)]
    pub export_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    #[serde(skip)]
    pub export_frames_done: u32,
    #[serde(skip)]
    pub export_frames_total: u32,
    /// Start time of the export (for elapsed/ETA calculation)
    #[serde(skip)]
    pub export_start_time: Option<std::time::Instant>,
    /// Number of frames to render before flushing to FFmpeg (controls peak RAM usage)
    pub export_batch_size: u32,
    /// Whether to use GPU acceleration for export rendering
    pub export_use_gpu: bool,
    /// Whether to use GPU encoder (NVENC/h264_nvenc) instead of CPU libx264
    pub export_use_gpu_encoder: bool,

    // UI: Elements modal (floating palette from Scene Graph)
    pub show_elements_modal: bool,
    // UI: Animations modal (floating palette from Scene Graph)
    pub show_animations_modal: bool,
    /// Remember last top-left position of the Animations modal (UI-only)
    #[serde(skip)]
    pub animations_modal_pos: Option<egui::Pos2>,

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
    pub available_fonts: Vec<String>,
    #[serde(skip)]
    pub font_map: std::collections::HashMap<String, std::path::PathBuf>,
    #[serde(skip)]
    pub font_definitions: egui::FontDefinitions,
    #[serde(skip)]
    pub font_arc_cache: std::collections::HashMap<String, ab_glyph::FontArc>,
    /// Incremented whenever the DSL is parsed into the scene.
    #[serde(skip)]
    pub scene_version: u32,
    // removed `gpu_scene_version`; tracking handled elsewhere if needed
}
// (field removed) preview_texture no longer exists; GPU preview handled via
// `preview_native_texture_id`/`resource`.

/// Information persisted for the duration of an ongoing canvas resize drag.
#[derive(Clone)]
pub struct ResizeInfo {
    pub path: Vec<usize>,
    /// Centre of the element (in screen coordinates) at the start of the drag.
    pub centre: egui::Pos2,
    /// Whether the horizontal dimension should change (left or right edge).
    pub horiz: bool,
    /// Whether the vertical dimension should change (top or bottom edge).
    pub vert: bool,
    /// Original width fraction (for rects) at drag start.
    pub orig_w: Option<f32>,
    /// Original height fraction (for rects) at drag start.
    pub orig_h: Option<f32>,
    pub orig_x: Option<f32>,
    pub orig_y: Option<f32>,
    /// Which horizontal edge(s) were hit.
    pub left: bool,
    pub right: bool,
    /// Which vertical edge(s) were hit.
    pub top: bool,
    pub bottom: bool,
}

/// Temporary information persisted for the duration of an ongoing canvas move
/// drag.  We only need to remember where the element centre was at the start
/// of the drag and the pointer position so that subsequent pointer movements
/// can be translated directly into updated centre coordinates.
#[derive(Clone)]
pub struct MoveInfo {
    pub path: Vec<usize>,
    /// Centre of the element (in screen coordinates) at the start of the drag.
    pub centre: egui::Pos2,
    /// Pointer position when the drag began; used to compute displacement.
    pub start_pos: egui::Pos2,
    pub axis_x: bool,
    pub axis_y: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToastType {
    Info,
    Error,
    Success,
}

// removed helper, progress tracking now handled directly in export logic
// fn default_progress() -> Arc<AtomicUsize> {
//     Arc::new(AtomicUsize::new(0))
// }

fn default_pid() -> Pid {
    sysinfo::get_current_pid().unwrap_or(Pid::from(0))
}

impl Default for AppState {
    fn default() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        let pid = sysinfo::get_current_pid().unwrap_or(Pid::from(0));
        // pre-compute font list once to avoid duplicated work
        let font_list = crate::shapes::fonts::list_system_fonts();
        let available_fonts: Vec<String> = font_list.iter().map(|(n, _)| n.clone()).collect();
        let font_map: std::collections::HashMap<String, PathBuf> =
            font_list.clone().into_iter().collect();

        // create the helper channels before we begin building the struct
        let (folder_dialog_tx, folder_dialog_rx) = std::sync::mpsc::channel::<PathBuf>();
        let (save_dialog_tx, save_dialog_rx) = std::sync::mpsc::channel::<PathBuf>();
        let (font_refresh_tx, font_refresh_rx) =
            std::sync::mpsc::channel::<(Vec<String>, std::collections::HashMap<String, PathBuf>)>();

        Self {
            fps: 60,
            duration_secs: 5.0,
            render_width: 1280,
            render_height: 720,
            preview_multiplier: 1.0,
            preview_fps: 60,
            #[cfg(feature = "wgpu")]
            wgpu_render_state: None,
            logo_texture: None,
            preview_native_texture_id: None,
            #[cfg(feature = "wgpu")]
            preview_native_texture_resource: None,
            preview_gpu_cache: Vec::new(),
            // legacy cache fields removed
            preview_worker_tx: None,
            preview_worker_rx: None,
            preview_job_pending: false,
            preview_worker_use_gpu: true,
            // legacy cache flags removed
            playing: false,
            time: 0.0,
            last_time_changed: None,
            dsl: DslState::default(),
            export_in_progress: false,
            last_export_path: None,
            // populate scene as ElementKeyframes converted from legacy sample_scene
            scene: crate::shapes::shapes_manager::Shape::sample_scene()
                .into_iter()
                .filter_map(|s| {
                    crate::shapes::element_store::ElementKeyframes::from_shape_at_spawn(&s, 60)
                })
                .collect(),
            selected: None,
            selected_node_path: None,
            show_dsl: false,
            show_settings: false,
            active_tab: None,
            last_active_tab: PanelTab::SceneGraph,
            transition_source_tab: None,
            tab_switch_time: None,
            dsl_code: String::new(),
            color_picker_data: None,
            completion_worker_tx: None,
            completion_worker_rx: None,
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
            completion_snippet_active: false,
            completion_snippet_region: None,
            completion_snippet_params: Vec::new(),
            completion_snippet_index: None,
            last_completion_query: None,
            last_completion_query_time: 0.0,
            // choose a slightly longer debounce so validation doesn't kick in
            // while the user is still typing.  This mirrors the value used in
            // the autosave timer elsewhere (previously 0.5s, now 0.8s per
            // user request).
            autosave: AutosaveState {
                cooldown_secs: 0.8,
                ..Default::default()
            },
            last_scene_parse_time: 0.0,
            preview_pending_from_code: false,
            project_path: None,
            project_path_input: String::new(),
            path_validation_error: None,
            show_welcome: true,
            picker_active: false,
            picker_color: [255, 255, 255, 255],
            resize_mode: false,
            resize_info: None,
            move_mode: false,
            move_info: None,
            drag_start_time: None,
            potential_drag_path: None,
            renaming_path: None,
            rename_buffer: String::new(),
            expanded_nodes: HashSet::new(),
            modifier_active_path: None,
            show_export_modal: false,
            export_modal_step: 0,
            export_modal_fps: 60,
            export_modal_width: 1280,
            export_modal_height: 720,
            export_modal_duration: 5.0,
            export_output_path: None,
            export_ffmpeg_log: Vec::new(),
            export_ffmpeg_rx: None,
            export_ffmpeg_done: false,
            export_ffmpeg_error: None,
            export_cancel: None,
            export_frames_done: 0,
            export_frames_total: 0,
            export_start_time: None,
            export_batch_size: 30,
            export_use_gpu: true,
            export_use_gpu_encoder: true,
            anim_modal_target_idx: 0,
            show_elements_modal: false,
            show_animations_modal: false,
            animations_modal_pos: None,
            sidebar_width: 250.0,
            timeline_root_path: None,
            timeline_prev_root_path: None,
            timeline_breadcrumb_anim_t: 1.0,
            // `list_system_fonts` is somewhat expensive; compute once and
            // reuse the vector for both `available_fonts` and `font_map`.
            available_fonts,
            font_map,
            font_definitions: egui::FontDefinitions::default(),
            font_arc_cache: std::collections::HashMap::new(),
            // export_progress: Arc::new(AtomicUsize::new(0)),
            // gpu_scene_version: 0,
            scene_version: 1,
            // async dialog channels (constructed above)
            folder_dialog_tx: Some(folder_dialog_tx),
            folder_dialog_rx: Some(folder_dialog_rx),
            save_dialog_tx: Some(save_dialog_tx),
            save_dialog_rx: Some(save_dialog_rx),
            font_refresh_tx: Some(font_refresh_tx),
            font_refresh_rx: Some(font_refresh_rx),
            // dialog channels are added below in the normal field order
            // (they were created above, before the `Self {` block).
        }
    }
}
// end impl Default for AppState

impl AppState {
    /// This was previously in `create_app` but migrating it here allows the
    /// public API of the application logic to be driven from a single state
    /// object and keeps the UI layer thin.  The method is intentionally
    /// idempotent so it may be called multiple times during tests.
    pub fn initialize_with_context(&mut self, cc: &eframe::CreationContext<'_>) {
        // Old VRAM detection and cache sizing logic has been removed.  The
        // preview pipeline now renders directly to GPU textures and no
        // estimate is required.

        // Load the SVG logo just once and cache the texture handle.
        if self.logo_texture.is_none() {
            if let Some(img) = crate::logo::color_image_from_svg(include_str!("../assets/logo.svg"))
            {
                let handle =
                    cc.egui_ctx
                        .load_texture("app_logo", img, egui::TextureOptions::NEAREST);
                self.logo_texture = Some(handle);
            }
        }

        // no-op: CPU caches are gone

        // wgpu side-effects: store render state and register our pipeline
        #[cfg(feature = "wgpu")]
        if let Some(render_state) = &cc.wgpu_render_state {
            self.preview_worker_use_gpu = true;
            self.wgpu_render_state = Some(render_state.clone());

            let device = &render_state.device;
            let target_format = render_state.target_format;
            use crate::canvas::GpuResources;
            let mut renderer = render_state.renderer.write();
            renderer
                .callback_resources
                .insert(GpuResources::new(device, target_format));
        }
    }

    /// Helper that spawns the autocomplete worker thread and stores the
    /// channels in the state.  This encapsulates the repetitive boilerplate
    /// previously duplicated in `ui::create_app`.
    pub fn ensure_completion_worker(&mut self) {
        if self.completion_worker_tx.is_some() {
            return; // already spawned
        }

        let (atx, arx) = std::sync::mpsc::channel::<String>();
        let (btx, brx) = std::sync::mpsc::channel::<Vec<CompletionItem>>();
        self.completion_worker_tx = Some(atx.clone());
        self.completion_worker_rx = Some(brx);

        std::thread::spawn(move || {
            // static candidate list built once
            let candidates = vec![
                CompletionItem::simple("project"),
                CompletionItem::simple("timeline"),
                CompletionItem::simple("layer"),
                CompletionItem::simple("fps"),
                CompletionItem::simple("duration"),
                CompletionItem::simple("size"),
                CompletionItem::simple("fill"),
                CompletionItem::simple("radius"),
                CompletionItem::simple("width"),
                CompletionItem::simple("height"),
                CompletionItem::simple("color"),
                CompletionItem::snippet(
                    "circle",
                    "circle \"Name\" {\n    x = 0.50,\n    y = 0.50,\n    radius = 0.10,\n    fill = \"#78c8ff\",\n    spawn = 0.00\n}\n",
                ),
                CompletionItem::snippet(
                    "rect",
                    "rect \"Name\" {\n    x = 0.50,\n    y = 0.50,\n    width = 0.30,\n    height = 0.20,\n    fill = \"#c87878\",\n    spawn = 0.00\n}\n",
                ),
                CompletionItem::snippet(
                    "text",
                    "text \"Name\" {\n    x = 0.50,\n    y = 0.50,\n    value = \"Hello\",\n    font = \"System\",\n    size = 24.0,\n    fill = \"#ffffff\",\n    spawn = 0.00\n}\n",
                ),
                CompletionItem::snippet(
                    "move",
                    "move {\n    element = \"Name\",\n    to = (0.50, 0.50),\n    during = 0.00 -> 1.00,\n    ease = linear\n}\n",
                ),
            ];

            while let Ok(query) = arx.recv() {
                let filtered: Vec<_> = candidates
                    .iter()
                    .filter(|c| c.label.starts_with(&query) && c.label != query)
                    .cloned()
                    .collect();
                let _ = btx.send(filtered);
            }
        });
    }

    /// Ensure that all fonts referenced by the currently-loaded scene are
    /// registered with egui.  This was previously repeated inline every frame
    /// inside `ui::update` and incurred multiple clones of font names and
    /// definition maps.
    pub fn load_scene_fonts(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        // gather all font names used by text elements
        let mut names = Vec::new();
        for elem in &self.scene {
            if elem.kind != "text" {
                continue;
            }
            if let Some(crate::scene::Shape::Text(t)) =
                elem.to_shape_at_frame(elem.spawn_frame, self.fps)
            {
                names.push(t.font.clone());
                for span in &t.spans {
                    names.push(span.font.clone());
                }
            }
        }

        for font_name in names {
            if font_name == "System" || font_name.is_empty() {
                continue;
            }
            if let Some(path) = self.font_map.get(&font_name) {
                if crate::shapes::fonts::load_font(&mut self.font_definitions, &font_name, path) {
                    changed = true;
                }
                if !self.font_arc_cache.contains_key(&font_name) {
                    if let Some(font) = crate::shapes::fonts::load_font_arc(path) {
                        self.font_arc_cache.insert(font_name.clone(), font);
                    }
                }
            }
        }
        if changed {
            ctx.set_fonts(self.font_definitions.clone());
        }
    }

    // NOTE: the diagnostics/auto‑save helpers previously defined here have
    // been moved to `states::autosave`.  We no longer expose public methods on
    // `AppState` for them because there are no external callers; the UI loop
    // uses `AppState::tick` to drive everything.

    /// Perform a debounced parse of the current DSL text and update the in-
    /// memory scene if successful.  This mirrors the logic that used to live
    /// inline in `ui::update` but is now encapsulated so the UI layer is more
    /// concise.
    ///
    /// Returns `true` if the scene was replaced (i.e. parse succeeded and
    /// produced non-empty output).
    pub fn debounced_parse(&mut self, now: f64) -> bool {
        // parse after ~120ms of inactivity
        let parse_debounce = 0.12_f64;
        if let Some(last_edit) = self.autosave.last_edit_time {
            if now - last_edit > parse_debounce && now - self.last_scene_parse_time > parse_debounce
            {
                // diagnostics are managed by the autosave helper (now
                // `states::autosave::tick`/`AppState::tick`) alone; the
                // parsing step should not interfere with them.  We used to
                // clear diagnostics here when a parse occurred, which caused
                // the gutter/banner to flash as the user typed.  Let the
                // previous errors remain visible until new results are
                // produced by the autosave timer.

                // parse configuration (non-fatal)
                if let Ok(config) = crate::dsl::parse_config(&self.dsl_code) {
                    self.fps = config.fps;
                    self.duration_secs = config.duration;
                    self.render_width = config.width;
                    self.render_height = config.height;
                }

                // parse full DSL into element keyframes
                let parsed = crate::dsl::parse_dsl_into_elements(&self.dsl_code, self.fps);
                if !parsed.is_empty() {
                    self.scene = parsed;
                    self.scene_version += 1;
                    self.dsl.event_handlers =
                        crate::dsl::extract_event_handlers_structured(&self.dsl_code);

                    // preview throttle logic is UI-specific but we keep a small
                    // helper here to avoid repeating the constants.
                    const CODE_PREVIEW_IDLE_SECS: f64 = 0.45;
                    let do_request = if self.active_tab == Some(PanelTab::Code) {
                        if let Some(last_edit) = self.autosave.last_edit_time {
                            if now - last_edit > CODE_PREVIEW_IDLE_SECS {
                                true
                            } else {
                                self.preview_pending_from_code = true;
                                false
                            }
                        } else {
                            true
                        }
                    } else {
                        true
                    };

                    if do_request {
                        crate::canvas::request_preview_frames(self, self.time);
                        self.preview_pending_from_code = false;
                    }
                }

                self.last_scene_parse_time = now;
                return true;
            }
        }
        false
    }

    /// Drive periodic state updates that were previously called from
    /// `ui::update`.  This keeps UI code thin and allows tests to tick the
    /// application state without pulling in egui.  The method returns `true`
    /// if the scene buffer was replaced by a successful parse.
    #[inline]
    pub fn tick(&mut self, now: f64) -> bool {
        // drive autosave/validation; implementation lives in states/autosave
        crate::states::autosave::tick(self, now);
        self.debounced_parse(now)
    }
}

// helper constructors for CompletionItem
impl CompletionItem {
    pub fn simple(label: &str) -> Self {
        Self {
            label: label.into(),
            insert_text: label.into(),
            is_snippet: false,
        }
    }

    pub fn snippet(label: &str, insert_text: &str) -> Self {
        Self {
            label: label.into(),
            insert_text: insert_text.into(),
            is_snippet: true,
        }
    }
}

impl AppState {
    /// Set the playhead time (seconds) and emit the `TimeChangedEvent`.
    ///
    /// This centralizes time updates so all callers get identical behavior
    /// (update state, compute frame, dispatch DSL handlers).
    pub fn set_time(&mut self, seconds: f32) {
        self.time = seconds;
        let frame = (self.time * self.fps as f32).round() as u32;
        crate::events::time_changed_event::TimeChangedEvent::on_time_changed(
            self, self.time, frame,
        );
    }

    // `refresh_fonts` is no longer called anywhere; the async variant is used
    // by the UI.  keep the implementation around for reference, but it is
    // effectively dead code and removed from compilation by commenting.
    // pub fn refresh_fonts(&mut self) {
    //     let mut all_fonts = crate::shapes::fonts::list_system_fonts();
    //     if let Some(path) = &self.project_path {
    //         let ws_fonts = crate::shapes::fonts::list_workspace_fonts(path);
    //         all_fonts.extend(ws_fonts);
    //     }
    //     all_fonts.sort_by(|a, b| a.0.cmp(&b.0));
    //     all_fonts.dedup_by(|a, b| a.0 == b.0);
    //
    //     self.available_fonts = all_fonts.iter().map(|(n, _)| n.clone()).collect();
    //     self.font_map = all_fonts.into_iter().collect();
    // }

    /// Spawn a background worker to recompute the font lists and update the
    /// state asynchronously.  Results arrive on `font_refresh_rx` and should
    /// be polled by the UI loop (see `ui.rs`).
    pub fn refresh_fonts_async(&mut self) {
        if let Some(tx) = &self.font_refresh_tx {
            let path = self.project_path.clone();
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut all_fonts = crate::shapes::fonts::list_system_fonts();
                if let Some(p) = path {
                    let ws_fonts = crate::shapes::fonts::list_workspace_fonts(&p);
                    all_fonts.extend(ws_fonts);
                }
                all_fonts.sort_by(|a, b| a.0.cmp(&b.0));
                all_fonts.dedup_by(|a, b| a.0 == b.0);
                let names = all_fonts.iter().map(|(n, _)| n.clone()).collect();
                let map = all_fonts.into_iter().collect();
                let _ = tx.send((names, map));
            });
        }
    }

    pub fn request_dsl_update(&mut self) {
        self.dsl_code = crate::dsl::generate_dsl_from_elements(
            &self.scene,
            self.render_width,
            self.render_height,
            self.fps,
            self.duration_secs,
        );
        crate::events::element_properties_changed_event::on_element_properties_changed(self);
    }
}

#[derive(Clone)]
pub struct ColorPickerData {
    pub range: std::ops::Range<usize>,
    pub color: [u8; 4],
    pub is_alpha: bool,
}

// GPU renderer struct was unused; remove to eliminate dead code.
// #[cfg(feature = "wgpu")]
// pub struct WgpuRenderer {
//     pub pipeline: wgpu::RenderPipeline,
//     pub bind_group_layout: wgpu::BindGroupLayout,
//     // We'll store a buffer for shapes
// }
