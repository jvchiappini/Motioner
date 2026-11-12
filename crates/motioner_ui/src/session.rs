//! Loading an MDF program and its overrides, and writing edits back.
//!
//! A [`PreviewSession`] owns the program text, the last good document, the
//! override set that was applied to it, the value origins the planner needs,
//! and the analysis cache. A reload compiles with
//! [`motioner_dsl::run_with_overrides`], so orphaned overrides arrive as
//! warnings instead of failing the load.
//!
//! Override edits update the override set, recompile from the cached program
//! text and write `<name>.overrides.motion` **debounced** (300 ms after the
//! last change) and atomically. Write-through edits ([`Self::apply_program`])
//! replace the program text, compile it with whatever overrides remain and
//! write the program atomically. The file watcher ignores the editor's own
//! writes because each watcher is reset after its write.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use motioner_core::{
    Composition, Document, FrameRate, NoTextMetrics, NodeId, OverrideSet, TextMetrics,
};
use motioner_dsl::{AssetResolver, Compiled, Diagnostic, Limits, NoAssets, ValueOrigins};
use motioner_eval::{Evaluator, RenderPlan};

use crate::file_watch::FileWatch;

/// Delay between the last edit and the overrides write.
pub const WRITE_DEBOUNCE: Duration = Duration::from_millis(300);

/// One user-facing diagnostic, independent of the DSL's source spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticInfo {
    /// Human-readable message.
    pub message: String,
    /// 1-based line, or `0` when the diagnostic has no source position.
    pub line: u32,
    /// 1-based column, or `0` when the diagnostic has no source position.
    pub column: u32,
    /// Whether this is a warning (the document still loaded with it).
    pub warning: bool,
}

impl DiagnosticInfo {
    fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        Self {
            message: diagnostic.message.clone(),
            line: diagnostic.span.line,
            column: diagnostic.span.column,
            warning: !diagnostic.is_error(),
        }
    }

    fn io(path: &Path, error: &std::io::Error) -> Self {
        Self {
            message: format!("cannot read {}: {error}", path.display()),
            line: 0,
            column: 0,
            warning: false,
        }
    }
}

impl fmt::Display for DiagnosticInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "{}:{}: {}", self.line, self.column, self.message)
        }
    }
}

/// Why a load did not produce a new document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// The file could not be read.
    Io {
        /// Source path.
        path: PathBuf,
        /// Underlying I/O message.
        message: String,
    },
    /// The MDF program (or its overrides file) produced a diagnostic.
    Diagnostic(DiagnosticInfo),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "cannot read {}: {message}", path.display()),
            Self::Diagnostic(diagnostic) => write!(f, "{diagnostic}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// The overrides file that belongs to `program`: `<stem>.overrides.motion`
/// next to it (D-0022).
#[must_use]
pub fn overrides_path(program: &Path) -> PathBuf {
    let stem = program
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("program");
    program.with_file_name(format!("{stem}.overrides.motion"))
}

/// Writes `text` to `path` through a temporary file in the same directory and
/// a rename, so a crash never leaves a half-written file.
///
/// # Errors
///
/// Returns the underlying I/O error with the path in its context.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temp = path.with_file_name(format!(".{file_name}.tmp"));
    fs::write(&temp, text)?;
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

/// The fallback rate used before a composition has loaded.
fn default_rate() -> FrameRate {
    FrameRate::new(60, 1).expect("60/1 is a valid frame rate")
}

/// The last good document, its overrides, and the latest diagnostics.
pub struct PreviewSession {
    path: PathBuf,
    overrides_path: PathBuf,
    source: String,
    overrides: OverrideSet,
    document: Option<Document>,
    diagnostics: Vec<DiagnosticInfo>,
    warnings: Vec<DiagnosticInfo>,
    evaluator: Evaluator,
    text_metrics: Option<Arc<dyn TextMetrics>>,
    plan: RenderPlan,
    plan_frame: Option<i64>,
    plan_generation: u64,
    generation: u64,
    /// Number of program writes performed (the window uses it to ignore its
    /// own writes in its separate file watcher).
    program_writes: u64,
    program_watch: FileWatch,
    overrides_watch: FileWatch,
    /// Text of the pending write, if an edit has not been flushed yet.
    dirty: Option<String>,
    write_at: Instant,
    /// Text this editor last wrote or read, to recognise its own write.
    last_overrides_text: String,
    /// Program text this editor last wrote or read.
    last_program_text: String,
    /// Value origins of the last good compile (the write-through planner).
    origins: Option<ValueOrigins>,
    /// Top-level variable bindings of the last good compile.
    bindings: Vec<(String, NodeId)>,
    /// Whether file writes are enabled (disabled by `motioner edit --dry-run`).
    write_enabled: bool,
    images: Rc<dyn motioner_core::ImageResolver>,
    videos: Rc<dyn motioner_core::VideoResolver>,
    assets: Rc<dyn AssetResolver>,
}

impl PreviewSession {
    /// Creates an empty session for `path`; call [`Self::reload`] to load it.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let overrides_path = overrides_path(&path);
        Self {
            program_watch: FileWatch::new(&path),
            overrides_watch: FileWatch::new(&overrides_path),
            path,
            overrides_path,
            source: String::new(),
            overrides: OverrideSet::new(),
            document: None,
            diagnostics: Vec::new(),
            warnings: Vec::new(),
            evaluator: Evaluator::new(),
            text_metrics: None,
            plan: RenderPlan::new(),
            plan_frame: None,
            plan_generation: 0,
            generation: 0,
            program_writes: 0,
            dirty: None,
            write_at: Instant::now(),
            last_overrides_text: String::new(),
            last_program_text: String::new(),
            origins: None,
            bindings: Vec::new(),
            write_enabled: true,
            images: Rc::new(motioner_core::NoImages),
            videos: Rc::new(motioner_core::NoVideos),
            assets: Rc::new(NoAssets),
        }
    }

    /// Installs the CPU text measurer used by `text_width`/`text_bounds`.
    ///
    /// Must be called before [`Self::reload`] for a program that measures text.
    pub fn set_text_metrics(&mut self, metrics: Arc<dyn TextMetrics>) {
        self.text_metrics = Some(metrics);
    }

    /// Installs the resolver used by `use()` of SVG symbols and `.motion`
    /// modules (D-0051).
    ///
    /// The host injects the project-rooted resolver before [`Self::reload`];
    /// there is no second resolution path and no fallback once a project is
    /// open.
    pub fn set_asset_resolver(&mut self, resolver: Rc<dyn AssetResolver>) {
        self.assets = resolver;
    }

    /// The installed `use()` resolver.
    #[must_use]
    pub fn asset_resolver(&self) -> &Rc<dyn AssetResolver> {
        &self.assets
    }

    /// The text metrics the program is compiled with.
    #[must_use]
    pub fn text_metrics(&self) -> &dyn TextMetrics {
        self.text_metrics.as_deref().unwrap_or(&NoTextMetrics)
    }

    /// Compiles `source` and `overrides` with the installed metrics and the
    /// injected `use()` resolver.
    fn compile_source(
        &self,
        source: &str,
        overrides_source: Option<&str>,
    ) -> Result<Compiled, Diagnostic> {
        let metrics: &dyn TextMetrics = self.text_metrics.as_deref().unwrap_or(&NoTextMetrics);
        motioner_dsl::run_with_overrides_and_metrics_and_assets(
            source,
            overrides_source,
            &Limits::default(),
            metrics,
            self.assets.as_ref(),
        )
    }

    /// The source path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Installs the image resolver used by `image()` nodes (D-0035).
    pub fn set_image_resolver(&mut self, resolver: Rc<dyn motioner_core::ImageResolver>) {
        self.images = resolver;
    }

    /// Installs the video resolver used by `video()` nodes.
    pub fn set_video_resolver(&mut self, resolver: Rc<dyn motioner_core::VideoResolver>) {
        self.videos = resolver;
    }

    /// Every decoded image, for the live painter's texture uploads.
    #[must_use]
    pub fn decoded_images(&self) -> Vec<motioner_core::DecodedImage> {
        self.images.decoded()
    }

    /// The overrides file path.
    #[must_use]
    pub fn overrides_path(&self) -> &Path {
        &self.overrides_path
    }

    /// The cached program text of the last good load.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The last successfully loaded document (overrides applied), if any.
    #[must_use]
    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// The override set applied to the document.
    #[must_use]
    pub fn overrides(&self) -> &OverrideSet {
        &self.overrides
    }

    /// The value origins recorded by the last good compile.
    ///
    /// The write-through planner needs these to find where a node and its
    /// values were written in the program (D-0036). `None` before the first
    /// successful load.
    #[must_use]
    pub fn origins(&self) -> Option<&ValueOrigins> {
        self.origins.as_ref()
    }

    /// The top-level variable bindings of the last good compile.
    #[must_use]
    pub fn bindings(&self) -> &[(String, NodeId)] {
        &self.bindings
    }

    /// Enables or disables file writes (in-memory edits still apply).
    ///
    /// `motioner edit --dry-run` turns writes off so it can show the diff
    /// without touching either file.
    pub fn set_write_enabled(&mut self, enabled: bool) {
        self.write_enabled = enabled;
    }

    /// Whether file writes are enabled.
    #[must_use]
    pub const fn writes_enabled(&self) -> bool {
        self.write_enabled
    }

    /// Whether a program write is waiting (currently always `false`: program
    /// writes are immediate and atomic per committed gesture).
    #[must_use]
    pub const fn is_program_dirty(&self) -> bool {
        false
    }

    /// Whether an edit is waiting for its debounced write.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty.is_some()
    }

    /// The last good composition settings, if any.
    #[must_use]
    pub fn composition(&self) -> Option<&Composition> {
        self.document.as_ref().map(Document::composition)
    }

    /// Error diagnostics from the most recent load attempt.
    #[must_use]
    pub fn diagnostics(&self) -> &[DiagnosticInfo] {
        &self.diagnostics
    }

    /// Warnings (orphaned or inapplicable overrides) of the last good load.
    #[must_use]
    pub fn warnings(&self) -> &[DiagnosticInfo] {
        &self.warnings
    }

    /// Increments on every successful load or recompile.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Number of program writes performed so far (for the window's own file
    /// watcher to ignore its writes).
    #[must_use]
    pub fn program_writes(&self) -> u64 {
        self.program_writes
    }

    /// The plan cached by the last [`Self::evaluate`] call.
    #[must_use]
    pub fn plan(&self) -> &RenderPlan {
        &self.plan
    }

    /// The frame index of the cached plan, if one has been evaluated.
    #[must_use]
    pub fn plan_frame(&self) -> Option<i64> {
        self.plan_frame
    }

    /// Reads and executes the program and its overrides again.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Io`] when the program cannot be read, or
    /// [`SessionError::Diagnostic`] when the program or the overrides file is
    /// invalid. The previous document is kept on a diagnostic.
    pub fn reload(&mut self) -> Result<(), SessionError> {
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) => {
                let diagnostic = DiagnosticInfo::io(&self.path, &error);
                self.diagnostics = vec![diagnostic];
                return Err(SessionError::Io {
                    path: self.path.clone(),
                    message: error.to_string(),
                });
            }
        };
        let overrides_text = match fs::read_to_string(&self.overrides_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                let diagnostic = DiagnosticInfo::io(&self.overrides_path, &error);
                self.diagnostics = vec![diagnostic];
                return Err(SessionError::Io {
                    path: self.overrides_path.clone(),
                    message: error.to_string(),
                });
            }
        };
        match self.compile_source(
            &source,
            (!overrides_text.trim().is_empty()).then_some(overrides_text.as_str()),
        ) {
            Ok(compiled) => {
                self.last_program_text = source.clone();
                self.source = source;
                self.last_overrides_text = overrides_text;
                self.accept(compiled);
                self.diagnostics.clear();
                Ok(())
            }
            Err(diagnostic) => {
                let info = DiagnosticInfo::from_diagnostic(&diagnostic);
                self.diagnostics = vec![info.clone()];
                Err(SessionError::Diagnostic(info))
            }
        }
    }

    fn accept(&mut self, mut compiled: Compiled) {
        let issues = compiled.document.resolve_images(self.images.as_ref());
        let _ = compiled.document.resolve_videos(self.videos.as_ref());
        self.document = Some(compiled.document);
        self.overrides = compiled.overrides;
        self.origins = Some(compiled.origins);
        self.bindings = compiled.bindings;
        self.warnings = compiled
            .warnings
            .iter()
            .map(DiagnosticInfo::from_diagnostic)
            .collect();
        for issue in issues {
            self.warnings.push(DiagnosticInfo {
                message: format!(
                    "image {:?} (node {}): {}",
                    issue.source, issue.node, issue.message
                ),
                line: 0,
                column: 0,
                warning: true,
            });
        }
        self.generation = self.generation.saturating_add(1);
        self.plan_frame = None;
    }

    /// Applies `set` in memory and schedules the debounced write.
    ///
    /// The document is recompiled from the cached program text; no file is
    /// read. Returns `false` (and keeps the previous document) when the set
    /// does not compile, which happens only for a value the UI should have
    /// rejected.
    pub fn edit_overrides(&mut self, set: &OverrideSet, now: Instant) -> bool {
        let rate = match self.composition() {
            Some(composition) => composition.frame_rate(),
            None => return false,
        };
        let text = set.to_mdf(rate);
        match self.compile_source(&self.source, Some(&text)) {
            Ok(compiled) => {
                self.accept(compiled);
                self.diagnostics.clear();
                self.dirty = Some(text);
                self.write_at = now + WRITE_DEBOUNCE;
                true
            }
            Err(diagnostic) => {
                self.diagnostics = vec![DiagnosticInfo::from_diagnostic(&diagnostic)];
                false
            }
        }
    }

    /// Replaces the program text, compiles it with `set` and writes both files
    /// atomically (unless writes are disabled).
    ///
    /// This is the write-through sink (D-0036): the editor passes the program
    /// with the new code already applied and the overrides that remain. The
    /// document and origins are updated only when the new program compiles.
    /// Returns `false` and keeps the previous program and document when it
    /// does not.
    ///
    /// The program is written **once** per committed gesture; the overrides
    /// file is only rewritten when the remaining set differs from what is on
    /// disk.
    pub fn apply_program(&mut self, source: &str, set: &OverrideSet, _now: Instant) -> bool {
        let rate = self
            .composition()
            .map_or_else(default_rate, Composition::frame_rate);
        let overrides_text = set.to_mdf(rate);
        let overrides_arg = (!overrides_text.trim().is_empty()).then_some(overrides_text.as_str());
        let compiled = match self.compile_source(source, overrides_arg) {
            Ok(compiled) => compiled,
            Err(diagnostic) => {
                self.diagnostics = vec![DiagnosticInfo::from_diagnostic(&diagnostic)];
                return false;
            }
        };
        let program_changed = source != self.source;
        let overrides_changed = self.overrides_need_write(set, &overrides_text);
        let writable = self.write_enabled && !self.path.as_os_str().is_empty();
        if writable {
            // Write the overrides first: a crash between the two writes leaves
            // the program unchanged, never a stale override over new code.
            if overrides_changed {
                if let Err(error) = write_atomic(&self.overrides_path, &overrides_text) {
                    self.diagnostics = vec![DiagnosticInfo::io(&self.overrides_path, &error)];
                    return false;
                }
            }
            if program_changed {
                if let Err(error) = write_atomic(&self.path, source) {
                    self.diagnostics = vec![DiagnosticInfo::io(&self.path, &error)];
                    return false;
                }
                self.program_writes = self.program_writes.saturating_add(1);
            }
            source.clone_into(&mut self.last_program_text);
            self.last_overrides_text.clone_from(&overrides_text);
        }
        source.clone_into(&mut self.source);
        self.accept(compiled);
        self.diagnostics.clear();
        self.dirty = None;
        if writable {
            self.program_watch.reset();
            if overrides_changed {
                self.overrides_watch.reset();
            }
        }
        true
    }

    /// Whether the overrides text differs from disk, treating "no file and no
    /// overrides" as equal (so write-through does not create an empty file).
    fn overrides_need_write(&self, set: &OverrideSet, text: &str) -> bool {
        if set.is_empty() && self.last_overrides_text.is_empty() {
            return false;
        }
        text != self.last_overrides_text
    }

    /// Compiles and accepts `new_source` with the current overrides, then
    /// writes it atomically. The previous document and source survive a
    /// failure so the caller can fall back to overrides.
    pub fn edit_program(&mut self, new_source: &str, now: Instant) -> bool {
        let set = self.overrides.clone();
        self.apply_program(new_source, &set, now)
    }

    /// Writes the current program text atomically when it differs from disk.
    ///
    /// # Errors
    ///
    /// [`SessionError::Io`] when the file cannot be written.
    pub fn write_program_now(&mut self) -> Result<PathBuf, SessionError> {
        if self.path.as_os_str().is_empty() {
            return Err(SessionError::Io {
                path: self.path.clone(),
                message: "no program file".to_owned(),
            });
        }
        if self.source != self.last_program_text {
            if let Err(error) = write_atomic(&self.path, &self.source) {
                self.diagnostics = vec![DiagnosticInfo::io(&self.path, &error)];
                return Err(SessionError::Io {
                    path: self.path.clone(),
                    message: error.to_string(),
                });
            }
            self.last_program_text = self.source.clone();
            self.program_writes = self.program_writes.saturating_add(1);
            self.program_watch.reset();
        }
        Ok(self.path.clone())
    }

    /// Writes the program now when it changed. Returns whether a write
    /// happened.
    pub fn flush_program(&mut self, _now: Instant) -> bool {
        if !self.write_enabled {
            return false;
        }
        let before = self.last_program_text.clone();
        self.write_program_now().is_ok() && self.last_program_text != before
    }

    /// Writes the current override set atomically when it differs from disk.
    ///
    /// Used by the buffered CLI transaction, where an edit is only persisted
    /// after every command succeeded.
    ///
    /// # Errors
    ///
    /// [`SessionError::Io`] when the file cannot be written.
    pub fn write_overrides_now(&mut self) -> Result<PathBuf, SessionError> {
        if self.path.as_os_str().is_empty() {
            return Ok(self.overrides_path.clone());
        }
        let rate = self
            .composition()
            .map_or_else(default_rate, Composition::frame_rate);
        let text = self.overrides.to_mdf(rate);
        if self.overrides_need_write(&self.overrides, &text) {
            if let Err(error) = write_atomic(&self.overrides_path, &text) {
                self.diagnostics = vec![DiagnosticInfo::io(&self.overrides_path, &error)];
                return Err(SessionError::Io {
                    path: self.overrides_path.clone(),
                    message: error.to_string(),
                });
            }
            self.last_overrides_text = text;
            self.overrides_watch.reset();
        }
        self.dirty = None;
        Ok(self.overrides_path.clone())
    }

    /// Writes the pending overrides when the debounce elapsed.
    ///
    /// Returns `true` when a file was written. A failed write leaves the edit
    /// pending and reports it as an I/O diagnostic.
    pub fn flush_overrides(&mut self, now: Instant) -> bool {
        let Some(text) = self.dirty.clone() else {
            return false;
        };
        if now < self.write_at {
            return false;
        }
        if let Err(error) = write_atomic(&self.overrides_path, &text) {
            self.diagnostics = vec![DiagnosticInfo::io(&self.overrides_path, &error)];
            return false;
        }
        self.last_overrides_text = text;
        self.dirty = None;
        self.overrides_watch.reset();
        true
    }

    /// Writes the overrides unconditionally, returning the path.
    ///
    /// # Errors
    ///
    /// [`SessionError::Io`] when the write fails.
    ///
    /// # Panics
    ///
    /// Never in practice: `60/1` is a valid fallback frame rate.
    pub fn force_write(&mut self) -> Result<PathBuf, SessionError> {
        let rate = self.composition().map_or_else(
            || FrameRate::new(60, 1).expect("60/1 is valid"),
            Composition::frame_rate,
        );
        let text = self.overrides.to_mdf(rate);
        if let Err(error) = write_atomic(&self.overrides_path, &text) {
            self.diagnostics = vec![DiagnosticInfo::io(&self.overrides_path, &error)];
            return Err(SessionError::Io {
                path: self.overrides_path.clone(),
                message: error.to_string(),
            });
        }
        self.last_overrides_text = text;
        self.dirty = None;
        self.overrides_watch.reset();
        Ok(self.overrides_path.clone())
    }

    /// Polls both files and reports whether an external change should reload.
    ///
    /// The editor's own writes are not changes: each watcher is reset after
    /// writing and the on-disk text is compared with what was written.
    pub fn poll_changes(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if self.program_watch.check(now) {
            match fs::read_to_string(&self.path) {
                Ok(text) if text == self.last_program_text => self.program_watch.reset(),
                _ => changed = true,
            }
        }
        if self.overrides_watch.check(now) {
            match fs::read_to_string(&self.overrides_path) {
                Ok(text) if text == self.last_overrides_text => self.overrides_watch.reset(),
                Ok(text) if self.dirty.as_deref() == Some(text.as_str()) => {
                    self.overrides_watch.reset();
                }
                _ => changed = true,
            }
        }
        changed
    }

    /// Forces the next poll to reload everything (the `R` shortcut).
    pub fn request_reload(&mut self) {
        self.program_watch.reset();
        self.overrides_watch.reset();
    }

    /// Evaluates `frame` if the cached plan is stale, and returns the document
    /// and plan to paint.
    ///
    /// The plan is reused when neither the frame index nor the document
    /// generation changed, so a paused window does not re-evaluate every frame.
    #[must_use]
    pub fn evaluate(&mut self, frame: i64) -> Option<(&Document, &RenderPlan)> {
        let document = self.document.as_ref()?;
        let stale = self.plan_frame != Some(frame) || self.plan_generation != self.generation;
        if stale {
            self.evaluator.evaluate(document, frame, &mut self.plan);
            self.plan_frame = Some(frame);
            self.plan_generation = self.generation;
        }
        Some((self.document.as_ref()?, &self.plan))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{OverrideAction, OverrideEntry, OverrideValue};

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "motioner_ui_session_{}_{name}.motion",
            std::process::id()
        ))
    }

    fn write(path: &Path, source: &str) {
        fs::write(path, source).unwrap();
    }

    fn cleanup(path: &Path) {
        fs::remove_file(path).ok();
        fs::remove_file(overrides_path(path)).ok();
    }

    #[test]
    fn loads_a_valid_program_and_evaluates_a_frame() {
        let path = temp_path("valid");
        write(&path, HELLO);
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        assert!(session.diagnostics().is_empty());
        assert_eq!(session.generation(), 1);
        assert_eq!(session.composition().unwrap().width(), 1920);
        let (document, plan) = session.evaluate(120).unwrap();
        assert_eq!(plan.width(), document.composition().width());
        assert_eq!(session.plan_frame(), Some(120));
        cleanup(&path);
    }

    #[test]
    fn loads_an_overrides_file_and_reports_orphans_as_warnings() {
        let path = temp_path("overrides");
        write(&path, HELLO);
        write(
            &overrides_path(&path),
            "override(\"ghost\", \"opacity\", 0.5)\n",
        );
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        assert_eq!(session.overrides().len(), 1);
        assert_eq!(session.warnings().len(), 1);
        assert!(session.warnings()[0].warning);
        assert!(session.diagnostics().is_empty());
        cleanup(&path);
    }

    #[test]
    fn an_edit_recompiles_in_memory_and_writes_after_the_debounce() {
        let path = temp_path("edit");
        write(&path, HELLO);
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        let title = session.document().unwrap().roots()[0];
        let key = session
            .document()
            .unwrap()
            .node(title)
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let mut set = session.overrides().clone();
        set.insert(OverrideEntry::new(
            key,
            "rotation",
            OverrideAction::Set(OverrideValue::Number(30.0)),
        ));
        let start = Instant::now();
        assert!(session.edit_overrides(&set, start));
        let rotation = session
            .document()
            .unwrap()
            .node(title)
            .unwrap()
            .transform()
            .rotation
            .value();
        assert!((rotation - 30.0).abs() < 1e-9);
        assert!(session.is_dirty());
        assert!(!session.flush_overrides(start), "debounce not elapsed");
        assert!(session.flush_overrides(start + WRITE_DEBOUNCE));
        assert!(!session.is_dirty());
        let written = fs::read_to_string(overrides_path(&path)).unwrap();
        assert!(written.contains("\"rotation\", 30"), "{written}");
        cleanup(&path);
    }

    #[test]
    fn the_editors_own_write_is_not_an_external_change() {
        let path = temp_path("selfwrite");
        write(&path, HELLO);
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        let mut set = session.overrides().clone();
        set.insert(OverrideEntry::new(
            "anything",
            "visible",
            OverrideAction::Visible(false),
        ));
        let start = Instant::now();
        session.edit_overrides(&set, start);
        assert!(session.flush_overrides(start + WRITE_DEBOUNCE));
        // Poll well past the debounce: the write must not look external.
        assert!(!session.poll_changes(start + WRITE_DEBOUNCE * 4));
        cleanup(&path);
    }

    #[test]
    fn an_external_change_to_either_file_is_reported() {
        let path = temp_path("external");
        write(&path, HELLO);
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        let start = Instant::now();
        assert!(!session.poll_changes(start));
        write(&path, &format!("{HELLO}\n# edited\n"));
        assert!(!session.poll_changes(start + Duration::from_millis(300)));
        assert!(
            session.poll_changes(start + Duration::from_millis(600)),
            "program change reported after the debounce"
        );
        fs::remove_file(&path).ok();
        fs::remove_file(overrides_path(&path)).ok();
    }

    #[test]
    fn a_diagnostic_keeps_the_previous_document() {
        let path = temp_path("diagnostic");
        write(&path, HELLO);
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        let generation = session.generation();
        write(&path, "comp(name=\"Broken\"\n");
        let error = session.reload().unwrap_err();
        assert!(matches!(error, SessionError::Diagnostic(_)));
        assert_eq!(session.generation(), generation);
        assert!(session.document().is_some());
        cleanup(&path);
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let path = temp_path("missing");
        fs::remove_file(&path).ok();
        let mut session = PreviewSession::new(&path);
        assert!(matches!(session.reload(), Err(SessionError::Io { .. })));
        assert_eq!(session.diagnostics()[0].line, 0);
    }

    #[test]
    fn a_paused_frame_reuses_the_cached_plan() {
        let path = temp_path("cache");
        write(&path, HELLO);
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        session.evaluate(10).unwrap();
        let first_items = session.plan().items().len();
        session.evaluate(10).unwrap();
        assert_eq!(session.plan().items().len(), first_items);
        session.evaluate(11).unwrap();
        assert_eq!(session.plan_frame(), Some(11));
        cleanup(&path);
    }

    #[test]
    fn atomic_writes_replace_the_destination() {
        let path = temp_path("atomic");
        write(&path, "one");
        write_atomic(&path, "two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        cleanup(&path);
    }

    #[test]
    fn edit_program_writes_and_keeps_the_previous_source_on_failure() {
        let path = temp_path("edit_program");
        write(&path, HELLO);
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        let original = session.source().to_owned();
        let added = format!("{original}\n# written through\n");
        assert!(session.edit_program(&added, Instant::now()));
        assert_eq!(session.source(), added);
        assert_eq!(fs::read_to_string(&path).unwrap(), added);
        let generation = session.generation();
        assert!(!session.edit_program("comp(name=\"Broken\"\n", Instant::now()));
        assert_eq!(session.source(), added, "a failure keeps the last program");
        assert_eq!(session.generation(), generation);
        cleanup(&path);
    }
}
