//! Loading an MDF document and evaluating it for one frame.
//!
//! A [`PreviewSession`] owns the source path, the last good
//! [`motioner_core::Document`], the evaluator and its plan. A failed reload
//! keeps the previous document on screen and records the diagnostic; a
//! successful reload bumps the generation, which invalidates the cached plan.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use motioner_core::{Composition, Document};
use motioner_dsl::{Diagnostic, Limits};
use motioner_eval::{Evaluator, RenderPlan};

/// One user-facing diagnostic, independent of the DSL's source spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticInfo {
    /// Human-readable message.
    pub message: String,
    /// 1-based line, or `0` when the diagnostic has no source position.
    pub line: u32,
    /// 1-based column, or `0` when the diagnostic has no source position.
    pub column: u32,
}

impl DiagnosticInfo {
    fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        Self {
            message: diagnostic.message.clone(),
            line: diagnostic.span.line,
            column: diagnostic.span.column,
        }
    }

    fn io(path: &Path, error: &std::io::Error) -> Self {
        Self {
            message: format!("cannot read {}: {error}", path.display()),
            line: 0,
            column: 0,
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
    /// The MDF program produced a lexical, parse or runtime diagnostic.
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

/// The last good document, its plan and the latest diagnostics.
pub struct PreviewSession {
    path: PathBuf,
    document: Option<Document>,
    diagnostics: Vec<DiagnosticInfo>,
    evaluator: Evaluator,
    plan: RenderPlan,
    plan_frame: Option<i64>,
    plan_generation: u64,
    generation: u64,
}

impl PreviewSession {
    /// Creates an empty session for `path`; call [`Self::reload`] to load it.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            document: None,
            diagnostics: Vec::new(),
            evaluator: Evaluator::new(),
            plan: RenderPlan::new(),
            plan_frame: None,
            plan_generation: 0,
            generation: 0,
        }
    }

    /// The source path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The last successfully loaded document, if any.
    #[must_use]
    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// The last good composition settings, if any.
    #[must_use]
    pub fn composition(&self) -> Option<&Composition> {
        self.document.as_ref().map(Document::composition)
    }

    /// Diagnostics from the most recent load attempt (usually zero or one).
    #[must_use]
    pub fn diagnostics(&self) -> &[DiagnosticInfo] {
        &self.diagnostics
    }

    /// Increments on every successful load, so callers can invalidate caches.
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

    /// Reads and executes the MDF file again.
    ///
    /// On success the last good document is replaced, diagnostics are cleared
    /// and the generation is bumped. On an MDF diagnostic the previous
    /// document is kept, the diagnostic is stored and the generation is
    /// unchanged. An I/O error is stored with an empty source position.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Io`] or [`SessionError::Diagnostic`] and leaves
    /// the stored diagnostics describing the failure.
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
        match motioner_dsl::run(&source, &Limits::default()) {
            Ok(document) => {
                self.document = Some(document);
                self.diagnostics.clear();
                self.generation = self.generation.saturating_add(1);
                self.plan_frame = None;
                Ok(())
            }
            Err(diagnostic) => {
                let info = DiagnosticInfo::from_diagnostic(&diagnostic);
                self.diagnostics = vec![info.clone()];
                Err(SessionError::Diagnostic(info))
            }
        }
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
        assert_eq!(plan.height(), document.composition().height());
        assert_eq!(session.plan_frame(), Some(120));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn a_diagnostic_keeps_the_previous_document_and_reports_line_and_column() {
        let path = temp_path("diagnostic");
        write(&path, HELLO);
        let mut session = PreviewSession::new(&path);
        session.reload().unwrap();
        let generation = session.generation();

        write(&path, "comp(name=\"Broken\"\nbox = rect(10, 10)\n");
        let error = session.reload().unwrap_err();
        assert!(matches!(error, SessionError::Diagnostic(_)));
        assert_eq!(session.generation(), generation);
        assert!(session.document().is_some(), "the last good document stays");
        let diagnostic = &session.diagnostics()[0];
        assert!(diagnostic.line >= 1, "the diagnostic points at a line");
        assert!(!diagnostic.message.is_empty());
        fs::remove_file(&path).ok();
    }

    #[test]
    fn a_diagnostic_before_any_good_load_leaves_no_document() {
        let path = temp_path("never_good");
        write(&path, "this is not mdf\n");
        let mut session = PreviewSession::new(&path);
        let error = session.reload().unwrap_err();
        assert!(matches!(error, SessionError::Diagnostic(_)));
        assert!(session.document().is_none());
        assert_eq!(session.generation(), 0);
        assert!(session.evaluate(0).is_none());
        fs::remove_file(&path).ok();
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let path = temp_path("missing");
        fs::remove_file(&path).ok();
        let mut session = PreviewSession::new(&path);
        let error = session.reload().unwrap_err();
        assert!(matches!(error, SessionError::Io { .. }));
        assert_eq!(session.diagnostics()[0].line, 0);
    }

    #[test]
    fn a_successful_reload_clears_diagnostics_and_bumps_the_generation() {
        let path = temp_path("recover");
        write(&path, "nope\n");
        let mut session = PreviewSession::new(&path);
        assert!(session.reload().is_err());
        write(&path, HELLO);
        session.reload().unwrap();
        assert!(session.diagnostics().is_empty());
        assert_eq!(session.generation(), 1);
        fs::remove_file(&path).ok();
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
        assert_eq!(session.plan_frame(), Some(10));
        session.evaluate(11).unwrap();
        assert_eq!(session.plan_frame(), Some(11));
        fs::remove_file(&path).ok();
    }
}
