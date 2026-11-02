//! Loading an MDF program and its overrides, and writing edits back.
//!
//! A [`PreviewSession`] owns the program text, the last good document, the
//! override set that was applied to it, and the analysis cache. A reload
//! compiles with [`motioner_dsl::run_with_overrides`], so orphaned overrides
//! arrive as warnings instead of failing the load.
//!
//! Edits never touch the program: they update the override set, recompile from
//! the cached program text and write `<name>.overrides.motion` **debounced**
//! (300 ms after the last change) and atomically. The file watcher ignores the
//! editor's own write because the watcher is reset after it.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use motioner_core::{Composition, Document, NoTextMetrics, OverrideSet, TextMetrics};
use motioner_dsl::{Compiled, Diagnostic, Limits};
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
    program_watch: FileWatch,
    overrides_watch: FileWatch,
    /// Text of the pending write, if an edit has not been flushed yet.
    dirty: Option<String>,
    write_at: Instant,
    /// Text this editor last wrote or read, to recognise its own write.
    last_overrides_text: String,
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
            dirty: None,
            write_at: Instant::now(),
            last_overrides_text: String::new(),
        }
    }

    /// Installs the CPU text measurer used by `text_width`/`text_bounds`.
    ///
    /// Must be called before [`Self::reload`] for a program that measures text.
    pub fn set_text_metrics(&mut self, metrics: Arc<dyn TextMetrics>) {
        self.text_metrics = Some(metrics);
    }

    /// The source path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
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
        let metrics: &dyn TextMetrics = self.text_metrics.as_deref().unwrap_or(&NoTextMetrics);
        match motioner_dsl::run_with_overrides_and_metrics(
            &source,
            (!overrides_text.trim().is_empty()).then_some(overrides_text.as_str()),
            &Limits::default(),
            metrics,
        ) {
            Ok(compiled) => {
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

    fn accept(&mut self, compiled: Compiled) {
        self.document = Some(compiled.document);
        self.overrides = compiled.overrides;
        self.warnings = compiled
            .warnings
            .iter()
            .map(DiagnosticInfo::from_diagnostic)
            .collect();
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
        let metrics: &dyn TextMetrics = self.text_metrics.as_deref().unwrap_or(&NoTextMetrics);
        match motioner_dsl::run_with_overrides_and_metrics(
            &self.source,
            Some(&text),
            &Limits::default(),
            metrics,
        ) {
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

    /// Polls both files and reports whether an external change should reload.
    ///
    /// The editor's own debounced write is not a change: the watcher is reset
    /// after writing and the on-disk text is compared with what was written.
    pub fn poll_changes(&mut self, now: Instant) -> bool {
        let program_changed = self.program_watch.check(now);
        let overrides_changed = self.overrides_watch.check(now);
        if !overrides_changed {
            return program_changed;
        }
        match fs::read_to_string(&self.overrides_path) {
            Ok(text) if text == self.last_overrides_text => false,
            Ok(text) if self.dirty.as_deref() == Some(text.as_str()) => {
                self.overrides_watch.reset();
                false
            }
            _ => true,
        }
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
}
