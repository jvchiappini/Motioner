//! Turning editing commands into override edits (`EditCommand`s): which
//! slot a command writes, keyed or static, which value and ease. Commands
//! resolve against the state at the start of a drag while one is open.

use motioner_core::{
    split_property, Axis, ContentField, Document, Ease, Frame, NodeId, OverrideAction, OverrideSet,
    OverrideValue, Property, Vec2,
};

use super::{channel_name, channels, Editor, Gesture, CHANNELS};
use crate::command::{Command, CommandError, ErrorKind};
use crate::edit::{self, EditCommand, KeyRef, MoveKey};

impl Editor {
    /// The state an edit resolves against: the drag start or now.
    fn base(&self) -> Result<(&Document, &OverrideSet, &[KeyRef]), CommandError> {
        match &self.gesture {
            Some(Gesture {
                document: Some(document),
                set,
                keys,
            }) => Ok((document, set, keys)),
            _ => Ok((
                self.require_document()?,
                self.session.overrides(),
                &self.selected_keys,
            )),
        }
    }

    /// Turns an editing command into override edits.
    pub(super) fn resolve_edit(&self, command: &Command) -> Result<Vec<EditCommand>, CommandError> {
        if let Some(parts) = per_axis(command) {
            // Keys of a whole vector are written per axis: the file sorts
            // `pos` before `pos.x`, so a whole-vector key next to axis keys
            // would lose on reload.
            let mut edits = Vec::new();
            let mut first_error = None;
            for part in &parts {
                match self.resolve_edit(part) {
                    Ok(mut more) => edits.append(&mut more),
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }
            return match first_error {
                Some(error) if edits.is_empty() || !matches!(error.kind, ErrorKind::NoKey) => {
                    Err(error)
                }
                _ => Ok(edits),
            };
        }
        let (document, set, keys) = self.base()?;
        let frame = self.frame();
        let edits = match command {
            Command::Set {
                id,
                property,
                value,
            } => vec![set_value(document, id, property, value, frame)?],
            Command::Edit {
                id,
                property,
                value,
            } => edit_value(document, set, id, property, value, frame)?,
            Command::Visible { .. }
            | Command::Lock { .. }
            | Command::Rename { .. }
            | Command::AssignId { .. }
            | Command::DeleteOrphan { .. } => layer_edits(command, document, set)?,
            _ => self.key_edits(command, document, keys)?,
        };
        if edits.is_empty() {
            return Err(CommandError::new(ErrorKind::Refused, "nothing to change"));
        }
        validate(document, &edits)?;
        Ok(edits)
    }

    /// Key edits: add, remove, move, ease, stopwatch, retime.
    #[allow(clippy::too_many_lines)] // one arm per key command
    fn key_edits(
        &self,
        command: &Command,
        document: &Document,
        keys: &[KeyRef],
    ) -> Result<Vec<EditCommand>, CommandError> {
        let rate = document.composition().frame_rate();
        let frame = self.frame();
        Ok(match command {
            Command::Key {
                id,
                property,
                time,
                value,
                ease,
            } => {
                let at = time.frame(rate)?;
                let (node, p, axis) = channels::resolve(document, id, property)?;
                let value = channel_value(p, axis, value)?;
                let ease = ease
                    .or_else(|| channels::key_at(document, node, p, axis, at).map(|(_, e)| e))
                    .unwrap_or(Ease::Linear);
                vec![EditCommand::SetKey {
                    id: id.clone(),
                    property: p.name().to_owned(),
                    axis,
                    frame: at,
                    value,
                    ease,
                }]
            }
            Command::Unkey { id, property, time } => {
                let at = time.frame(rate)?;
                let (node, p, axis) = channels::resolve(document, id, property)?;
                existing_key(document, node, p, axis, at, id, property)?;
                vec![EditCommand::RemoveKey {
                    id: id.clone(),
                    property: p.name().to_owned(),
                    axis,
                    frame: at,
                }]
            }
            Command::MoveKey {
                id,
                property,
                from,
                to,
            } => {
                let (from, to) = (from.frame(rate)?, to.frame(rate)?);
                let (node, p, axis) = channels::resolve(document, id, property)?;
                let (value, ease) = existing_key(document, node, p, axis, from, id, property)?;
                vec![EditCommand::MoveKeys(vec![MoveKey {
                    key: KeyRef {
                        id: id.clone(),
                        property: channel_name(p, axis),
                        frame: from,
                    },
                    to,
                    value,
                    ease,
                }])]
            }
            Command::Ease {
                id,
                property,
                time,
                ease,
            } => {
                let at = time.frame(rate)?;
                let (node, p, axis) = channels::resolve(document, id, property)?;
                let (value, _) = existing_key(document, node, p, axis, at, id, property)?;
                vec![EditCommand::MoveKeys(vec![MoveKey {
                    key: KeyRef {
                        id: id.clone(),
                        property: channel_name(p, axis),
                        frame: at,
                    },
                    to: at,
                    value,
                    ease: *ease,
                }])]
            }
            Command::SetEase(ease) => {
                vec![EditCommand::MoveKeys(moves(
                    document,
                    keys,
                    0,
                    Some(*ease),
                )?)]
            }
            Command::RetimeKeys(delta) => {
                vec![EditCommand::MoveKeys(moves(document, keys, *delta, None)?)]
            }
            Command::DeleteKeys => {
                if keys.is_empty() {
                    return Err(no_keys());
                }
                keys.iter()
                    .map(|key| {
                        let (base, axis) = split_property(&key.property);
                        EditCommand::RemoveKey {
                            id: key.id.clone(),
                            property: base.to_owned(),
                            axis,
                            frame: key.frame,
                        }
                    })
                    .collect()
            }
            Command::Stopwatch { id, property } => {
                let (node, p, axis) = channels::resolve(document, id, property)?;
                vec![EditCommand::ToggleStopwatch {
                    id: id.clone(),
                    property: p,
                    axis,
                    frame,
                    value: channels::sampled(document, node, p, axis, frame)
                        .unwrap_or(OverrideValue::Number(0.0)),
                    existing_keys: channels::keys(document, node, p, axis),
                }]
            }
            Command::ToggleKey { id, property } => {
                let (node, p, axis) = channels::resolve(document, id, property)?;
                vec![toggle_key(document, id, node, p, axis, frame)]
            }
            Command::AddKeys => {
                let node = self.require_selection()?;
                let id = self.node_identity(node);
                let animated: Vec<(Property, Option<Axis>)> = CHANNELS
                    .into_iter()
                    .filter(|(p, a)| !channels::keys(document, node, *p, *a).is_empty())
                    .collect();
                let targets = if animated.is_empty() {
                    vec![
                        (Property::Position, Some(Axis::X)),
                        (Property::Position, Some(Axis::Y)),
                    ]
                } else {
                    animated
                };
                targets
                    .into_iter()
                    .filter(|(p, a)| !channels::keys(document, node, *p, *a).contains(&frame))
                    .map(|(p, a)| EditCommand::SetKey {
                        id: id.clone(),
                        property: p.name().to_owned(),
                        axis: a,
                        frame,
                        value: channels::sampled(document, node, p, a, frame)
                            .unwrap_or(OverrideValue::Number(0.0)),
                        ease: Ease::Linear,
                    })
                    .collect()
            }
            other => return Err(not_an_edit(other)),
        })
    }
}

/// Layer edits: visibility, lock, name, identity, orphans.
fn layer_edits(
    command: &Command,
    document: &Document,
    set: &OverrideSet,
) -> Result<Vec<EditCommand>, CommandError> {
    let rate = document.composition().frame_rate();
    Ok(match command {
        Command::Visible { id, visible } => {
            channels::node_of(document, id)?;
            vec![EditCommand::SetVisible {
                id: id.clone(),
                visible: *visible,
            }]
        }
        Command::Lock { id, locked } => {
            channels::node_of(document, id)?;
            vec![EditCommand::SetLocked {
                id: id.clone(),
                locked: *locked,
            }]
        }
        Command::Rename { id, name } => {
            channels::node_of(document, id)?;
            vec![EditCommand::SetName {
                id: id.clone(),
                name: name.clone(),
            }]
        }
        Command::AssignId { from, to } => {
            if !set.entries().iter().any(|entry| &entry.id == from) {
                return Err(CommandError::new(
                    ErrorKind::UnknownNode,
                    format!("no override uses the identity {from:?}"),
                ));
            }
            vec![EditCommand::Reassign {
                from: from.clone(),
                to: to.clone(),
            }]
        }
        Command::DeleteOrphan { id, property, time } => {
            if document.find(id).is_some() {
                return Err(CommandError::new(
                    ErrorKind::Refused,
                    format!("{id:?} is a live node, not an orphan"),
                ));
            }
            let at = time.map(|t| t.frame(rate)).transpose()?;
            let entry = set
                .entries()
                .iter()
                .find(|e| &e.id == id && &e.property == property && e.frame() == at)
                .ok_or_else(|| {
                    CommandError::new(
                        ErrorKind::UnknownProperty,
                        format!("no orphaned override {id:?} {property:?}"),
                    )
                })?;
            vec![EditCommand::Remove {
                id: entry.id.clone(),
                property: entry.property.clone(),
                frame: entry.frame(),
            }]
        }
        other => return Err(not_an_edit(other)),
    })
}

fn not_an_edit(command: &Command) -> CommandError {
    CommandError::new(
        ErrorKind::Refused,
        format!("{} is not an edit", command.kind().name()),
    )
}

fn no_keys() -> CommandError {
    CommandError::new(ErrorKind::NoSelection, "no keys are selected")
}

/// A key that must exist, with its value and ease.
fn existing_key(
    document: &Document,
    node: NodeId,
    property: Property,
    axis: Option<Axis>,
    frame: Frame,
    id: &str,
    name: &str,
) -> Result<(OverrideValue, Ease), CommandError> {
    channels::key_at(document, node, property, axis, frame).ok_or_else(|| {
        CommandError::new(
            ErrorKind::NoKey,
            format!("{id} has no key on {name} at frame {frame}"),
        )
    })
}

/// Moves for the selected keys: shifted by `delta`, with a new ease or
/// their own.
fn moves(
    document: &Document,
    keys: &[KeyRef],
    delta: Frame,
    ease: Option<Ease>,
) -> Result<Vec<MoveKey>, CommandError> {
    if keys.is_empty() {
        return Err(no_keys());
    }
    keys.iter()
        .map(|key| {
            let (node, p, axis) = channels::resolve(document, &key.id, &key.property)?;
            let (value, own) =
                existing_key(document, node, p, axis, key.frame, &key.id, &key.property)?;
            Ok(MoveKey {
                key: key.clone(),
                to: key.frame + delta,
                value,
                ease: ease.unwrap_or(own),
            })
        })
        .collect()
}

/// The ◆ button: remove the key at `frame`, or add one with the sampled
/// value.
fn toggle_key(
    document: &Document,
    id: &str,
    node: NodeId,
    property: Property,
    axis: Option<Axis>,
    frame: Frame,
) -> EditCommand {
    if channels::keys(document, node, property, axis).contains(&frame) {
        EditCommand::RemoveKey {
            id: id.to_owned(),
            property: property.name().to_owned(),
            axis,
            frame,
        }
    } else {
        EditCommand::SetKey {
            id: id.to_owned(),
            property: property.name().to_owned(),
            axis,
            frame,
            value: channels::sampled(document, node, property, axis, frame)
                .unwrap_or(OverrideValue::Number(0.0)),
            ease: Ease::Linear,
        }
    }
}

/// The value a channel key takes: a number for an axis or scalar, a pair
/// (or a uniform number for scale) for a whole vector property.
fn channel_value(
    property: Property,
    axis: Option<Axis>,
    value: &OverrideValue,
) -> Result<OverrideValue, CommandError> {
    let bad = |what: &str| {
        CommandError::new(
            ErrorKind::BadValue,
            format!("{} expects {what}", channel_name(property, axis)),
        )
    };
    match (property.is_vector(), axis, value) {
        (true, Some(_), OverrideValue::Number(_)) | (false, _, OverrideValue::Number(_)) => {
            Ok(value.clone())
        }
        (true, None, OverrideValue::Vec2(_)) => Ok(value.clone()),
        (true, None, OverrideValue::Number(n)) if property == Property::Scale => {
            Ok(OverrideValue::Vec2(Vec2::new(*n, *n)))
        }
        (true, None, _) => Err(bad("an (x, y) pair")),
        _ => Err(bad("a number")),
    }
}

/// `set(...)`: a static override. An axis of a vector property writes the
/// whole pair, keeping the other axis at its value on the playhead.
fn set_value(
    document: &Document,
    id: &str,
    property: &str,
    value: &OverrideValue,
    frame: Frame,
) -> Result<EditCommand, CommandError> {
    let node = channels::node_of(document, id)?;
    if property == motioner_core::VISIBLE || property == motioner_core::LOCKED {
        return Err(CommandError::new(
            ErrorKind::BadArgument,
            format!("use {property}(id, on) for '{property}'"),
        ));
    }
    let (base, axis) = split_property(property);
    if let (Some(p), Some(axis)) = (Property::from_name(base), axis) {
        let OverrideValue::Number(number) = value else {
            return Err(CommandError::new(
                ErrorKind::BadValue,
                format!("{property} expects a number"),
            ));
        };
        let mut pair = match channels::sampled(document, node, p, None, frame) {
            Some(OverrideValue::Vec2(pair)) => pair,
            _ => Vec2::ZERO,
        };
        match axis {
            Axis::X => pair.x = *number,
            Axis::Y => pair.y = *number,
        }
        return Ok(EditCommand::SetValue {
            id: id.to_owned(),
            property: p.name().to_owned(),
            value: OverrideValue::Vec2(pair),
        });
    }
    if property == motioner_core::NAME {
        let OverrideValue::Text(name) = value else {
            return Err(CommandError::new(ErrorKind::BadValue, "a name is a string"));
        };
        return Ok(EditCommand::SetName {
            id: id.to_owned(),
            name: name.clone(),
        });
    }
    if Property::from_name(property).is_none() && ContentField::from_name(property).is_none() {
        return Err(CommandError::new(
            ErrorKind::UnknownProperty,
            format!("unknown property '{property}'"),
        ));
    }
    Ok(EditCommand::SetValue {
        id: id.to_owned(),
        property: property.to_owned(),
        value: value.clone(),
    })
}

/// `edit(...)`: what an inspector field or a canvas drag does. An animated
/// channel gets a key at the playhead (keeping that key's ease); a static
/// one gets a static override.
fn edit_value(
    document: &Document,
    set: &OverrideSet,
    id: &str,
    property: &str,
    value: &OverrideValue,
    frame: Frame,
) -> Result<Vec<EditCommand>, CommandError> {
    let (base, axis) = split_property(property);
    let Some(p) = Property::from_name(base) else {
        return Ok(vec![set_value(document, id, property, value, frame)?]);
    };
    let node = channels::node_of(document, id)?;
    if axis.is_some() && !p.is_vector() {
        return Err(CommandError::new(
            ErrorKind::UnknownProperty,
            format!("'{base}' has no axes"),
        ));
    }
    let keyed = !channels::keys(document, node, p, axis).is_empty()
        || set.entries().iter().any(|entry| {
            let (entry_base, entry_axis) = split_property(&entry.property);
            entry.id == id
                && entry_base == p.name()
                && matches!(entry.action, OverrideAction::Key { .. })
                && (axis.is_none() || entry_axis.is_none() || entry_axis == axis)
        });
    if !keyed {
        return Ok(vec![set_value(document, id, property, value, frame)?]);
    }
    let value = channel_value(p, axis, value)?;
    let parts: Vec<(Option<Axis>, OverrideValue)> = match (axis, &value) {
        // An animated vector is keyed per axis (see `per_axis`).
        (None, OverrideValue::Vec2(pair)) => vec![
            (Some(Axis::X), OverrideValue::Number(pair.x)),
            (Some(Axis::Y), OverrideValue::Number(pair.y)),
        ],
        _ => vec![(axis, value)],
    };
    Ok(parts
        .into_iter()
        .map(|(axis, value)| EditCommand::SetKey {
            id: id.to_owned(),
            property: p.name().to_owned(),
            axis,
            frame,
            value,
            ease: channels::key_at(document, node, p, axis, frame).map_or(Ease::Linear, |(_, e)| e),
        })
        .collect())
}

/// The per-axis commands a whole-vector key command stands for, or `None`
/// when the command names a scalar channel or one axis already.
fn per_axis(command: &Command) -> Option<Vec<Command>> {
    let (Command::Key { property, .. }
    | Command::Unkey { property, .. }
    | Command::MoveKey { property, .. }
    | Command::Ease { property, .. }
    | Command::Stopwatch { property, .. }
    | Command::ToggleKey { property, .. }) = command
    else {
        return None;
    };
    let (base, axis) = split_property(property);
    let vector = Property::from_name(base).filter(|p| p.is_vector() && axis.is_none())?;
    // A single number only means "both axes" for scale; elsewhere it is a
    // bad value, reported by the unsplit path.
    if let Command::Key {
        value: OverrideValue::Number(_),
        ..
    } = command
    {
        if vector != Property::Scale {
            return None;
        }
    }
    Some(
        [Axis::X, Axis::Y]
            .into_iter()
            .map(|axis| {
                let property = channel_name(vector, Some(axis));
                match command.clone() {
                    Command::Key {
                        id,
                        time,
                        value,
                        ease,
                        ..
                    } => Command::Key {
                        id,
                        property,
                        time,
                        value: match value {
                            OverrideValue::Vec2(pair) => OverrideValue::Number(match axis {
                                Axis::X => pair.x,
                                Axis::Y => pair.y,
                            }),
                            other => other,
                        },
                        ease,
                    },
                    Command::Unkey { id, time, .. } => Command::Unkey { id, property, time },
                    Command::MoveKey { id, from, to, .. } => Command::MoveKey {
                        id,
                        property,
                        from,
                        to,
                    },
                    Command::Ease { id, time, ease, .. } => Command::Ease {
                        id,
                        property,
                        time,
                        ease,
                    },
                    Command::Stopwatch { id, .. } => Command::Stopwatch { id, property },
                    Command::ToggleKey { id, .. } => Command::ToggleKey { id, property },
                    other => other,
                }
            })
            .collect(),
    )
}

/// Rejects edits whose value does not fit the node (the overrides file
/// would keep them as warnings, which is right for hand edits but wrong for
/// a command).
fn validate(document: &Document, edits: &[EditCommand]) -> Result<(), CommandError> {
    let mut probe = OverrideSet::new();
    for edit in edits {
        if matches!(
            edit,
            EditCommand::SetValue { .. } | EditCommand::SetKey { .. }
        ) {
            edit::apply(edit, &mut probe);
        }
    }
    if probe.is_empty() {
        return Ok(());
    }
    let mut trial = document.clone();
    if let Some(issue) = probe.apply(&mut trial).into_iter().next() {
        return Err(CommandError::new(
            ErrorKind::BadValue,
            format!("{} {}: {}", issue.id, issue.property, issue.message),
        ));
    }
    Ok(())
}
