//! The resolved edit vocabulary shared by the editor and the code planner.
//!
//! A UI command resolves into one or more [`EditCommand`] values. The editor
//! applies them to its override set; the write-through planner (D-0036) reads
//! the same values to decide which code edit reproduces the change. The type
//! lives in the core so both layers speak one vocabulary without a dependency
//! cycle.

use crate::animation::{Axis, Ease};
use crate::overrides::OverrideValue;
use crate::time::Frame;

/// One keyframe of one channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRef {
    /// Node identity.
    pub id: String,
    /// Property name (`pos`, `opacity`, ...).
    pub property: String,
    /// Composition frame.
    pub frame: Frame,
}

/// What an edit does.
#[derive(Debug, Clone, PartialEq)]
pub enum EditCommand {
    /// Sets the static value of a property or content field.
    SetValue {
        /// Node identity.
        id: String,
        /// Property name.
        property: String,
        /// New value.
        value: OverrideValue,
    },
    /// Adds or replaces the key at `frame`.
    SetKey {
        /// Node identity.
        id: String,
        /// Property name (a vector axis is written `pos.x`).
        property: String,
        /// Axis for vector properties.
        axis: Option<Axis>,
        /// Composition frame.
        frame: Frame,
        /// Value at that frame.
        value: OverrideValue,
        /// Ease of the segment arriving at the key (the editor keeps the
        /// ease of a key it replaces).
        ease: Ease,
    },
    /// Removes the key at `frame`.
    RemoveKey {
        /// Node identity.
        id: String,
        /// Property name (a vector axis is written `pos.x`).
        property: String,
        /// Axis for vector properties.
        axis: Option<Axis>,
        /// Composition frame.
        frame: Frame,
    },
    /// Moves keys to new frames (retiming).
    MoveKeys(Vec<MoveKey>),
    /// Adds the first key at the playhead, or removes every key and keeps the
    /// value at the playhead as a static override.
    ToggleStopwatch {
        /// Node identity.
        id: String,
        /// Channel base name (`pos`, `opacity`, `fill`, `size`, ...).
        property: String,
        /// Axis for vector properties.
        axis: Option<Axis>,
        /// Playhead.
        frame: Frame,
        /// Value at the playhead.
        value: OverrideValue,
        /// Frames that currently have a key on the channel.
        existing_keys: Vec<Frame>,
    },
    /// Shows or hides the node.
    SetVisible {
        /// Node identity.
        id: String,
        /// New visibility.
        visible: bool,
    },
    /// Locks or unlocks the node for editing.
    SetLocked {
        /// Node identity.
        id: String,
        /// New lock state.
        locked: bool,
    },
    /// Sets the display name shown in the layers panel.
    SetName {
        /// Node identity.
        id: String,
        /// New display name (empty removes the override).
        name: String,
    },
    /// Rewrites every override of `from` to `to` (the inspector's "Assign ID"
    /// action; the program itself is edited by the user).
    Reassign {
        /// Current identity.
        from: String,
        /// New identity.
        to: String,
    },
    /// Removes the entry in one slot (deleting an orphaned override).
    Remove {
        /// Identity.
        id: String,
        /// Property, as written in the file.
        property: String,
        /// Frame of a key or unkey entry; `None` for static entries.
        frame: Option<Frame>,
    },
}

/// One key moving to another frame.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveKey {
    /// The key being moved.
    pub key: KeyRef,
    /// New frame.
    pub to: Frame,
    /// Value of the key (kept as it moves).
    pub value: OverrideValue,
    /// Ease of its incoming segment.
    pub ease: Ease,
}
