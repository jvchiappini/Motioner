//! The editor without a window: document session, transport, selection,
//! view state, undo, and the one executor of [`Command`]s (D-0024).
//!
//! The preview window, `motioner edit`, `motioner query` and the live
//! control endpoint all drive this type, so an action has one
//! implementation whoever asks for it. Gestures (drags) open a gesture with
//! [`Editor::begin_gesture`]: commands executed while it is open resolve
//! against the state at the start of the drag and coalesce into one undo
//! step, exactly like a single command.

mod bake;
pub(crate) mod channels;
mod fields;
mod graph;
mod query;
mod rebind;
mod resolve;
mod rig_edit;
mod rig_json;
mod view;

use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use motioner_core::{
    split_property, Document, Ease, Frame, NoTextMetrics, NodeId, OverrideAction, OverrideEntry,
    OverrideSet, OverrideValue, TextMetrics,
};
use motioner_dsl::{AssetResolver, CodeEdit, NoAssets, Plan, Strategy, ValueOrigins};

use crate::command::{Command, CommandError, ErrorKind, Panel};
use crate::edit::{self, EditCommand, KeyRef, UndoStack, UndoState};
use crate::inspect;
use crate::layout::PanelSizes;
use crate::timeline_model::TimelineRows;
use crate::{outline_rows, InspectModel, OutlineRow, PreviewSession, Transport};

pub use channels::{channel_name, Channel, CHANNELS};
pub use motioner_dsl::Strategy as CodeStrategy;

/// Where a committed edit is written (D-0036).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditMode {
    /// Patch the program text and keep overrides only as a fallback.
    #[default]
    Code,
    /// Keep the D-0022 behaviour: every edit becomes an override entry.
    Overrides,
}

impl EditMode {
    /// The stable lower-case name (`code` or `overrides`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Overrides => "overrides",
        }
    }
}

/// What the last committed gesture did: whether it wrote code, the strategies
/// it used, and how many edits had to stay as overrides (with reasons).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitSummary {
    /// Whether the whole gesture was written into the program.
    pub code: bool,
    /// Strategies used by the code edits, in edit order.
    pub strategies: Vec<Strategy>,
    /// Number of edits that stayed as overrides.
    pub overrides: usize,
    /// Why the effect stayed as an override, if it did.
    pub reasons: Vec<String>,
}

impl CommitSummary {
    fn code(strategies: Vec<Strategy>) -> Self {
        Self {
            code: true,
            strategies,
            overrides: 0,
            reasons: Vec::new(),
        }
    }

    fn overrides(count: usize, reasons: Vec<String>) -> Self {
        Self {
            code: false,
            strategies: Vec::new(),
            overrides: count,
            reasons,
        }
    }
}

/// Whether a channel has a key at `frame`.
#[must_use]
pub fn channel_has_key(document: &Document, node: NodeId, channel: Channel, frame: Frame) -> bool {
    channels::keys(document, node, channel).contains(&frame)
}

/// Editor view state that is not part of the document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewState {
    /// Safe-area guides on the canvas.
    pub safe_area: bool,
    /// Snapping while dragging on the canvas.
    pub snap: bool,
    /// Panel sizes in logical pixels.
    pub sizes: PanelSizes,
    /// Scroll offsets in logical pixels: layers, inspector, timeline.
    pub scroll: [f64; 3],
    /// Canvas zoom in window pixels per composition pixel; `None` fits the
    /// composition to the canvas (D-0026).
    pub zoom: Option<f64>,
    /// Canvas pan in composition pixels, positive right and down.
    pub pan: (f64, f64),
    /// The fit scale of the last laid-out canvas, kept so a wheel step while
    /// fitted is relative to what the user sees. The preview updates it every
    /// frame; headless runs leave the default.
    pub fit_zoom: f64,
}

impl ViewState {
    /// The scroll offset of `panel`.
    #[must_use]
    pub fn scroll(&self, panel: Panel) -> f64 {
        self.scroll[panel_index(panel)]
    }

    /// Sets the scroll offset of `panel` (never negative).
    pub fn set_scroll(&mut self, panel: Panel, offset: f64) {
        self.scroll[panel_index(panel)] = offset.max(0.0);
    }
}

fn panel_index(panel: Panel) -> usize {
    match panel {
        Panel::Layers => 0,
        Panel::Inspector => 1,
        Panel::Timeline => 2,
    }
}

/// What the caller should do after a command, beyond redrawing.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Nothing.
    None,
    /// Close the editor.
    Quit,
    /// The program was reloaded from disk (painter caches are stale).
    Reloaded,
    /// Copy this text to the clipboard.
    Copy(String),
    /// Write a copy of the project under this destination.
    SaveAs(std::path::PathBuf),
    /// Close the project and show the start screen.
    CloseProject,
    /// Open a project folder, manifest or program.
    OpenProject(std::path::PathBuf),
    /// Create a project and open it.
    NewProject(crate::start::NewProjectRequest),
    /// Import a file (empty opens the picker).
    ImportAsset(String),
    /// Remove an asset and its blob.
    RemoveAsset(String),
    /// Rename an asset key.
    RenameAsset {
        /// Registry key.
        key: String,
        /// New key.
        name: String,
    },
    /// Append the layer for an asset.
    PlaceAsset {
        /// Registry key.
        key: String,
        /// Drop frame.
        frame: Option<i64>,
        /// Drop position in composition pixels.
        pos: Option<(f64, f64)>,
    },
    /// Remove every unreferenced asset.
    RemoveUnusedAssets,
    /// Restore the offered backup.
    RestoreBackup,
    /// Dismiss the recovery offer.
    DismissRecovery,
    /// Show or hide the code panel.
    ShowCode {
        /// Whether the panel is shown.
        on: bool,
    },
    /// Show or hide the pending-overrides list.
    ShowOverrides {
        /// Whether the list is shown.
        on: bool,
    },
    /// The project's edit-mode setting changed; persist it in `editor.motion`.
    EditModeChanged {
        /// `true` when edits are kept as overrides.
        overrides: bool,
    },
}

/// The result of a command that ran.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// What happened, in one line.
    pub summary: String,
    /// What the caller should do.
    pub effect: Effect,
}

impl Outcome {
    fn done(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            effect: Effect::None,
        }
    }
}

/// The state captured when a drag starts.
struct Gesture {
    set: OverrideSet,
    document: Option<Document>,
    keys: Vec<KeyRef>,
}

/// The editor state and command executor.
pub struct Editor {
    session: PreviewSession,
    transport: Transport,
    outline: Vec<OutlineRow>,
    timeline: TimelineRows,
    inspector: InspectModel,
    inspector_frame: Option<Frame>,
    expanded: BTreeSet<usize>,
    pinned: BTreeSet<usize>,
    /// Groups whose descendants are hidden in the layers panel and timeline.
    collapsed: BTreeSet<usize>,
    /// Layer filter text (`filter_layers`), empty for no filter.
    filter: String,
    /// Outline indices visible after collapsing and filtering.
    visible: Vec<usize>,
    /// Layers shown exclusively in the preview (`solo`).
    solo: BTreeSet<NodeId>,
    /// Layers hidden from the preview (`shy`).
    shy: BTreeSet<NodeId>,
    /// Whether shy layers are shown anyway (`show_shy`).
    show_shy: bool,
    /// The open graph editor.
    graph: Option<crate::graph::GraphModel>,
    only_animated: bool,
    motion_blur_preview: bool,
    selected: Option<NodeId>,
    selected_keys: Vec<KeyRef>,
    undo: UndoStack,
    gesture: Option<Gesture>,
    /// Where committed edits are written (D-0036; `code` by default).
    edit_mode: EditMode,
    /// What the last commit did, for the CLI and the fallback chip.
    last_commit: Option<CommitSummary>,
    /// The program before and after the last commit (for the code diff and
    /// the code panel).
    last_code: Option<(String, String)>,
    /// The exact edit the last commit applied (for the code-panel highlight).
    last_edit: Option<CodeEdit>,
    /// A transient note, for example that disk won over a pending gesture.
    notice: Option<String>,
    view: ViewState,
    /// Families the text inspector offers: bundled plus project faces.
    font_families: Vec<String>,
    model_generation: u64,
    /// Generation the full outline (including visibility runs) was built for.
    outline_generation: Option<u64>,
}

impl Editor {
    /// An editor over a loaded (or failed) session, selecting outline row
    /// `select` when given.
    #[must_use]
    pub fn new(session: PreviewSession, transport: Transport, select: Option<usize>) -> Self {
        let mut editor = Self {
            session,
            transport,
            outline: Vec::new(),
            timeline: TimelineRows::default(),
            inspector: empty_inspector(),
            inspector_frame: None,
            expanded: BTreeSet::new(),
            pinned: BTreeSet::new(),
            collapsed: BTreeSet::new(),
            filter: String::new(),
            visible: Vec::new(),
            solo: BTreeSet::new(),
            shy: BTreeSet::new(),
            show_shy: false,
            graph: None,
            only_animated: false,
            motion_blur_preview: false,
            selected: None,
            selected_keys: Vec::new(),
            undo: UndoStack::default_limit(),
            gesture: None,
            edit_mode: EditMode::Code,
            last_commit: None,
            last_code: None,
            last_edit: None,
            notice: None,
            view: ViewState {
                safe_area: false,
                snap: true,
                sizes: PanelSizes::default(),
                scroll: [0.0; 3],
                zoom: None,
                pan: (0.0, 0.0),
                fit_zoom: 1.0,
            },
            font_families: vec!["Inter".to_owned(), "Roboto".to_owned()],
            model_generation: u64::MAX,
            outline_generation: None,
        };
        editor.refresh_models();
        if let Some(row) = select {
            editor.select_row(row);
        }
        editor
    }

    /// Loads `path` (and its overrides file) into a new editor, paused on
    /// frame 0. Used by the headless `edit` and `query` commands.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Io`](crate::ErrorKind::Io) when the program cannot be read and
    /// [`ErrorKind::NoDocument`](crate::ErrorKind::NoDocument) when it does not compile.
    pub fn open(path: &Path) -> Result<Self, CommandError> {
        Self::open_with_metrics(path, Arc::new(NoTextMetrics))
    }

    /// Opens a program with a CPU text measurer installed (D-0034).
    ///
    /// # Errors
    ///
    /// Same as [`Self::open`].
    pub fn open_with_metrics(
        path: &Path,
        metrics: Arc<dyn TextMetrics>,
    ) -> Result<Self, CommandError> {
        Self::open_with_assets(path, metrics, Rc::new(NoAssets))
    }

    /// Opens a program with text metrics and the host's `use()` resolver.
    ///
    /// This is the constructor `query`, `edit`, `bake`, the tool host and the
    /// tests use for a program that imports an SVG symbol or a `.motion`
    /// module (D-0051/D-0056). There is no second resolution path: every
    /// compile of this session uses the session's single `compile_source`.
    ///
    /// # Errors
    ///
    /// Same as [`Self::open`].
    pub fn open_with_assets(
        path: &Path,
        metrics: Arc<dyn TextMetrics>,
        assets: Rc<dyn AssetResolver>,
    ) -> Result<Self, CommandError> {
        let mut session = PreviewSession::new(path);
        session.set_text_metrics(metrics);
        session.set_asset_resolver(assets);
        session.reload().map_err(|error| match error {
            crate::SessionError::Io { .. } => CommandError::new(ErrorKind::Io, error.to_string()),
            crate::SessionError::Diagnostic(_) => {
                CommandError::new(ErrorKind::NoDocument, error.to_string())
            }
        })?;
        let (rate, duration) = session
            .composition()
            .map(|c| (c.frame_rate(), c.duration()))
            .ok_or_else(|| CommandError::new(ErrorKind::NoDocument, "no composition"))?;
        Ok(Self::new(session, Transport::new(rate, duration), None))
    }

    /// An editor with no document, for the start screen.
    ///
    /// Nothing is read from disk until a project is opened.
    ///
    /// # Panics
    ///
    /// Never in practice: `60/1` is a valid frame rate.
    #[must_use]
    pub fn blank(metrics: Arc<dyn TextMetrics>) -> Self {
        let mut session = PreviewSession::new(std::path::PathBuf::new());
        session.set_text_metrics(metrics);
        let rate = motioner_core::FrameRate::new(60, 1).expect("60/1 is a valid rate");
        Self::new(session, Transport::new(rate, 1), None)
    }

    /// Replaces the loaded document, keeping the panel layout.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NoDocument`] when the new session has no composition.
    pub fn replace_session(&mut self, session: PreviewSession) -> Result<(), CommandError> {
        let (rate, duration) = session
            .composition()
            .map(|composition| (composition.frame_rate(), composition.duration()))
            .ok_or_else(|| CommandError::new(ErrorKind::NoDocument, "no composition"))?;
        self.session = session;
        self.transport = Transport::new(rate, duration);
        self.outline.clear();
        self.timeline = TimelineRows::default();
        self.inspector = empty_inspector();
        self.inspector_frame = None;
        self.expanded.clear();
        self.pinned.clear();
        self.collapsed.clear();
        self.filter.clear();
        self.visible.clear();
        self.solo.clear();
        self.shy.clear();
        self.show_shy = false;
        self.graph = None;
        self.only_animated = false;
        self.motion_blur_preview = false;
        self.selected = None;
        self.selected_keys.clear();
        self.undo = UndoStack::default_limit();
        self.gesture = None;
        self.last_commit = None;
        self.notice = None;
        self.view.zoom = None;
        self.view.pan = (0.0, 0.0);
        self.model_generation = u64::MAX;
        self.outline_generation = None;
        self.refresh_models();
        Ok(())
    }

    // ---- read access -----------------------------------------------------

    /// Sets the families the inspector offers in the Text section.
    pub fn set_font_families(&mut self, families: Vec<String>) {
        if families.is_empty() {
            return;
        }
        self.font_families = families;
        self.refresh_models();
    }

    /// The document session.
    #[must_use]
    pub fn session(&self) -> &PreviewSession {
        &self.session
    }

    /// The session, for evaluation and file polling.
    pub fn session_mut(&mut self) -> &mut PreviewSession {
        &mut self.session
    }

    /// The transport.
    #[must_use]
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// The transport, for advancing playback and scrubbing.
    pub fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
    }

    /// The playhead.
    #[must_use]
    pub fn frame(&self) -> Frame {
        self.transport.frame()
    }

    /// The layer rows.
    #[must_use]
    pub fn outline(&self) -> &[OutlineRow] {
        &self.outline
    }

    /// The timeline rows.
    #[must_use]
    pub fn timeline(&self) -> &TimelineRows {
        &self.timeline
    }

    /// The inspector model for the selection at the playhead.
    #[must_use]
    pub fn inspector(&self) -> &InspectModel {
        &self.inspector
    }

    /// Expanded outline rows.
    #[must_use]
    pub fn expanded(&self) -> &BTreeSet<usize> {
        &self.expanded
    }

    /// Outline indices visible after collapsing and filtering, in document
    /// order.
    #[must_use]
    pub fn visible_outline(&self) -> &[usize] {
        &self.visible
    }

    /// The current layer filter text.
    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Whether a layer is soloed in the preview.
    #[must_use]
    pub fn is_solo(&self, node: NodeId) -> bool {
        self.solo.contains(&node)
    }

    /// Whether a layer is shy (hidden from the preview by default).
    #[must_use]
    pub fn is_shy(&self, node: NodeId) -> bool {
        self.shy.contains(&node)
    }

    /// Whether shy layers are currently shown.
    #[must_use]
    pub const fn show_shy(&self) -> bool {
        self.show_shy
    }

    /// Whether solo/shy state hides anything in the preview.
    #[must_use]
    pub fn has_preview_filter(&self) -> bool {
        !self.solo.is_empty() || (!self.show_shy && !self.shy.is_empty())
    }

    /// Nodes the preview must not draw because of solo/shy state.
    ///
    /// `solo` hides every layer that is not soloed (and its descendants);
    /// `shy` hides the marked layer unless [`Self::show_shy`] is on. Solo wins
    /// over shy for the soloed branch. The render and still paths never see
    /// this set.
    #[must_use]
    pub fn preview_hidden(&self) -> std::collections::BTreeSet<NodeId> {
        let Some(document) = self.document() else {
            return std::collections::BTreeSet::new();
        };
        let mut hidden = std::collections::BTreeSet::new();
        let solo_active = !self.solo.is_empty();
        for row in document.nodes() {
            let node = row.id();
            let soloed = self.solo.contains(&node) || self.has_solo_ancestor(document, node);
            let hide = if solo_active {
                !soloed && !self.has_solo_descendant(document, node)
            } else {
                self.shy.contains(&node) && !self.show_shy
            };
            if hide {
                hidden.insert(node);
            }
        }
        hidden
    }

    fn has_solo_ancestor(&self, document: &Document, node: NodeId) -> bool {
        let mut current = document.node(node).and_then(motioner_core::Node::parent);
        while let Some(parent) = current {
            if self.solo.contains(&parent) {
                return true;
            }
            current = document.node(parent).and_then(motioner_core::Node::parent);
        }
        false
    }

    fn has_solo_descendant(&self, document: &Document, node: NodeId) -> bool {
        let Some(current) = document.node(node) else {
            return false;
        };
        current
            .children()
            .iter()
            .any(|child| self.solo.contains(child) || self.has_solo_descendant(document, *child))
    }

    /// Whether only animated property rows are shown.
    #[must_use]
    pub fn only_animated(&self) -> bool {
        self.only_animated
    }

    /// Whether the preview should show motion blur (D-0037).
    #[must_use]
    pub const fn motion_blur_preview(&self) -> bool {
        self.motion_blur_preview
    }

    /// The selected node.
    #[must_use]
    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }

    /// The outline row of the selected node.
    #[must_use]
    pub fn selected_row(&self) -> Option<usize> {
        let selected = self.selected?;
        self.outline.iter().position(|row| row.node == selected)
    }

    /// The selected keys.
    #[must_use]
    pub fn selected_keys(&self) -> &[KeyRef] {
        &self.selected_keys
    }

    /// View state (guides, snapping, panel sizes, scroll).
    #[must_use]
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// View state, for splitter drags and scroll clamping.
    pub fn view_mut(&mut self) -> &mut ViewState {
        &mut self.view
    }

    /// Undo and redo depths.
    #[must_use]
    pub fn history(&self) -> (usize, usize) {
        (self.undo.len(), self.undo.redo_len())
    }

    /// Where committed edits are written (`code` by default, D-0036).
    #[must_use]
    pub const fn edit_mode(&self) -> EditMode {
        self.edit_mode
    }

    /// Selects the write-through sink or the D-0022 override fallback.
    pub fn set_edit_mode(&mut self, mode: EditMode) {
        self.edit_mode = mode;
    }

    /// What the last committed edit did (strategies, overrides kept, reasons).
    #[must_use]
    pub fn last_commit(&self) -> Option<&CommitSummary> {
        self.last_commit.as_ref()
    }

    /// A transient note for the UI (currently: a disk/gesture conflict).
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The program with the lines the last gesture wrote marked.
    ///
    /// The ranges come from the applied [`CodeEdit`], not from re-diffing the
    /// text, so the panel can never disagree with what was written.
    #[must_use]
    pub fn code_panel(&self) -> crate::CodePanel {
        crate::CodePanel::build(self.session.source(), self.last_edit.as_ref())
    }

    /// One row per pending override, with the last commit's reasons.
    #[must_use]
    pub fn override_rows(&self) -> Vec<crate::OverrideRow> {
        let reasons = self
            .last_commit
            .as_ref()
            .map_or_else(Vec::new, |report| report.reasons.clone());
        crate::overrides_panel::rows(self.session.overrides(), &reasons)
    }

    /// The unified diff of the last commit, or an empty string when the
    /// program did not change.
    #[must_use]
    pub fn last_diff(&self) -> String {
        let Some((before, after)) = &self.last_code else {
            return String::new();
        };
        let path = self
            .session
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("program.motion");
        crate::unified_diff(before, after, path)
    }

    /// The JSON result of a committed command: the strategy per command, the
    /// overrides kept with their reasons and the code diff.
    ///
    /// `None` when the command did not edit anything.
    #[must_use]
    pub fn commit_json(&self, outcome: &Outcome) -> Option<String> {
        let report = self.last_commit.as_ref()?;
        let mut j = crate::Json::new();
        j.begin_object();
        j.key("summary").string(&outcome.summary);
        j.key("code").boolean(report.code);
        j.key("strategies").begin_array();
        for strategy in &report.strategies {
            j.string(strategy.name());
        }
        j.end_array();
        j.key("overrides_kept")
            .int(i64::try_from(report.overrides).unwrap_or(i64::MAX));
        j.key("fallback_reasons").begin_array();
        for reason in &report.reasons {
            j.string(reason);
        }
        j.end_array();
        j.key("path").string(
            self.session
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("program.motion"),
        );
        j.key("diff").string(&self.last_diff());
        j.end_object();
        Some(j.finish())
    }

    /// Converts one override entry into code through the write-through sink.
    ///
    /// On success the program is written once, the override is removed in the
    /// same undo entry and `Ok(Ok(()))` is returned. When no exact code edit
    /// exists the override stays untouched and `Ok(Err(reason))` carries the
    /// planner's explanation. `Err` is a hard failure (no document, I/O).
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NoDocument`] when nothing is loaded and
    /// [`ErrorKind::BadValue`] when the written program does not compile.
    pub fn convert_override(
        &mut self,
        entry: &OverrideEntry,
    ) -> Result<Result<(), String>, CommandError> {
        let document = self.require_document()?.clone();
        let origins = self.session.origins().cloned().ok_or_else(|| {
            CommandError::new(ErrorKind::NoDocument, "the program has no recorded origins")
        })?;
        let Some(command) = Self::command_for_override(entry) else {
            return Ok(Err(format!(
                "{:?} is editor state; it cannot become program code",
                entry.action
            )));
        };
        let source = self.session.source().to_owned();
        let before = self.session.overrides().clone();
        let written = self.try_write_through(
            std::slice::from_ref(&command),
            &document,
            &before,
            &source,
            &origins,
        )?;
        if written {
            self.undo.record(UndoState {
                source: source.clone(),
                overrides: before,
            });
            self.refresh_models();
            return Ok(Ok(()));
        }
        let reason = self
            .last_commit
            .as_ref()
            .and_then(|report| report.reasons.first().cloned())
            .unwrap_or_else(|| "no exact code edit".to_owned());
        Ok(Err(reason))
    }

    /// The [`EditCommand`] one override entry stands for.
    fn command_for_override(entry: &OverrideEntry) -> Option<EditCommand> {
        match &entry.action {
            OverrideAction::Set(value) => Some(EditCommand::SetValue {
                id: entry.id.clone(),
                property: entry.property.clone(),
                value: value.clone(),
            }),
            OverrideAction::Key { frame, value, ease } => {
                let (base, axis) = split_property(&entry.property);
                Some(EditCommand::SetKey {
                    id: entry.id.clone(),
                    property: base.to_owned(),
                    axis,
                    frame: *frame,
                    value: value.clone(),
                    ease: *ease,
                })
            }
            OverrideAction::Visible(visible) => Some(EditCommand::SetVisible {
                id: entry.id.clone(),
                visible: *visible,
            }),
            OverrideAction::Unkey { frame } => {
                let (base, axis) = split_property(&entry.property);
                Some(EditCommand::RemoveKey {
                    id: entry.id.clone(),
                    property: base.to_owned(),
                    axis,
                    frame: *frame,
                })
            }
            OverrideAction::Locked(_) => None,
        }
    }

    /// The identity of the selected layer, when one is selected.
    #[must_use]
    pub fn selected_identity(&self) -> Option<String> {
        self.selected
            .and_then(|node| self.document()?.node(node))
            .map(|node| node.identity().as_str().to_owned())
    }

    /// The program before and after the last committed edit.
    #[must_use]
    pub fn last_program_change(&self) -> Option<(&str, &str)> {
        self.last_code
            .as_ref()
            .map(|(before, after)| (before.as_str(), after.as_str()))
    }

    /// The identity of `node`.
    #[must_use]
    pub fn node_identity(&self, node: NodeId) -> String {
        self.document()
            .and_then(|document| document.node(node))
            .map(|node| node.identity().as_str().to_owned())
            .unwrap_or_default()
    }

    /// Whether `node` is locked in the editor.
    #[must_use]
    pub fn node_locked(&self, node: NodeId) -> bool {
        let id = self.node_identity(node);
        self.session
            .overrides()
            .get(&id, motioner_core::LOCKED, None)
            .is_some_and(|entry| entry.action == OverrideAction::Locked(true))
    }

    /// Overrides whose node no longer exists.
    #[must_use]
    pub fn orphans(&self) -> Vec<OverrideEntry> {
        let Some(document) = self.document() else {
            return Vec::new();
        };
        self.session
            .overrides()
            .entries()
            .iter()
            .filter(|entry| document.find(&entry.id).is_none())
            .cloned()
            .collect()
    }

    fn document(&self) -> Option<&Document> {
        self.session.document()
    }

    fn require_document(&self) -> Result<&Document, CommandError> {
        self.document().ok_or_else(|| {
            CommandError::new(
                ErrorKind::NoDocument,
                "the program has an error; fix it and reload",
            )
        })
    }

    fn rate(&self) -> Result<motioner_core::FrameRate, CommandError> {
        Ok(self.require_document()?.composition().frame_rate())
    }

    // ---- models ------------------------------------------------------------

    /// Rebuilds the outline, timeline and inspector after the document
    /// changed (reload or edit) and keeps the selection valid.
    pub fn refresh_models(&mut self) {
        let generation = self.session.generation();
        if self.outline_generation != Some(generation) {
            self.outline = self.document().map(outline_rows).unwrap_or_default();
            self.outline_generation = Some(generation);
        }
        crate::outline::apply_visibility(&mut self.outline, &self.collapsed, &self.filter);
        self.visible = self
            .outline
            .iter()
            .enumerate()
            .filter(|(_, row)| row.visible)
            .map(|(index, _)| index)
            .collect();
        if self.selected.is_some() && self.selected_row().is_none() {
            self.selected = None;
        }
        self.rebuild_timeline();
        self.rebuild_inspector();
        self.rebuild_graph();
        self.model_generation = self.session.generation();
    }

    /// Rebuilds the models when the document or the playhead changed since
    /// the last call. Cheap when nothing changed; call it once per frame.
    pub fn sync_models(&mut self) {
        if self.model_generation != self.session.generation() {
            self.refresh_models();
        } else if self.inspector_frame != Some(self.frame()) {
            self.rebuild_inspector();
        }
    }

    fn rebuild_timeline(&mut self) {
        self.timeline = match self.session.document() {
            Some(document) => {
                TimelineRows::compute(document, &self.outline, &self.expanded, self.only_animated)
            }
            None => TimelineRows::default(),
        };
    }

    fn rebuild_inspector(&mut self) {
        let frame = self.frame();
        self.inspector_frame = Some(frame);
        let Some(document) = self.session.document() else {
            self.inspector = empty_inspector();
            return;
        };
        let fallback = self
            .selected_row()
            .and_then(|row| self.outline.get(row))
            .map_or("", |row| row.name.as_str());
        let mut model = match self.selected {
            Some(node) => inspect::inspect_node(
                document,
                node,
                frame,
                Some(self.session.overrides()),
                fallback,
                &self.font_families,
                inspect::MediaState {
                    muted: !document
                        .node(node)
                        .is_some_and(motioner_core::Node::is_visible),
                    solo: self.solo.contains(&node),
                },
            ),
            None => Some(inspect::inspect_comp(
                document.composition(),
                self.outline.len(),
            )),
        }
        .unwrap_or_else(empty_inspector);
        if !self.selected_keys.is_empty() {
            let mut eases = self.selected_keys.iter().map(|key| self.key_ease(key));
            let first = eases.next().flatten();
            let current = if eases.all(|ease| ease == first) {
                first
            } else {
                None
            };
            model.rows.push(inspect::Row {
                section: "Keys",
                label: "Ease".to_owned(),
                value: inspect::RowValue::Ease { current },
                channel: None,
            });
            // A bezier key is not a preset: its four control numbers are
            // editable fields. Each commit is one `set_ease(bezier(...))`
            // command, so the drag and the typed value are one undo step
            // (D-0037 quantisation, D-0024 command path).
            if let Some(curve) = self.selected_bezier() {
                let (x1, y1, x2, y2) = curve.points();
                for (part, value) in [
                    (inspect::BezierPart::X1, x1),
                    (inspect::BezierPart::Y1, y1),
                    (inspect::BezierPart::X2, x2),
                    (inspect::BezierPart::Y2, y2),
                ] {
                    model.rows.push(inspect::Row {
                        section: "Keys",
                        label: format!("Bezier {}", part.name()),
                        value: inspect::RowValue::Single(inspect::Field {
                            target: inspect::FieldTarget::Bezier(part),
                            label: part.name().to_owned(),
                            value,
                            text: crate::outline::number(value),
                            factor: 1.0,
                            suffix: "",
                        }),
                        channel: None,
                    });
                }
            }
        }
        self.inspector = model;
    }

    /// Reloads both files from disk. Returns `false` (keeping the last good
    /// document) when the program does not compile.
    ///
    /// The selection is kept by node identity, because an external edit may
    /// renumber the provenance keys. If a gesture was open, disk wins: the
    /// gesture is discarded and the conflict reported in [`Self::notice`].
    pub fn reload(&mut self, now: Duration) -> bool {
        let previous = self.selected.and_then(|node| {
            self.document()
                .and_then(|document| document.node(node))
                .map(|node| node.identity().as_str().to_owned())
        });
        if self.gesture.take().is_some() {
            self.undo.end_gesture();
            self.notice =
                Some("the file changed on disk; the pending gesture was discarded".to_owned());
        }
        if self.session.reload().is_err() {
            self.refresh_models();
            return false;
        }
        if let Some(composition) = self.session.composition() {
            self.transport
                .retime(composition.frame_rate(), composition.duration(), now);
        }
        self.selected = previous.and_then(|identity| {
            self.document()
                .and_then(|document| document.find(&identity))
        });
        self.refresh_models();
        true
    }

    // ---- gestures ----------------------------------------------------------

    /// Starts a drag: edits until [`Self::end_gesture`] resolve against the
    /// current state and form one undo step.
    pub fn begin_gesture(&mut self) {
        self.undo.begin_gesture();
        self.gesture = Some(Gesture {
            set: self.session.overrides().clone(),
            document: self.document().cloned(),
            keys: self.selected_keys.clone(),
        });
    }

    /// Ends the drag.
    ///
    /// In code mode the whole gesture is planned, applied and written as one
    /// program write; when any edit cannot be written exactly, the gesture
    /// stays as overrides (D-0036).
    ///
    /// # Errors
    ///
    /// A typed [`CommandError`] when the program or the overrides file cannot
    /// be written.
    pub fn end_gesture(&mut self) -> Result<(), CommandError> {
        self.undo.end_gesture();
        let Some(gesture) = self.gesture.take() else {
            return Ok(());
        };
        if self.edit_mode == EditMode::Overrides {
            return Ok(());
        }
        self.commit_gesture_code(&gesture)
    }

    /// Whether a drag is open.
    #[must_use]
    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    /// Writes every pending editor file now (program and overrides) instead of
    /// waiting for the debounce.
    ///
    /// A no-op while writes are disabled (the buffered CLI transaction calls
    /// [`Self::flush_now`] itself).
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Io`](crate::ErrorKind::Io) when a file cannot be written.
    pub fn save_now(&mut self) -> Result<(), CommandError> {
        if !self.session.writes_enabled() {
            return Ok(());
        }
        self.flush_now()
    }

    /// Writes the program and the overrides unconditionally (the transaction
    /// commit of `motioner edit`).
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Io`](crate::ErrorKind::Io) when a file cannot be written.
    pub fn flush_now(&mut self) -> Result<(), CommandError> {
        if self.session.path().as_os_str().is_empty() {
            return Ok(());
        }
        // Overrides first: a crash between the writes leaves the old program,
        // never a stale override on top of new code.
        self.session
            .write_overrides_now()
            .map_err(|error| CommandError::new(ErrorKind::Io, error.to_string()))?;
        self.session
            .write_program_now()
            .map(|_| ())
            .map_err(|error| CommandError::new(ErrorKind::Io, error.to_string()))
    }

    // ---- executing commands ----------------------------------------------------

    /// Runs one command. `now` is the transport clock (time since the editor
    /// started); it only matters for play and pause.
    ///
    /// # Errors
    ///
    /// A typed [`CommandError`]; the state is unchanged on error.
    #[allow(clippy::too_many_lines)] // one arm per command family
    pub fn execute(&mut self, command: &Command, now: Duration) -> Result<Outcome, CommandError> {
        // Each command reports its own commit; a view command reports nothing.
        self.last_commit = None;
        self.last_code = None;
        self.last_edit = None;
        match command {
            Command::Play
            | Command::Pause
            | Command::TogglePlay
            | Command::Step(_)
            | Command::Seek(_)
            | Command::GoToStart
            | Command::GoToEnd
            | Command::Loop(_)
            | Command::PrevKey { .. }
            | Command::NextKey { .. } => self.execute_transport(command, now),
            Command::Reload => {
                if self.reload(now) {
                    Ok(Outcome {
                        summary: "reloaded".to_owned(),
                        effect: Effect::Reloaded,
                    })
                } else {
                    let message = self
                        .session
                        .diagnostics()
                        .first()
                        .map_or_else(String::new, |d| d.message.clone());
                    Err(CommandError::new(ErrorKind::NoDocument, message))
                }
            }
            Command::Quit => Ok(Outcome {
                summary: "closing".to_owned(),
                effect: Effect::Quit,
            }),
            Command::ShowCode { on } => Ok(Outcome {
                summary: if *on {
                    "code panel shown"
                } else {
                    "code panel hidden"
                }
                .to_owned(),
                effect: Effect::ShowCode { on: *on },
            }),
            Command::ShowOverrides { on } => Ok(Outcome {
                summary: if *on {
                    "overrides shown"
                } else {
                    "overrides hidden"
                }
                .to_owned(),
                effect: Effect::ShowOverrides { on: *on },
            }),
            Command::SetEditMode { overrides } => {
                self.edit_mode = if *overrides {
                    EditMode::Overrides
                } else {
                    EditMode::Code
                };
                Ok(Outcome {
                    summary: format!("edits go to {}", self.edit_mode.name()),
                    effect: Effect::EditModeChanged {
                        overrides: *overrides,
                    },
                })
            }
            Command::Undo => self.undo_redo(true),
            Command::Redo => self.undo_redo(false),
            Command::Bake(ids) => self.bake(ids),
            Command::Save => match self.save_now() {
                Ok(()) => Ok(Outcome::done(format!(
                    "saved {}",
                    self.session.path().display()
                ))),
                Err(error) => Err(error),
            },
            Command::SaveAs { path } => Ok(Outcome {
                summary: format!("write a copy under {path}"),
                effect: Effect::SaveAs(std::path::PathBuf::from(path)),
            }),
            Command::CloseProject => Ok(Outcome {
                summary: "closed the project".to_owned(),
                effect: Effect::CloseProject,
            }),
            Command::OpenProject { path } => Ok(Outcome {
                summary: format!("open {path}"),
                effect: Effect::OpenProject(std::path::PathBuf::from(path)),
            }),
            Command::ImportAsset { path } => Ok(Outcome {
                summary: if path.is_empty() {
                    "choose files to import".to_owned()
                } else {
                    format!("import {path}")
                },
                effect: Effect::ImportAsset(path.clone()),
            }),
            Command::RemoveAsset { key } => Ok(Outcome {
                summary: format!("remove asset {key}"),
                effect: Effect::RemoveAsset(key.clone()),
            }),
            Command::RenameAsset { key, name } => Ok(Outcome {
                summary: format!("rename asset {key} to {name}"),
                effect: Effect::RenameAsset {
                    key: key.clone(),
                    name: name.clone(),
                },
            }),
            Command::PlaceAsset { key, frame, x, y } => Ok(Outcome {
                summary: format!("place {key}"),
                effect: Effect::PlaceAsset {
                    key: key.clone(),
                    frame: *frame,
                    pos: x.zip(*y),
                },
            }),
            Command::RemoveUnusedAssets => Ok(Outcome {
                summary: "remove unused assets".to_owned(),
                effect: Effect::RemoveUnusedAssets,
            }),
            Command::RestoreBackup => Ok(Outcome {
                summary: "restore the backup".to_owned(),
                effect: Effect::RestoreBackup,
            }),
            Command::DismissRecovery => Ok(Outcome {
                summary: "keep the files on disk".to_owned(),
                effect: Effect::DismissRecovery,
            }),
            Command::NewProject {
                name,
                dir,
                width,
                height,
                fps,
                duration,
            } => Ok(Outcome {
                summary: format!("create {name} in {dir}"),
                effect: Effect::NewProject(crate::start::NewProjectRequest {
                    name: name.clone(),
                    location: std::path::PathBuf::from(dir),
                    width: *width,
                    height: *height,
                    fps: *fps,
                    duration: *duration,
                    background: "#000000".to_owned(),
                }),
            }),
            Command::AddLayer { .. }
            | Command::DeleteLayer { .. }
            | Command::DuplicateLayer { .. }
            | Command::ReorderLayer { .. }
            | Command::ReparentLayer { .. }
            | Command::GroupLayers { .. }
            | Command::UngroupLayer { .. } => {
                let structural = command.structural().ok_or_else(|| {
                    CommandError::new(ErrorKind::BadArgument, "not a structural command")
                })?;
                self.execute_structural(&structural)
            }
            _ if command.kind().edits() => self.execute_edit(command),
            _ => self.execute_view(command),
        }
    }

    /// Runs an editing command: resolve, commit as one undo step, update
    /// the key selection.
    fn execute_edit(&mut self, command: &Command) -> Result<Outcome, CommandError> {
        let edits = self.resolve_edit(command)?;
        // During a drag the selection starts from the press, so repeated
        // retimes do not accumulate.
        let start = self
            .gesture
            .as_ref()
            .map_or_else(|| self.selected_keys.clone(), |g| g.keys.clone());
        self.commit(&edits)?;
        self.after_edit(command, start);
        Ok(Outcome::done(command.to_text()))
    }

    fn key_ease(&self, key: &KeyRef) -> Option<Ease> {
        let document = self.document()?;
        let (node, channel) = channels::resolve(document, &key.id, &key.property).ok()?;
        channels::key_at(document, node, channel, key.frame).map(|(_, ease)| ease)
    }

    fn undo_state(&self) -> UndoState {
        UndoState {
            source: self.session.source().to_owned(),
            overrides: self.session.overrides().clone(),
        }
    }

    fn undo_redo(&mut self, undo: bool) -> Result<Outcome, CommandError> {
        let current = self.undo_state();
        let restored = if undo {
            self.undo.undo(&current)
        } else {
            self.undo.redo(&current)
        };
        let Some(state) = restored else {
            return Ok(Outcome::done(if undo {
                "nothing to undo"
            } else {
                "nothing to redo"
            }));
        };
        let applied = if state.source == self.session.source() {
            self.session
                .edit_overrides(&state.overrides, Instant::now())
        } else {
            self.session
                .apply_program(&state.source, &state.overrides, Instant::now())
        };
        if !applied {
            return Err(self.compile_error());
        }
        self.refresh_models();
        self.prune_key_selection();
        Ok(Outcome::done(if undo { "undone" } else { "redone" }))
    }

    fn compile_error(&self) -> CommandError {
        let message = self.session.diagnostics().first().map_or_else(
            || "the edit does not compile".to_owned(),
            |d| d.message.clone(),
        );
        CommandError::new(ErrorKind::BadValue, message)
    }

    /// Drops selected keys that no longer exist.
    fn prune_key_selection(&mut self) {
        let Some(document) = self.document() else {
            self.selected_keys.clear();
            return;
        };
        let keep: Vec<KeyRef> = self
            .selected_keys
            .iter()
            .filter(|key| {
                channels::resolve(document, &key.id, &key.property).is_ok_and(|(node, channel)| {
                    channels::keys(document, node, channel).contains(&key.frame)
                })
            })
            .cloned()
            .collect();
        if keep.len() != self.selected_keys.len() {
            self.selected_keys = keep;
            self.rebuild_inspector();
        }
    }

    /// Applies edits on top of the gesture start (or the current set),
    /// records one undo step and commits them (write-through in code mode).
    fn commit(&mut self, edits: &[EditCommand]) -> Result<(), CommandError> {
        let current = self.session.overrides().clone();
        let base = self
            .gesture
            .as_ref()
            .map_or_else(|| current.clone(), |g| g.set.clone());
        let mut set = base.clone();
        for edit in edits {
            edit::apply(edit, &mut set);
        }
        if set == current {
            return Ok(());
        }
        self.undo.record(UndoState {
            source: self.session.source().to_owned(),
            overrides: base,
        });

        // A drag patches the document in memory and waits for its end; the
        // whole gesture becomes one program write (D-0036).
        if self.gesture.is_some() {
            if !self.session.edit_overrides(&set, Instant::now()) {
                return Err(self.compile_error());
            }
            self.refresh_models();
            return Ok(());
        }

        if self.edit_mode == EditMode::Code {
            if let Some(document) = self.session.document().cloned() {
                if let Some(origins) = self.session.origins().cloned() {
                    let source = self.session.source().to_owned();
                    match self.try_write_through(edits, &document, &current, &source, &origins) {
                        Ok(true) => {
                            self.refresh_models();
                            return Ok(());
                        }
                        Ok(false) => {}
                        Err(error) => return Err(error),
                    }
                } else {
                    self.last_commit = Some(CommitSummary::overrides(
                        edits.len(),
                        vec!["the program has no recorded origins".to_owned()],
                    ));
                }
            }
        }

        // Fallback (or overrides mode): the D-0022 path.
        if !self.session.edit_overrides(&set, Instant::now()) {
            return Err(self.compile_error());
        }
        let already_reported = self.last_commit.as_ref().is_some_and(|report| !report.code);
        if !already_reported {
            self.last_commit = Some(CommitSummary::overrides(edits.len(), Vec::new()));
        }
        self.refresh_models();
        Ok(())
    }

    /// Plans `commands` against the program and writes the code when every
    /// command is exact.
    ///
    /// Returns `Ok(true)` when the program was written, `Ok(false)` when the
    /// caller must fall back to overrides (a report is stored either way).
    fn try_write_through(
        &mut self,
        commands: &[EditCommand],
        document: &Document,
        base: &OverrideSet,
        source: &str,
        origins: &ValueOrigins,
    ) -> Result<bool, CommandError> {
        let mut code_edits: Vec<CodeEdit> = Vec::with_capacity(commands.len());
        let mut strategies: Vec<Strategy> = Vec::with_capacity(commands.len());
        let mut reasons = Vec::new();
        for command in commands {
            match Self::plan_command(
                command,
                document,
                source,
                origins,
                self.session.text_metrics(),
            ) {
                Ok(edit) => {
                    strategies.push(edit.strategy());
                    code_edits.push(edit);
                }
                Err(reason) => reasons.push(reason),
            }
        }
        if !reasons.is_empty() || code_edits.is_empty() {
            self.last_commit = Some(CommitSummary::overrides(commands.len(), reasons));
            return Ok(false);
        }
        let Ok(combined) = CodeEdit::combine(code_edits) else {
            self.last_commit = Some(CommitSummary::overrides(
                commands.len(),
                vec!["the code edits conflict with each other".to_owned()],
            ));
            return Ok(false);
        };
        let Ok(new_source) = combined.apply(source) else {
            self.last_commit = Some(CommitSummary::overrides(
                commands.len(),
                vec!["the code edit does not fit the program".to_owned()],
            ));
            return Ok(false);
        };
        let mut delta = OverrideSet::new();
        for command in commands {
            edit::apply(command, &mut delta);
        }
        let mut expected = document.clone();
        if !delta.apply(&mut expected).is_empty() {
            self.last_commit = Some(CommitSummary::overrides(
                commands.len(),
                vec!["the expected document could not be built".to_owned()],
            ));
            return Ok(false);
        }
        // The program now contains these edits, so their override slots are
        // removed; unrelated overrides stay and are re-keyed by the edit.
        let mut remaining = base.clone();
        for entry in delta.entries() {
            remaining.remove_slot(&entry.id, &entry.property, entry.frame());
        }
        let remaining = combined.rekey_overrides(&remaining);
        let rate = document.composition().frame_rate();
        let overrides_text = remaining.to_mdf(rate);
        let overrides_arg = (!overrides_text.trim().is_empty()).then_some(overrides_text.as_str());
        if !motioner_dsl::verify_with_metrics(
            &new_source,
            overrides_arg,
            &expected,
            self.session.text_metrics(),
        ) {
            self.last_commit = Some(CommitSummary::overrides(
                commands.len(),
                vec!["the code edit did not reproduce the document".to_owned()],
            ));
            return Ok(false);
        }
        if !self
            .session
            .apply_program(&new_source, &remaining, Instant::now())
        {
            return Err(self.compile_error());
        }
        self.last_code = Some((source.to_owned(), new_source));
        self.last_edit = Some(combined);
        self.last_commit = Some(CommitSummary::code(strategies));
        Ok(true)
    }

    /// Plans one resolved edit. Key, stopwatch and name changes are already
    /// structural `EditCommand`s and go through the same planner ladder.
    fn plan_command(
        command: &EditCommand,
        document: &Document,
        source: &str,
        origins: &ValueOrigins,
        metrics: &dyn motioner_core::TextMetrics,
    ) -> Result<CodeEdit, String> {
        match motioner_dsl::plan_with_scope_and_metrics(
            source,
            origins,
            document,
            command,
            motioner_dsl::Scope::This,
            metrics,
        ) {
            Plan::Code(edit) => Ok(edit),
            Plan::Fallback(reason) => Err(reason),
        }
    }

    /// Runs a phase 5 structural gesture (add, delete, duplicate, rename,
    /// reorder, reparent, place asset) through the write-through sink.
    ///
    /// A structural command has no override equivalent: either the planner
    /// produces an exact code edit, which is written once and recorded as one
    /// undo step, or the command is refused with the planner's reason. The
    /// program file is the only sink.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NoDocument`] when no program is loaded,
    /// [`ErrorKind::Refused`] when the edit cannot be represented exactly, and
    /// [`ErrorKind::BadValue`] when the resulting program does not compile.
    pub fn execute_structural(
        &mut self,
        command: &motioner_dsl::structural::StructuralCommand,
    ) -> Result<Outcome, CommandError> {
        if self.edit_mode == EditMode::Overrides {
            return Err(CommandError::new(
                ErrorKind::Refused,
                "structural gestures change the layers themselves, which overrides cannot \
                 express; switch the project to code mode to add, delete or reparent layers",
            ));
        }
        let document = self.require_document()?.clone();
        let origins = self.session.origins().cloned().ok_or_else(|| {
            CommandError::new(ErrorKind::NoDocument, "the program has no recorded origins")
        })?;
        let source = self.session.source().to_owned();
        let edit = match motioner_dsl::structural::plan_structural_with_metrics(
            &source,
            &origins,
            &document,
            command,
            self.session.text_metrics(),
        ) {
            Plan::Code(edit) => edit,
            Plan::Fallback(reason) => return Err(CommandError::new(ErrorKind::Refused, reason)),
        };
        let new_source = edit
            .apply(&source)
            .map_err(|error| CommandError::new(ErrorKind::Refused, error.to_string()))?;
        let before = self.session.overrides().clone();
        let remaining = edit.rekey_overrides(&before);
        self.undo.record(UndoState {
            source: source.clone(),
            overrides: before,
        });
        if !self
            .session
            .apply_program(&new_source, &remaining, Instant::now())
        {
            return Err(self.compile_error());
        }
        self.last_code = Some((source.clone(), new_source));
        self.last_edit = Some(edit.clone());
        // Adding a layer selects it, so the next gesture targets the new
        // layer (the layers panel and the inspector both follow the
        // selection).
        if matches!(
            command,
            motioner_dsl::structural::StructuralCommand::AddLayer { .. }
                | motioner_dsl::structural::StructuralCommand::PlaceAsset { .. }
        ) {
            let before: BTreeSet<String> = document
                .nodes()
                .map(|node| node.identity().as_str().to_owned())
                .collect();
            if let Some(added) = self.session.document().and_then(|document| {
                document
                    .nodes()
                    .find(|node| !before.contains(node.identity().as_str()))
                    .map(motioner_core::Node::id)
            }) {
                self.selected = Some(added);
            }
        }
        self.last_commit = Some(CommitSummary::code(vec![edit.strategy()]));
        self.refresh_models();
        Ok(Outcome::done("structural edit written as code"))
    }

    /// Finishes a gesture in code mode: plans the whole accumulated change and
    /// writes it as one program write, or keeps it as overrides.
    fn commit_gesture_code(&mut self, gesture: &Gesture) -> Result<(), CommandError> {
        let current = self.session.overrides().clone();
        let delta: Vec<OverrideEntry> = current
            .entries()
            .iter()
            .filter(|entry| !gesture.set.entries().contains(entry))
            .cloned()
            .collect();
        if delta.is_empty() {
            return Ok(());
        }
        let Some(document) = gesture.document.clone() else {
            self.last_commit = Some(CommitSummary::overrides(delta.len(), Vec::new()));
            return self.flush_gesture_overrides();
        };
        let (commands, reasons) = commands_for_delta(&delta);
        let Some(origins) = self.session.origins().cloned() else {
            self.last_commit = Some(CommitSummary::overrides(
                delta.len(),
                vec!["the program has no recorded origins".to_owned()],
            ));
            return self.flush_gesture_overrides();
        };
        if !reasons.is_empty() {
            self.last_commit = Some(CommitSummary::overrides(delta.len(), reasons));
            return self.flush_gesture_overrides();
        }
        let source = self.session.source().to_owned();
        match self.try_write_through(&commands, &document, &gesture.set, &source, &origins) {
            Ok(true) => {
                self.refresh_models();
                Ok(())
            }
            Ok(false) => self.flush_gesture_overrides(),
            Err(error) => Err(error),
        }
    }

    fn flush_gesture_overrides(&mut self) -> Result<(), CommandError> {
        self.session
            .write_overrides_now()
            .map(|_| ())
            .map_err(|error| CommandError::new(ErrorKind::Io, error.to_string()))
    }

    /// The key selection after an edit, from the selection before it
    /// (`start`): retimed and moved keys keep being selected.
    fn after_edit(&mut self, command: &Command, start: Vec<KeyRef>) {
        self.selected_keys = match command {
            Command::RetimeKeys(delta) => start
                .into_iter()
                .map(|key| KeyRef {
                    frame: key.frame + delta,
                    ..key
                })
                .collect(),
            Command::MoveKey {
                id,
                property,
                from,
                to,
            }
            | Command::MoveKeyValue {
                id,
                property,
                from,
                to,
                ..
            } => {
                let frames = self
                    .rate()
                    .ok()
                    .and_then(|rate| Some((from.frame(rate).ok()?, to.frame(rate).ok()?)));
                start
                    .into_iter()
                    .map(|key| match frames {
                        Some((from, to))
                            if &key.id == id && &key.property == property && key.frame == from =>
                        {
                            KeyRef { frame: to, ..key }
                        }
                        _ => key,
                    })
                    .collect()
            }
            Command::DeleteKeys => Vec::new(),
            _ => start,
        };
        self.prune_key_selection();
        self.rebuild_inspector();
    }
}

fn empty_inspector() -> InspectModel {
    InspectModel {
        header: None,
        rows: Vec::new(),
    }
}

/// The resolved commands an accumulated gesture delta stands for.
///
/// Overrides store a key move as an `Unkey` plus a `Key`; the planner speaks
/// `MoveKeys`, so the two entries are paired back into one command. A lone
/// `Unkey` is a key removal, a lone `Key` is a key addition, `name` is a
/// rename and `locked` stays editor state.
fn commands_for_delta(delta: &[OverrideEntry]) -> (Vec<EditCommand>, Vec<String>) {
    let mut commands = Vec::with_capacity(delta.len());
    let mut reasons = Vec::new();
    let mut consumed = vec![false; delta.len()];
    for (index, entry) in delta.iter().enumerate() {
        if consumed[index] {
            continue;
        }
        let (property, axis) = split_property(&entry.property);
        match &entry.action {
            OverrideAction::Unkey { frame } => {
                let moved = delta.iter().enumerate().find(|(other, candidate)| {
                    !consumed.get(*other).copied().unwrap_or(true)
                        && candidate.id == entry.id
                        && candidate.property == entry.property
                        && matches!(candidate.action, OverrideAction::Key { .. })
                });
                if let Some((other, candidate)) = moved {
                    if let OverrideAction::Key {
                        frame: to,
                        value,
                        ease,
                    } = &candidate.action
                    {
                        consumed[other] = true;
                        commands.push(EditCommand::MoveKeys(vec![crate::edit::MoveKey {
                            key: KeyRef {
                                id: entry.id.clone(),
                                property: entry.property.clone(),
                                frame: *frame,
                            },
                            to: *to,
                            value: value.clone(),
                            ease: *ease,
                        }]));
                    }
                } else {
                    commands.push(EditCommand::RemoveKey {
                        id: entry.id.clone(),
                        property: property.to_owned(),
                        axis,
                        frame: *frame,
                    });
                }
            }
            OverrideAction::Key { frame, value, ease } => commands.push(EditCommand::SetKey {
                id: entry.id.clone(),
                property: property.to_owned(),
                axis,
                frame: *frame,
                value: value.clone(),
                ease: *ease,
            }),
            OverrideAction::Set(value) if entry.property == motioner_core::NAME => {
                let name = match value {
                    OverrideValue::Text(text) => text.clone(),
                    _ => String::new(),
                };
                commands.push(EditCommand::SetName {
                    id: entry.id.clone(),
                    name,
                });
            }
            OverrideAction::Set(value) => commands.push(EditCommand::SetValue {
                id: entry.id.clone(),
                property: entry.property.clone(),
                value: value.clone(),
            }),
            OverrideAction::Visible(visible) => commands.push(EditCommand::SetVisible {
                id: entry.id.clone(),
                visible: *visible,
            }),
            OverrideAction::Locked(_) => reasons.push(format!(
                "'{}' is editor state and stays as an override",
                entry.property
            )),
        }
    }
    (commands, reasons)
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests;
