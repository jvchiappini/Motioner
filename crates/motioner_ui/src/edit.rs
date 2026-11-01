//! Pure edit commands on an [`OverrideSet`], and the undo stack.
//!
//! The editor never mutates the program: every gesture becomes one
//! [`EditCommand`], which produces one override entry (or a small group of
//! them) in the set. [`UndoStack`] snapshots the set before a gesture, so undo
//! and redo are exact and a continuous drag coalesces into a single step.

use std::collections::VecDeque;

use motioner_core::{Axis, Ease, Frame, OverrideAction, OverrideEntry, OverrideSet, OverrideValue};

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

/// Applies `command` to `set`.
pub fn apply(command: &EditCommand, set: &mut OverrideSet) {
    match command {
        EditCommand::SetValue {
            id,
            property,
            value,
        } => set.insert(OverrideEntry::new(
            id.clone(),
            property.clone(),
            OverrideAction::Set(value.clone()),
        )),
        EditCommand::SetKey {
            id,
            property,
            axis,
            frame,
            value,
            ease,
        } => set.insert(OverrideEntry::new(
            id.clone(),
            property_name(property, *axis),
            OverrideAction::Key {
                frame: *frame,
                value: value.clone(),
                ease: *ease,
            },
        )),
        EditCommand::RemoveKey {
            id,
            property,
            axis,
            frame,
        } => set.insert(OverrideEntry::new(
            id.clone(),
            property_name(property, *axis),
            OverrideAction::Unkey { frame: *frame },
        )),
        EditCommand::MoveKeys(moves) => apply_moves(set, moves),
        EditCommand::ToggleStopwatch {
            id,
            property,
            axis,
            frame,
            value,
            existing_keys,
        } => apply_stopwatch(set, id, property, *axis, *frame, value, existing_keys),
        EditCommand::SetVisible { id, visible } => set.insert(OverrideEntry::new(
            id.clone(),
            motioner_core::VISIBLE,
            OverrideAction::Visible(*visible),
        )),
        EditCommand::SetLocked { id, locked } => set.insert(OverrideEntry::new(
            id.clone(),
            motioner_core::LOCKED,
            OverrideAction::Locked(*locked),
        )),
        EditCommand::SetName { id, name } => apply_name(set, id, name),
        EditCommand::Reassign { from, to } => apply_reassign(set, from, to),
        EditCommand::Remove {
            id,
            property,
            frame,
        } => set.remove_slot(id, property, *frame),
    }
}

fn apply_moves(set: &mut OverrideSet, moves: &[MoveKey]) {
    for entry in moves {
        let property = entry.key.property.clone();
        let id = entry.key.id.clone();
        set.remove_slot(&id, &property, Some(entry.key.frame));
        set.insert(OverrideEntry::new(
            id.clone(),
            property.clone(),
            OverrideAction::Unkey {
                frame: entry.key.frame,
            },
        ));
        set.insert(OverrideEntry::new(
            id,
            property,
            OverrideAction::Key {
                frame: entry.to,
                value: entry.value.clone(),
                ease: entry.ease,
            },
        ));
    }
}

/// The override property name for a channel: `pos` or `pos.x`.
fn property_name(property: &str, axis: Option<Axis>) -> String {
    match axis {
        Some(axis) => format!("{property}.{}", axis.name()),
        None => property.to_owned(),
    }
}

fn apply_stopwatch(
    set: &mut OverrideSet,
    id: &str,
    property: &str,
    axis: Option<Axis>,
    frame: Frame,
    value: &OverrideValue,
    existing_keys: &[Frame],
) {
    let name = property_name(property, axis);
    if existing_keys.is_empty() {
        set.insert(OverrideEntry::new(
            id.to_owned(),
            name.clone(),
            OverrideAction::Key {
                frame,
                value: value.clone(),
                ease: Ease::Linear,
            },
        ));
        return;
    }
    for key in existing_keys {
        set.insert(OverrideEntry::new(
            id.to_owned(),
            name.clone(),
            OverrideAction::Unkey { frame: *key },
        ));
    }
    set.insert(OverrideEntry::new(
        id.to_owned(),
        name,
        OverrideAction::Set(value.clone()),
    ));
}

fn apply_name(set: &mut OverrideSet, id: &str, name: &str) {
    if name.is_empty() {
        set.remove_slot(id, motioner_core::NAME, None);
    } else {
        set.insert(OverrideEntry::new(
            id.to_owned(),
            motioner_core::NAME,
            OverrideAction::Set(OverrideValue::Text(name.to_owned())),
        ));
    }
}

fn apply_reassign(set: &mut OverrideSet, from: &str, to: &str) {
    for entry in set.entries().to_vec() {
        if entry.id == from {
            set.insert(OverrideEntry::new(
                to.to_owned(),
                entry.property.clone(),
                entry.action.clone(),
            ));
        }
    }
    set.remove_id(from);
}
/// A bounded undo/redo history of override sets.
///
/// A gesture is the unit of undo: [`Self::begin_gesture`] opens one,
/// [`Self::record`] stores the state before the *first* change of the gesture
/// and ignores the rest, and [`Self::end_gesture`] closes it. Commands outside
/// a gesture each record their own step.
#[derive(Debug)]
pub struct UndoStack {
    undo: VecDeque<OverrideSet>,
    redo: Vec<OverrideSet>,
    limit: usize,
    gesture_open: bool,
    gesture_recorded: bool,
}

impl UndoStack {
    /// A stack that remembers at most `limit` steps.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            gesture_open: false,
            gesture_recorded: false,
        }
    }

    /// A stack with the editor's default of 200 steps.
    #[must_use]
    pub fn default_limit() -> Self {
        Self::new(200)
    }

    /// Number of undo steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// Whether there is nothing to undo.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// Whether there is anything to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is anything to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Number of redo steps.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Opens a gesture (a drag). Records after this call coalesce.
    pub fn begin_gesture(&mut self) {
        self.gesture_open = true;
        self.gesture_recorded = false;
    }

    /// Closes the gesture.
    pub fn end_gesture(&mut self) {
        self.gesture_open = false;
        self.gesture_recorded = false;
    }

    /// Stores `before` as an undo step.
    ///
    /// Inside a gesture, only the first call stores; later ones are no-ops.
    pub fn record(&mut self, before: OverrideSet) {
        if self.gesture_open && self.gesture_recorded {
            return;
        }
        if self.gesture_open {
            self.gesture_recorded = true;
        }
        self.undo.push_back(before);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
        self.redo.clear();
    }

    /// Moves the current state onto the redo stack and returns the state to
    /// restore, if any.
    pub fn undo(&mut self, current: &OverrideSet) -> Option<OverrideSet> {
        let previous = self.undo.pop_back()?;
        self.redo.push(current.clone());
        Some(previous)
    }

    /// Returns the next redo state, if any.
    pub fn redo(&mut self, current: &OverrideSet) -> Option<OverrideSet> {
        let next = self.redo.pop()?;
        self.undo.push_back(current.clone());
        Some(next)
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::default_limit()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::Vec2;

    fn set_with(value: f64) -> OverrideSet {
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "a",
            "opacity",
            OverrideAction::Set(OverrideValue::Number(value)),
        ));
        set
    }

    #[test]
    fn every_command_writes_the_expected_entry() {
        let mut set = OverrideSet::new();
        apply(
            &EditCommand::SetValue {
                id: "a".into(),
                property: "pos".into(),
                value: OverrideValue::Vec2(Vec2::new(1.0, 2.0)),
            },
            &mut set,
        );
        assert_eq!(
            set.entries()[0].action,
            OverrideAction::Set(OverrideValue::Vec2(Vec2::new(1.0, 2.0)))
        );
        apply(
            &EditCommand::SetKey {
                id: "a".into(),
                property: "opacity".into(),
                axis: None,
                frame: 10,
                value: OverrideValue::Number(0.5),
                ease: Ease::Linear,
            },
            &mut set,
        );
        assert_eq!(
            set.get("a", "opacity", Some(10)).unwrap().action,
            OverrideAction::Key {
                frame: 10,
                value: OverrideValue::Number(0.5),
                ease: Ease::Linear,
            }
        );
        apply(
            &EditCommand::RemoveKey {
                id: "a".into(),
                property: "opacity".into(),
                axis: None,
                frame: 10,
            },
            &mut set,
        );
        assert_eq!(
            set.get("a", "opacity", Some(10)).unwrap().action,
            OverrideAction::Unkey { frame: 10 }
        );
        apply(
            &EditCommand::SetVisible {
                id: "a".into(),
                visible: false,
            },
            &mut set,
        );
        assert_eq!(
            set.get("a", "visible", None).unwrap().action,
            OverrideAction::Visible(false)
        );
        apply(
            &EditCommand::SetLocked {
                id: "a".into(),
                locked: true,
            },
            &mut set,
        );
        assert_eq!(
            set.get("a", "locked", None).unwrap().action,
            OverrideAction::Locked(true)
        );
        apply(
            &EditCommand::SetName {
                id: "a".into(),
                name: "Title".into(),
            },
            &mut set,
        );
        assert_eq!(
            set.get("a", "name", None).unwrap().action,
            OverrideAction::Set(OverrideValue::Text("Title".into()))
        );
        apply(
            &EditCommand::SetName {
                id: "a".into(),
                name: String::new(),
            },
            &mut set,
        );
        assert!(set.get("a", "name", None).is_none());
    }

    #[test]
    fn stopwatch_adds_a_key_or_removes_every_key() {
        let mut set = OverrideSet::new();
        apply(
            &EditCommand::ToggleStopwatch {
                id: "a".into(),
                property: "rotation".into(),
                axis: None,
                frame: 5,
                value: OverrideValue::Number(9.0),
                existing_keys: Vec::new(),
            },
            &mut set,
        );
        assert!(matches!(
            set.get("a", "rotation", Some(5)).unwrap().action,
            OverrideAction::Key { .. }
        ));
        apply(
            &EditCommand::ToggleStopwatch {
                id: "a".into(),
                property: "rotation".into(),
                axis: None,
                frame: 5,
                value: OverrideValue::Number(9.0),
                existing_keys: vec![0, 30],
            },
            &mut set,
        );
        assert!(matches!(
            set.get("a", "rotation", Some(0)).unwrap().action,
            OverrideAction::Unkey { frame: 0 }
        ));
        assert!(matches!(
            set.get("a", "rotation", Some(30)).unwrap().action,
            OverrideAction::Unkey { frame: 30 }
        ));
        assert!(matches!(
            set.get("a", "rotation", None).unwrap().action,
            OverrideAction::Set(OverrideValue::Number(9.0))
        ));
    }

    #[test]
    fn moving_keys_unkeys_the_old_frame_and_keys_the_new_one() {
        let mut set = OverrideSet::new();
        apply(
            &EditCommand::MoveKeys(vec![MoveKey {
                key: KeyRef {
                    id: "a".into(),
                    property: "opacity".into(),
                    frame: 10,
                },
                to: 14,
                value: OverrideValue::Number(0.5),
                ease: Ease::QuadIn,
            }]),
            &mut set,
        );
        assert!(matches!(
            set.get("a", "opacity", Some(10)).unwrap().action,
            OverrideAction::Unkey { frame: 10 }
        ));
        assert!(matches!(
            set.get("a", "opacity", Some(14)).unwrap().action,
            OverrideAction::Key { frame: 14, .. }
        ));
    }

    #[test]
    fn reassign_moves_every_entry_and_drops_the_old_id() {
        let mut set = set_with(0.5);
        set.insert(OverrideEntry::new(
            "a",
            "rotation",
            OverrideAction::Set(OverrideValue::Number(3.0)),
        ));
        set.insert(OverrideEntry::new(
            "b",
            "opacity",
            OverrideAction::Set(OverrideValue::Number(0.9)),
        ));
        apply(
            &EditCommand::Reassign {
                from: "a".into(),
                to: "@1".into(),
            },
            &mut set,
        );
        assert!(set.get("a", "opacity", None).is_none());
        assert!(set.get("@1", "opacity", None).is_some());
        assert!(set.get("@1", "rotation", None).is_some());
        assert!(set.get("b", "opacity", None).is_some());
    }

    #[test]
    fn undo_and_redo_are_symmetric() {
        let mut stack = UndoStack::default_limit();
        let state0 = OverrideSet::new();
        let state1 = set_with(0.5);
        let state2 = set_with(0.9);
        stack.record(state0.clone());
        stack.record(state1.clone());
        let back = stack.undo(&state2).unwrap();
        assert_eq!(back, state1);
        let back = stack.undo(&back).unwrap();
        assert_eq!(back, state0);
        assert!(!stack.can_undo());
        let forward = stack.redo(&state0).unwrap();
        assert_eq!(forward, state1);
        let forward = stack.redo(&forward).unwrap();
        assert_eq!(forward, state2);
        assert!(!stack.can_redo());
    }

    #[test]
    fn a_gesture_is_one_undo_step() {
        let mut stack = UndoStack::default_limit();
        let before = OverrideSet::new();
        stack.begin_gesture();
        for step in 0..10 {
            stack.record(before.clone());
            let _ = step;
        }
        stack.end_gesture();
        assert_eq!(stack.len(), 1);
        let current = set_with(1.0);
        assert_eq!(stack.undo(&current), Some(before));
        assert!(!stack.can_undo());
    }

    #[test]
    fn a_new_edit_clears_redo_and_the_stack_is_bounded() {
        let mut stack = UndoStack::new(2);
        stack.record(OverrideSet::new());
        let current = set_with(0.1);
        assert!(stack.undo(&current).is_some());
        assert!(stack.can_redo());
        stack.record(OverrideSet::new());
        assert!(!stack.can_redo());
        for _ in 0..10 {
            stack.record(OverrideSet::new());
        }
        assert_eq!(stack.len(), 2, "bounded");
    }
}
