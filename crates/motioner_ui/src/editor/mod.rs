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
mod query;
mod rebind;
mod resolve;
mod view;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use motioner_core::{
    Document, Ease, Frame, NoTextMetrics, NodeId, OverrideAction, OverrideEntry, OverrideSet,
    TextMetrics,
};

use crate::command::{Command, CommandError, ErrorKind, Panel};
use crate::edit::{self, EditCommand, KeyRef, UndoStack};
use crate::inspect;
use crate::layout::PanelSizes;
use crate::timeline_model::TimelineRows;
use crate::{outline_rows, InspectModel, OutlineRow, PreviewSession, Transport};

pub use channels::{channel_name, Channel, CHANNELS};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Nothing.
    None,
    /// Close the editor.
    Quit,
    /// The program was reloaded from disk (painter caches are stale).
    Reloaded,
    /// Copy this text to the clipboard.
    Copy(String),
}

/// The result of a command that ran.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    only_animated: bool,
    selected: Option<NodeId>,
    selected_keys: Vec<KeyRef>,
    undo: UndoStack,
    gesture: Option<Gesture>,
    view: ViewState,
    /// Families the text inspector offers: bundled plus project faces.
    font_families: Vec<String>,
    model_generation: u64,
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
            only_animated: false,
            selected: None,
            selected_keys: Vec::new(),
            undo: UndoStack::default_limit(),
            gesture: None,
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
        let mut session = PreviewSession::new(path);
        session.set_text_metrics(metrics);
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

    /// Whether only animated property rows are shown.
    #[must_use]
    pub fn only_animated(&self) -> bool {
        self.only_animated
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
        self.outline = self.document().map(outline_rows).unwrap_or_default();
        if self.selected.is_some() && self.selected_row().is_none() {
            self.selected = None;
        }
        self.rebuild_timeline();
        self.rebuild_inspector();
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
        }
        self.inspector = model;
    }

    /// Reloads both files from disk. Returns `false` (keeping the last good
    /// document) when the program does not compile.
    pub fn reload(&mut self, now: Duration) -> bool {
        if self.session.reload().is_err() {
            self.refresh_models();
            return false;
        }
        if let Some(composition) = self.session.composition() {
            self.transport
                .retime(composition.frame_rate(), composition.duration(), now);
        }
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
    pub fn end_gesture(&mut self) {
        self.gesture = None;
        self.undo.end_gesture();
    }

    /// Whether a drag is open.
    #[must_use]
    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    /// Writes the pending overrides now instead of after the debounce.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Io`](crate::ErrorKind::Io) when the file cannot be written.
    pub fn save_now(&mut self) -> Result<(), CommandError> {
        if !self.session.is_dirty() {
            return Ok(());
        }
        let far = Instant::now() + crate::WRITE_DEBOUNCE * 2;
        if self.session.flush_overrides(far) {
            Ok(())
        } else {
            let message = self.session.diagnostics().first().map_or_else(
                || "cannot write the overrides file".to_owned(),
                |d| d.message.clone(),
            );
            Err(CommandError::new(ErrorKind::Io, message))
        }
    }

    // ---- executing commands ----------------------------------------------------

    /// Runs one command. `now` is the transport clock (time since the editor
    /// started); it only matters for play and pause.
    ///
    /// # Errors
    ///
    /// A typed [`CommandError`]; the state is unchanged on error.
    pub fn execute(&mut self, command: &Command, now: Duration) -> Result<Outcome, CommandError> {
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
            Command::Undo => self.undo_redo(true),
            Command::Redo => self.undo_redo(false),
            Command::Bake(ids) => self.bake(ids),
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

    fn undo_redo(&mut self, undo: bool) -> Result<Outcome, CommandError> {
        let current = self.session.overrides().clone();
        let restored = if undo {
            self.undo.undo(&current)
        } else {
            self.undo.redo(&current)
        };
        let Some(set) = restored else {
            return Ok(Outcome::done(if undo {
                "nothing to undo"
            } else {
                "nothing to redo"
            }));
        };
        if !self.session.edit_overrides(&set, Instant::now()) {
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
    /// records one undo step and recompiles.
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
        self.undo.record(if self.gesture.is_some() {
            base
        } else {
            current
        });
        if !self.session.edit_overrides(&set, Instant::now()) {
            return Err(self.compile_error());
        }
        self.refresh_models();
        Ok(())
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

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests;
