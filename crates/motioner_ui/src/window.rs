//! Entry point of the preview window: options, errors and `run_preview`.
//!
//! The composition is drawn in the engine's `Pure2D` pass through
//! [`motioner_render::LivePainter`]; the chrome is drawn in the engine's UI
//! pass, which the renderer composites on top. That ordering is why items can
//! overflow the composition over the pasteboard and still stay below the
//! panels. Engine 2D views are not used: they clip shapes but not text, and
//! the pinned engine's multi-view path clips hard edges at the wrong place
//! for views away from the origin (fixed upstream in the engine clone).

use std::cell::RefCell;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ferrous_engine::{App, AppMode, Color};
use motioner_core::FrameRate;
use motioner_render::RenderError;

use crate::app::PreviewApp;
use crate::capture::PngWriter;
use crate::control::ControlServer;
use crate::{Editor, PreviewSession, SessionError, Transport};

/// Embedded UI font (also used by the engine's immediate text pass).
const UI_FONT: &[u8] = include_bytes!("../../../engine/ferrous/assets/fonts/Roboto-Regular.ttf");

/// Default window size in physical pixels.
pub const DEFAULT_WINDOW_SIZE: (u32, u32) = (1600, 900);

/// Options accepted by [`run_preview`].
#[derive(Clone, Default)]
pub struct PreviewOptions {
    /// Initial frame (clamped into the composition).
    pub frame: Option<i64>,
    /// Start paused instead of playing immediately.
    pub paused: bool,
    /// Close the window after this many presented frames (smoke tests/CI).
    pub exit_after_frames: Option<u64>,
    /// Initial window size in physical pixels; [`DEFAULT_WINDOW_SIZE`] when
    /// `None`.
    pub window_size: Option<(u32, u32)>,
    /// Chrome scale override (for example `1.25`); the monitor's scale
    /// factor when `None`. Used to check layouts at other DPI settings.
    pub ui_scale: Option<f32>,
    /// Outline row selected at startup (screenshots and demos).
    pub select: Option<usize>,
    /// Capture the window to this PNG once `exit_after_frames` frames were
    /// presented (one by default), then close it.
    pub screenshot: Option<PathBuf>,
    /// Writes screenshots; supplied by the application so this crate does
    /// not depend on the export crate.
    pub png_writer: Option<PngWriter>,
    /// Open the live control endpoint and write its endpoint file here
    /// (`preview --control`).
    pub control: Option<PathBuf>,
    /// CPU text measurer for `text_width`/`text_bounds` (D-0034).
    pub text_metrics: Option<std::sync::Arc<dyn motioner_core::TextMetrics>>,
    /// Faces the preview may use (project plus bundled). The default renders
    /// with the bundled set only.
    pub fonts: Option<motioner_render::FontSet>,
}

/// Errors that prevent the preview window from running.
#[derive(Debug)]
pub enum UiError {
    /// The file could not be read at startup.
    Load(SessionError),
    /// The embedded font could not be registered with the renderer.
    Painter(RenderError),
    /// A requested screenshot could not be taken or written.
    Screenshot(String),
    /// The control endpoint could not be opened.
    Control(String),
    /// An internal invariant failed without a better typed error.
    Internal(String),
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => write!(f, "{error}"),
            Self::Painter(error) => write!(f, "{error}"),
            Self::Screenshot(message) => write!(f, "screenshot failed: {message}"),
            Self::Control(message) => write!(f, "cannot open the control endpoint: {message}"),
            Self::Internal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for UiError {}

/// Opens the preview window for `path` and blocks until it closes.
///
/// A missing or unreadable file is a startup error. An MDF diagnostic opens
/// the window with no composition and shows the diagnostic, so the user can
/// fix the file and let hot reload pick it up.
///
/// # Errors
///
/// Returns [`UiError::Load`] for an unreadable file, [`UiError::Painter`]
/// when the embedded font cannot be registered and [`UiError::Screenshot`]
/// when a requested screenshot cannot be taken or written.
pub fn run_preview(path: &Path, options: PreviewOptions) -> Result<(), UiError> {
    let mut session = PreviewSession::new(path);
    if let Some(metrics) = options.text_metrics.clone() {
        session.set_text_metrics(metrics);
    }
    match session.reload() {
        Ok(()) | Err(SessionError::Diagnostic(_)) => {}
        Err(error @ SessionError::Io { .. }) => return Err(UiError::Load(error)),
    }

    let (rate, duration) = session
        .composition()
        .map_or((default_rate()?, 1), |composition| {
            (composition.frame_rate(), composition.duration())
        });
    let mut transport = Transport::new(rate, duration);
    if let Some(frame) = options.frame {
        transport.set_frame(frame);
    }

    let (width, height) = options.window_size.unwrap_or(DEFAULT_WINDOW_SIZE);
    let title = format!(
        "Motioner — {}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled")
    );
    let control = match &options.control {
        Some(file) => {
            let server =
                ControlServer::start(file).map_err(|error| UiError::Control(error.to_string()))?;
            println!(
                "Control endpoint: 127.0.0.1:{} (token in {})",
                server.endpoint().port,
                file.display()
            );
            Some(server)
        }
        None => None,
    };
    // `App::run` consumes the app and returns `()`, so a fatal error raised
    // inside the event loop is handed back through this shared slot.
    let fatal = Rc::new(RefCell::new(None));
    let mut editor = Editor::new(session, transport, options.select);
    if let Some(fonts) = &options.fonts {
        let mut families: Vec<String> =
            fonts.faces().map(|(family, _)| family.to_owned()).collect();
        families.sort();
        families.dedup();
        editor.set_font_families(families);
    }
    let app = PreviewApp::new(editor, options, control, Rc::clone(&fatal));
    App::new(app)
        .with_title(&title)
        .with_size(width.max(320), height.max(240))
        .with_mode(AppMode::Flat2D)
        .with_background_color(Color::rgb(0.0, 0.0, 0.0))
        .with_target_fps(Some(60))
        .with_vsync(true)
        .with_font_bytes(UI_FONT)
        .run();
    let error = fatal.borrow_mut().take();
    error.map_or(Ok(()), Err)
}

fn default_rate() -> Result<FrameRate, UiError> {
    FrameRate::new(60, 1).map_err(|error| UiError::Internal(error.to_string()))
}
