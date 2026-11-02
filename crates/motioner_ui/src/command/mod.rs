//! The typed command layer (D-0024).
//!
//! Everything a person can do in the editor is one [`Command`]: buttons,
//! shortcuts, drag results, inspector fields, layer actions. The window turns
//! gestures into commands, `motioner edit` and `motioner ctl` parse them from
//! text, and [`crate::Editor::execute`] runs them, so there is exactly one
//! implementation of each action.
//!
//! The text form is MDF call syntax parsed by [`motioner_dsl::parse`], the
//! same grammar as programs and overrides files:
//!
//! ```text
//! seek(90)
//! select("title")
//! edit("title", "pos.x", 900)
//! key("title", "opacity", 1.5, 0.5, ease="quad_out")
//! move_key("title", "pos.x", 0.8, frame(60))
//! visible("@586/i1", false)
//! ```
//!
//! Times are seconds like the overrides file; `frame(N)` names a frame
//! exactly. Every variant has a stable name ([`CommandKind::name`]) and a
//! canonical text ([`Command::to_text`]) that parses back to the same command.

use std::fmt;

use motioner_core::{Ease, EffectKind, Frame, FrameRate, OverrideValue};

/// A point in time: seconds (as in the overrides file) or an exact frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Time {
    /// Seconds, converted with the composition rate (rounded to a frame).
    Seconds(f64),
    /// An exact frame index.
    Frame(Frame),
}

impl Time {
    /// The frame this time names at `rate`.
    ///
    /// # Errors
    ///
    /// Returns a [`ErrorKind::BadArgument`] error for an out-of-range time.
    pub fn frame(self, rate: FrameRate) -> Result<Frame, CommandError> {
        match self {
            Self::Frame(frame) => Ok(frame),
            Self::Seconds(seconds) => rate
                .seconds_to_frame(seconds)
                .map_err(|error| CommandError::new(ErrorKind::BadArgument, error.to_string())),
        }
    }
}

/// One key: node identity, channel (`pos.x`, `opacity`) and time.
#[derive(Debug, Clone, PartialEq)]
pub struct KeySpec {
    /// Node identity.
    pub id: String,
    /// Channel name, with an axis suffix for vector properties.
    pub property: String,
    /// When the key is.
    pub time: Time,
}

/// A resizable, scrollable panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// The layers panel (its width; its row list scrolls).
    Layers,
    /// The inspector (its width; its rows scroll).
    Inspector,
    /// The timeline (its height; its rows scroll).
    Timeline,
}

impl Panel {
    /// Every panel with its text name.
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Layers, "layers"),
        (Self::Inspector, "inspector"),
        (Self::Timeline, "timeline"),
    ];

    /// The text name.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(panel, _)| *panel == self)
            .map_or("layers", |(_, name)| name)
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(panel, _)| *panel)
    }
}

/// Everything a person can do in the editor.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Start playback.
    Play,
    /// Stop playback.
    Pause,
    /// Start or stop playback (Space).
    TogglePlay,
    /// Step by a signed number of frames (arrows; Shift steps 10).
    Step(i64),
    /// Move the playhead to a frame (ruler click and scrub).
    Seek(Frame),
    /// Jump to the first frame (Home).
    GoToStart,
    /// Jump to the last frame (End).
    GoToEnd,
    /// Turn looping on or off (L toggles it).
    Loop(bool),
    /// Jump to the previous key of a channel (◀ on a property row).
    PrevKey {
        /// Node identity.
        id: String,
        /// Channel.
        property: String,
    },
    /// Jump to the next key of a channel (▶ on a property row).
    NextKey {
        /// Node identity.
        id: String,
        /// Channel.
        property: String,
    },
    /// Select a layer (layers panel, timeline row, canvas pick).
    Select(String),
    /// Clear the layer selection (click on empty canvas).
    Deselect,
    /// Select the layer `n` rows below (Down) or above (Up).
    SelectRelative(i64),
    /// Show or hide a layer's property rows in the timeline (chevron).
    Expand {
        /// Node identity.
        id: String,
        /// Whether the rows are shown.
        open: bool,
    },
    /// Expand or collapse every layer (Alt+chevron).
    ExpandAll(bool),
    /// Select keys (diamond click, Shift adds, marquee).
    SelectKeys {
        /// The keys.
        keys: Vec<KeySpec>,
        /// Add to the current selection instead of replacing it.
        add: bool,
    },
    /// Clear the key selection.
    ClearKeys,
    /// Show or hide the safe-area guides (G toggles them).
    SafeArea(bool),
    /// Turn snapping on or off (top bar magnet).
    Snap(bool),
    /// Show only animated properties in the timeline (U toggles it).
    OnlyAnimated(bool),
    /// Zoom the canvas to an absolute factor (1 = 100 %), clamped to
    /// 0.05..=32 (D-0026).
    Zoom(f64),
    /// Fit the composition to the canvas and reset the pan (F, zoom pill).
    ZoomFit,
    /// Zoom by wheel steps of 1.25, keeping the composition point under the
    /// pointer fixed when `x`/`y` (composition pixels) are given.
    ZoomBy {
        /// Wheel steps; positive zooms in.
        steps: i64,
        /// Composition x under the pointer.
        x: Option<f64>,
        /// Composition y under the pointer.
        y: Option<f64>,
    },
    /// Move the view by `(dx, dy)` composition pixels (middle drag,
    /// Space+drag).
    Pan {
        /// Horizontal delta; positive moves the composition right.
        dx: f64,
        /// Vertical delta; positive moves the composition down.
        dy: f64,
    },
    /// Resize a panel (splitter drag), in logical pixels.
    PanelSize {
        /// The panel.
        panel: Panel,
        /// Width (layers, inspector) or height (timeline).
        size: f64,
    },
    /// Scroll a panel's content (mouse wheel), in logical pixels from the top.
    Scroll {
        /// The panel.
        panel: Panel,
        /// Offset from the top of the content.
        offset: f64,
    },
    /// Copy the selected layer's identity to the clipboard (Copy button).
    CopyId,
    /// Reload the program and overrides from disk (R).
    Reload,
    /// Close the editor (Escape).
    Quit,
    /// Write a static override, exactly like `override(...)` in the file.
    Set {
        /// Node identity.
        id: String,
        /// Property or content field (`pos`, `pos.x`, `fill`, `name`...).
        property: String,
        /// The value.
        value: OverrideValue,
    },
    /// Change a property the way an inspector field does: the static value
    /// when it has no keys, the key at the playhead when it is animated.
    Edit {
        /// Node identity.
        id: String,
        /// Property or content field.
        property: String,
        /// The value.
        value: OverrideValue,
    },
    /// Add or replace a key, like `override_key(...)` in the file.
    Key {
        /// Node identity.
        id: String,
        /// Channel (`pos` keys both axes).
        property: String,
        /// When.
        time: Time,
        /// The value.
        value: OverrideValue,
        /// Ease of the arriving segment; `None` keeps the replaced key's ease
        /// (linear for a new key).
        ease: Option<Ease>,
    },
    /// Remove a key, like `override_unkey(...)` in the file.
    Unkey {
        /// Node identity.
        id: String,
        /// Channel.
        property: String,
        /// When.
        time: Time,
    },
    /// Move one key to another time, keeping its value and ease.
    MoveKey {
        /// Node identity.
        id: String,
        /// Channel.
        property: String,
        /// Current time of the key.
        from: Time,
        /// New time.
        to: Time,
    },
    /// Change the ease of one key.
    Ease {
        /// Node identity.
        id: String,
        /// Channel.
        property: String,
        /// When the key is.
        time: Time,
        /// The new ease.
        ease: Ease,
    },
    /// Change the ease of every selected key (inspector ease grid).
    SetEase(Ease),
    /// Toggle a channel's stopwatch: add the first key at the playhead, or
    /// remove every key and keep the value at the playhead.
    Stopwatch {
        /// Node identity.
        id: String,
        /// Channel.
        property: String,
    },
    /// Add or remove the channel's key at the playhead (◆ button).
    ToggleKey {
        /// Node identity.
        id: String,
        /// Channel.
        property: String,
    },
    /// Key the selected layer's animated channels at the playhead (K).
    AddKeys,
    /// Delete the selected keys (Delete).
    DeleteKeys,
    /// Move the selected keys by a number of frames (key drag).
    RetimeKeys(i64),
    /// Show or hide a layer (eye).
    Visible {
        /// Node identity.
        id: String,
        /// Whether it is drawn.
        visible: bool,
    },
    /// Lock or unlock a layer (padlock).
    Lock {
        /// Node identity.
        id: String,
        /// Whether it is locked.
        locked: bool,
    },
    /// Rename a layer (double click on its name); empty removes the name.
    Rename {
        /// Node identity.
        id: String,
        /// Display name.
        name: String,
    },
    /// Move every override of one identity to another (after giving the node
    /// an explicit `.id()` in the program).
    AssignId {
        /// Current identity.
        from: String,
        /// New identity.
        to: String,
    },
    /// Delete an orphaned override (× in the orphans list).
    DeleteOrphan {
        /// The orphan's identity.
        id: String,
        /// Its property.
        property: String,
        /// Its key time, for key and unkey entries.
        time: Option<Time>,
    },
    /// Add an effect of `kind` to a layer's stack.
    EffectAdd {
        /// Node identity.
        id: String,
        /// The effect kind.
        kind: EffectKind,
    },
    /// Remove an effect from a layer's stack.
    EffectRemove {
        /// Node identity.
        id: String,
        /// The effect kind.
        kind: EffectKind,
    },
    /// Move an effect to `index` in the stack (0 is applied first).
    EffectMove {
        /// Node identity.
        id: String,
        /// The effect kind.
        kind: EffectKind,
        /// New position in the stack.
        index: usize,
    },
    /// Mask a node with another node's drawn subtree (D-0033).
    MaskSet {
        /// Masked node identity.
        id: String,
        /// Source node identity (the coverage).
        source: String,
        /// Alpha or luma coverage.
        mode: motioner_core::MaskMode,
        /// Whether the coverage is inverted.
        invert: bool,
    },
    /// Remove a node's mask.
    MaskClear {
        /// Masked node identity.
        id: String,
    },
    /// Undo the last edit (Ctrl+Z).
    Undo,
    /// Redo (Ctrl+Shift+Z, Ctrl+Y).
    Redo,
    /// Fold overrides into the program (`motioner bake`); all ids when empty.
    Bake(Vec<String>),
}

/// The variant of a [`Command`], without data: names, signatures, examples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)] // each variant is documented on `Command`
pub enum CommandKind {
    Play,
    Pause,
    TogglePlay,
    Step,
    Seek,
    GoToStart,
    GoToEnd,
    Loop,
    PrevKey,
    NextKey,
    Select,
    Deselect,
    SelectRelative,
    Expand,
    ExpandAll,
    SelectKeys,
    ClearKeys,
    SafeArea,
    Snap,
    OnlyAnimated,
    Zoom,
    ZoomFit,
    ZoomBy,
    Pan,
    PanelSize,
    Scroll,
    CopyId,
    Reload,
    Quit,
    Set,
    Edit,
    Key,
    Unkey,
    MoveKey,
    Ease,
    SetEase,
    Stopwatch,
    ToggleKey,
    AddKeys,
    DeleteKeys,
    RetimeKeys,
    Visible,
    Lock,
    Rename,
    AssignId,
    DeleteOrphan,
    EffectAdd,
    EffectRemove,
    EffectMove,
    MaskSet,
    MaskClear,
    Undo,
    Redo,
    Bake,
}

impl CommandKind {
    /// Every kind, in documentation order.
    pub const ALL: [Self; 54] = [
        Self::Play,
        Self::Pause,
        Self::TogglePlay,
        Self::Step,
        Self::Seek,
        Self::GoToStart,
        Self::GoToEnd,
        Self::Loop,
        Self::PrevKey,
        Self::NextKey,
        Self::Select,
        Self::Deselect,
        Self::SelectRelative,
        Self::Expand,
        Self::ExpandAll,
        Self::SelectKeys,
        Self::ClearKeys,
        Self::SafeArea,
        Self::Snap,
        Self::OnlyAnimated,
        Self::Zoom,
        Self::ZoomFit,
        Self::ZoomBy,
        Self::Pan,
        Self::PanelSize,
        Self::Scroll,
        Self::CopyId,
        Self::Reload,
        Self::Quit,
        Self::Set,
        Self::Edit,
        Self::Key,
        Self::Unkey,
        Self::MoveKey,
        Self::Ease,
        Self::SetEase,
        Self::Stopwatch,
        Self::ToggleKey,
        Self::AddKeys,
        Self::DeleteKeys,
        Self::RetimeKeys,
        Self::Visible,
        Self::Lock,
        Self::Rename,
        Self::AssignId,
        Self::DeleteOrphan,
        Self::EffectAdd,
        Self::EffectRemove,
        Self::EffectMove,
        Self::MaskSet,
        Self::MaskClear,
        Self::Undo,
        Self::Redo,
        Self::Bake,
    ];

    /// The stable text name (the function name in the text form).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Pause => "pause",
            Self::TogglePlay => "toggle_play",
            Self::Step => "step",
            Self::Seek => "seek",
            Self::GoToStart => "go_to_start",
            Self::GoToEnd => "go_to_end",
            Self::Loop => "loop",
            Self::PrevKey => "prev_key",
            Self::NextKey => "next_key",
            Self::Select => "select",
            Self::Deselect => "deselect",
            Self::SelectRelative => "select_relative",
            Self::Expand => "expand",
            Self::ExpandAll => "expand_all",
            Self::SelectKeys => "select_keys",
            Self::ClearKeys => "clear_keys",
            Self::SafeArea => "safe_area",
            Self::Snap => "snap",
            Self::OnlyAnimated => "only_animated",
            Self::Zoom => "zoom",
            Self::ZoomFit => "zoom_fit",
            Self::ZoomBy => "zoom_by",
            Self::Pan => "pan",
            Self::PanelSize => "panel_size",
            Self::Scroll => "scroll",
            Self::CopyId => "copy_id",
            Self::Reload => "reload",
            Self::Quit => "quit",
            Self::Set => "set",
            Self::Edit => "edit",
            Self::Key => "key",
            Self::Unkey => "unkey",
            Self::MoveKey => "move_key",
            Self::Ease => "ease",
            Self::SetEase => "set_ease",
            Self::Stopwatch => "stopwatch",
            Self::ToggleKey => "toggle_key",
            Self::AddKeys => "add_keys",
            Self::DeleteKeys => "delete_keys",
            Self::RetimeKeys => "retime_keys",
            Self::Visible => "visible",
            Self::Lock => "lock",
            Self::Rename => "rename",
            Self::AssignId => "assign_id",
            Self::DeleteOrphan => "delete_orphan",
            Self::EffectAdd => "effect_add",
            Self::EffectRemove => "effect_remove",
            Self::EffectMove => "effect_move",
            Self::MaskSet => "mask_set",
            Self::MaskClear => "mask_clear",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Bake => "bake",
        }
    }

    /// The kind with this text name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.name() == name)
    }

    /// The call signature shown in help and errors.
    #[must_use]
    pub const fn signature(self) -> &'static str {
        match self {
            Self::Play => "play()",
            Self::Pause => "pause()",
            Self::TogglePlay => "toggle_play()",
            Self::Step => "step(frames)",
            Self::Seek => "seek(frame)",
            Self::GoToStart => "go_to_start()",
            Self::GoToEnd => "go_to_end()",
            Self::Loop => "loop(on)",
            Self::PrevKey => "prev_key(id, property)",
            Self::NextKey => "next_key(id, property)",
            Self::Select => "select(id)",
            Self::Deselect => "deselect()",
            Self::SelectRelative => "select_relative(rows)",
            Self::Expand => "expand(id, open=true)",
            Self::ExpandAll => "expand_all(open)",
            Self::SelectKeys => "select_keys([(id, property, time), ...], add=false)",
            Self::ClearKeys => "clear_keys()",
            Self::SafeArea => "safe_area(on)",
            Self::Snap => "snap(on)",
            Self::OnlyAnimated => "only_animated(on)",
            Self::Zoom => "zoom(factor)",
            Self::ZoomFit => "zoom_fit()",
            Self::ZoomBy => "zoom_by(steps, x=None, y=None)",
            Self::Pan => "pan(dx, dy)",
            Self::PanelSize => "panel_size(\"layers\"|\"inspector\"|\"timeline\", size)",
            Self::Scroll => "scroll(\"layers\"|\"inspector\"|\"timeline\", offset)",
            Self::CopyId => "copy_id()",
            Self::Reload => "reload()",
            Self::Quit => "quit()",
            Self::Set => "set(id, property, value)",
            Self::Edit => "edit(id, property, value)",
            Self::Key => "key(id, property, time, value, ease=\"linear\")",
            Self::Unkey => "unkey(id, property, time)",
            Self::MoveKey => "move_key(id, property, from_time, to_time)",
            Self::Ease => "ease(id, property, time, name)",
            Self::SetEase => "set_ease(name)",
            Self::Stopwatch => "stopwatch(id, property)",
            Self::ToggleKey => "toggle_key(id, property)",
            Self::AddKeys => "add_keys()",
            Self::DeleteKeys => "delete_keys()",
            Self::RetimeKeys => "retime_keys(frames)",
            Self::Visible => "visible(id, on)",
            Self::Lock => "lock(id, on)",
            Self::Rename => "rename(id, name)",
            Self::AssignId => "assign_id(from, to)",
            Self::DeleteOrphan => "delete_orphan(id, property, time=None)",
            Self::EffectAdd => "effect_add(id, kind)",
            Self::EffectRemove => "effect_remove(id, kind)",
            Self::EffectMove => "effect_move(id, kind, index)",
            Self::MaskSet => "mask_set(id, source, mode=\"alpha\", invert=False)",
            Self::MaskClear => "mask_clear(id)",
            Self::Undo => "undo()",
            Self::Redo => "redo()",
            Self::Bake => "bake(id, ...)",
        }
    }

    /// Whether the command changes the overrides (and so the undo history).
    #[must_use]
    pub const fn edits(self) -> bool {
        matches!(
            self,
            Self::Set
                | Self::Edit
                | Self::Key
                | Self::Unkey
                | Self::MoveKey
                | Self::Ease
                | Self::SetEase
                | Self::Stopwatch
                | Self::ToggleKey
                | Self::AddKeys
                | Self::DeleteKeys
                | Self::RetimeKeys
                | Self::Visible
                | Self::Lock
                | Self::Rename
                | Self::AssignId
                | Self::DeleteOrphan
                | Self::EffectAdd
                | Self::EffectRemove
                | Self::EffectMove
                | Self::MaskSet
                | Self::MaskClear
        )
    }
}

impl Command {
    /// The variant of this command.
    #[must_use]
    pub const fn kind(&self) -> CommandKind {
        match self {
            Self::Play => CommandKind::Play,
            Self::Pause => CommandKind::Pause,
            Self::TogglePlay => CommandKind::TogglePlay,
            Self::Step(_) => CommandKind::Step,
            Self::Seek(_) => CommandKind::Seek,
            Self::GoToStart => CommandKind::GoToStart,
            Self::GoToEnd => CommandKind::GoToEnd,
            Self::Loop(_) => CommandKind::Loop,
            Self::PrevKey { .. } => CommandKind::PrevKey,
            Self::NextKey { .. } => CommandKind::NextKey,
            Self::Select(_) => CommandKind::Select,
            Self::Deselect => CommandKind::Deselect,
            Self::SelectRelative(_) => CommandKind::SelectRelative,
            Self::Expand { .. } => CommandKind::Expand,
            Self::ExpandAll(_) => CommandKind::ExpandAll,
            Self::SelectKeys { .. } => CommandKind::SelectKeys,
            Self::ClearKeys => CommandKind::ClearKeys,
            Self::SafeArea(_) => CommandKind::SafeArea,
            Self::Snap(_) => CommandKind::Snap,
            Self::OnlyAnimated(_) => CommandKind::OnlyAnimated,
            Self::Zoom(_) => CommandKind::Zoom,
            Self::ZoomFit => CommandKind::ZoomFit,
            Self::ZoomBy { .. } => CommandKind::ZoomBy,
            Self::Pan { .. } => CommandKind::Pan,
            Self::PanelSize { .. } => CommandKind::PanelSize,
            Self::Scroll { .. } => CommandKind::Scroll,
            Self::CopyId => CommandKind::CopyId,
            Self::Reload => CommandKind::Reload,
            Self::Quit => CommandKind::Quit,
            Self::Set { .. } => CommandKind::Set,
            Self::Edit { .. } => CommandKind::Edit,
            Self::Key { .. } => CommandKind::Key,
            Self::Unkey { .. } => CommandKind::Unkey,
            Self::MoveKey { .. } => CommandKind::MoveKey,
            Self::Ease { .. } => CommandKind::Ease,
            Self::SetEase(_) => CommandKind::SetEase,
            Self::Stopwatch { .. } => CommandKind::Stopwatch,
            Self::ToggleKey { .. } => CommandKind::ToggleKey,
            Self::AddKeys => CommandKind::AddKeys,
            Self::DeleteKeys => CommandKind::DeleteKeys,
            Self::RetimeKeys(_) => CommandKind::RetimeKeys,
            Self::Visible { .. } => CommandKind::Visible,
            Self::Lock { .. } => CommandKind::Lock,
            Self::Rename { .. } => CommandKind::Rename,
            Self::AssignId { .. } => CommandKind::AssignId,
            Self::DeleteOrphan { .. } => CommandKind::DeleteOrphan,
            Self::EffectAdd { .. } => CommandKind::EffectAdd,
            Self::EffectRemove { .. } => CommandKind::EffectRemove,
            Self::EffectMove { .. } => CommandKind::EffectMove,
            Self::MaskSet { .. } => CommandKind::MaskSet,
            Self::MaskClear { .. } => CommandKind::MaskClear,
            Self::Undo => CommandKind::Undo,
            Self::Redo => CommandKind::Redo,
            Self::Bake(_) => CommandKind::Bake,
        }
    }
}

/// What went wrong with a command, as a stable machine-readable kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The text is not valid call syntax.
    Parse,
    /// No command or request has that name.
    UnknownCommand,
    /// Wrong number or type of arguments.
    BadArgument,
    /// No document is loaded (the program has an error).
    NoDocument,
    /// No node has that identity.
    UnknownNode,
    /// The property does not exist or does not apply to the node.
    UnknownProperty,
    /// The command needs a selection (layer or keys) and there is none.
    NoSelection,
    /// No key exists at that time.
    NoKey,
    /// The value does not fit the property.
    BadValue,
    /// Reading or writing a file failed.
    Io,
    /// The command was understood but cannot run here (for example `bake`
    /// for a node created in a loop).
    Refused,
}

impl ErrorKind {
    /// The stable name used in JSON output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::UnknownCommand => "unknown_command",
            Self::BadArgument => "bad_argument",
            Self::NoDocument => "no_document",
            Self::UnknownNode => "unknown_node",
            Self::UnknownProperty => "unknown_property",
            Self::NoSelection => "no_selection",
            Self::NoKey => "no_key",
            Self::BadValue => "bad_value",
            Self::Io => "io",
            Self::Refused => "refused",
        }
    }
}

/// A typed command error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    /// Machine-readable kind.
    pub kind: ErrorKind,
    /// Human-readable explanation.
    pub message: String,
}

impl CommandError {
    /// An error of `kind`.
    #[must_use]
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.name(), self.message)
    }
}

impl std::error::Error for CommandError {}

mod examples;
mod parse;
mod text;

pub(crate) use parse::Args;
pub use parse::{parse_calls, parse_commands, Call};

#[cfg(test)]
mod tests;
