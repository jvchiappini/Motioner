//! Turning editing commands into override edits (`EditCommand`s): which
//! slot a command writes, keyed or static, which value and ease. Commands
//! resolve against the state at the start of a drag while one is open.

use motioner_core::{
    split_property, Axis, CharProperty, CharValue, Color, ContentField, Document, Ease,
    EffectChannel, EffectKind, Frame, NodeId, OverrideAction, OverrideSet, OverrideValue, Property,
    TextAnimator, Vec2,
};

use super::{channel_name, channels, Channel, Editor, Gesture};
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
            | Command::DeleteOrphan { .. }
            | Command::EffectAdd { .. }
            | Command::EffectRemove { .. }
            | Command::EffectMove { .. }
            | Command::MaskSet { .. }
            | Command::MaskClear { .. }
            | Command::TextAnimAdd { .. }
            | Command::TextAnimRemove { .. } => layer_edits(command, document, set)?,
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
                let (node, channel) = channels::resolve(document, id, property)?;
                let value = channel_value(channel, value)?;
                let ease = ease
                    .or_else(|| channels::key_at(document, node, channel, at).map(|(_, e)| e))
                    .unwrap_or(Ease::Linear);
                vec![EditCommand::SetKey {
                    id: id.clone(),
                    property: channel.base().clone(),
                    axis: channel.axis(),
                    frame: at,
                    value,
                    ease,
                }]
            }
            Command::Unkey { id, property, time } => {
                let at = time.frame(rate)?;
                let (node, channel) = channels::resolve(document, id, property)?;
                existing_key(document, node, channel, at, id, property)?;
                vec![EditCommand::RemoveKey {
                    id: id.clone(),
                    property: channel.base().clone(),
                    axis: channel.axis(),
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
                let (node, channel) = channels::resolve(document, id, property)?;
                let (value, ease) = existing_key(document, node, channel, from, id, property)?;
                vec![EditCommand::MoveKeys(vec![MoveKey {
                    key: KeyRef {
                        id: id.clone(),
                        property: channel.name(),
                        frame: from,
                    },
                    to,
                    value,
                    ease,
                }])]
            }
            Command::MoveKeyValue {
                id,
                property,
                from,
                to,
                value,
            } => {
                let (from, to) = (from.frame(rate)?, to.frame(rate)?);
                let (node, channel) = channels::resolve(document, id, property)?;
                let (_, ease) = existing_key(document, node, channel, from, id, property)?;
                let value = channel_value(channel, value)?;
                vec![EditCommand::MoveKeys(vec![MoveKey {
                    key: KeyRef {
                        id: id.clone(),
                        property: channel.name(),
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
                let (node, channel) = channels::resolve(document, id, property)?;
                let (value, _) = existing_key(document, node, channel, at, id, property)?;
                vec![EditCommand::MoveKeys(vec![MoveKey {
                    key: KeyRef {
                        id: id.clone(),
                        property: channel.name(),
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
                let (node, channel) = channels::resolve(document, id, property)?;
                vec![EditCommand::ToggleStopwatch {
                    id: id.clone(),
                    property: channel.base().clone(),
                    axis: channel.axis(),
                    frame,
                    value: channels::sampled(document, node, channel, frame)
                        .unwrap_or(OverrideValue::Number(0.0)),
                    existing_keys: channels::keys(document, node, channel),
                }]
            }
            Command::ToggleKey { id, property } => {
                let (node, channel) = channels::resolve(document, id, property)?;
                vec![toggle_key(document, id, node, channel, frame)]
            }
            Command::AddKeys => {
                let node = self.require_selection()?;
                let id = self.node_identity(node);
                let animated: Vec<Channel> = channels::node_channels(document, node)
                    .into_iter()
                    .filter(|channel| !channels::keys(document, node, *channel).is_empty())
                    .collect();
                let targets = if animated.is_empty() {
                    vec![
                        Channel::Transform(Property::Position, Some(Axis::X)),
                        Channel::Transform(Property::Position, Some(Axis::Y)),
                    ]
                } else {
                    animated
                };
                targets
                    .into_iter()
                    .filter(|channel| !channels::keys(document, node, *channel).contains(&frame))
                    .map(|channel| EditCommand::SetKey {
                        id: id.clone(),
                        property: channel.base().clone(),
                        axis: channel.axis(),
                        frame,
                        value: channels::sampled(document, node, channel, frame)
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
    if matches!(
        command,
        Command::EffectAdd { .. } | Command::EffectRemove { .. } | Command::EffectMove { .. }
    ) {
        return effect_edits(command, document);
    }
    if matches!(command, Command::MaskSet { .. } | Command::MaskClear { .. }) {
        return mask_edits(command, document);
    }
    if matches!(
        command,
        Command::TextAnimAdd { .. } | Command::TextAnimRemove { .. }
    ) {
        return text_animator_edits(command, document);
    }
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
    channel: Channel,
    frame: Frame,
    id: &str,
    name: &str,
) -> Result<(OverrideValue, Ease), CommandError> {
    channels::key_at(document, node, channel, frame).ok_or_else(|| {
        CommandError::new(
            ErrorKind::NoKey,
            format!("{id} has no key on {name} at frame {frame}"),
        )
    })
}

/// Effect stack commands: add, remove and reorder rewrite the `effects`
/// override; parameter values go through the generic `set`/`edit`/`key`.
fn effect_edits(command: &Command, document: &Document) -> Result<Vec<EditCommand>, CommandError> {
    let (Command::EffectAdd { id, kind }
    | Command::EffectRemove { id, kind }
    | Command::EffectMove { id, kind, .. }) = command
    else {
        return Err(CommandError::new(
            ErrorKind::Refused,
            "not an effect command",
        ));
    };
    let node = channels::node_of(document, id)?;
    let stack: Vec<EffectKind> = document
        .node(node)
        .map(|node| {
            node.effects()
                .iter()
                .map(motioner_core::Effect::kind)
                .collect()
        })
        .unwrap_or_default();
    match command {
        Command::EffectAdd { .. } => {
            if stack.contains(kind) {
                return Err(CommandError::new(
                    ErrorKind::BadValue,
                    format!("{id} already has a '{}' effect", kind.name()),
                ));
            }
            let mut kinds = stack;
            kinds.push(*kind);
            Ok(vec![effect_stack_edit(id, &kinds)])
        }
        Command::EffectRemove { .. } => {
            if !stack.contains(kind) {
                return Err(CommandError::new(
                    ErrorKind::BadValue,
                    format!("{id} has no '{}' effect", kind.name()),
                ));
            }
            let kinds = stack
                .into_iter()
                .filter(|candidate| candidate != kind)
                .collect::<Vec<_>>();
            Ok(vec![effect_stack_edit(id, &kinds)])
        }
        Command::EffectMove { index, .. } => {
            let Some(from) = stack.iter().position(|candidate| candidate == kind) else {
                return Err(CommandError::new(
                    ErrorKind::BadValue,
                    format!("{id} has no '{}' effect", kind.name()),
                ));
            };
            if *index >= stack.len() {
                return Err(CommandError::new(
                    ErrorKind::BadArgument,
                    format!(
                        "effect index {index} is out of range: {id} has {} effect(s)",
                        stack.len()
                    ),
                ));
            }
            let mut kinds = stack;
            let moved = kinds.remove(from);
            kinds.insert(*index, moved);
            Ok(vec![effect_stack_edit(id, &kinds)])
        }
        _ => Ok(Vec::new()),
    }
}

/// Mask commands rewrite the `mask` override (D-0033).
///
/// The value is `"<source>:<mode>:<invert>"`; the source is written by identity
/// so it survives a reload and shows up in `query`.
fn mask_edits(command: &Command, document: &Document) -> Result<Vec<EditCommand>, CommandError> {
    match command {
        Command::MaskSet {
            id,
            source,
            mode,
            invert,
        } => {
            let node = channels::node_of(document, id)?;
            let source_node = document.find(source).ok_or_else(|| {
                CommandError::new(
                    ErrorKind::UnknownNode,
                    format!("no node has the identity {source:?} (see `query` for the ids)"),
                )
            })?;
            // Validate on a copy so the typed core error (self, ancestor,
            // cycle) reaches the caller without touching the session.
            document
                .clone()
                .set_mask(node, source_node, *mode, *invert)
                .map_err(|error| CommandError::new(ErrorKind::BadValue, error.to_string()))?;
            let spec = motioner_core::MaskSpec {
                source: source_node,
                mode: *mode,
                invert: *invert,
            };
            Ok(vec![EditCommand::SetValue {
                id: id.clone(),
                property: motioner_core::MASK.to_owned(),
                value: OverrideValue::Text(spec.override_text(source)),
            }])
        }
        Command::MaskClear { id } => {
            let node = channels::node_of(document, id)?;
            if document
                .node(node)
                .and_then(motioner_core::Node::mask)
                .is_none()
            {
                return Err(CommandError::new(
                    ErrorKind::BadValue,
                    format!("{id} has no mask to clear"),
                ));
            }
            Ok(vec![EditCommand::Remove {
                id: id.clone(),
                property: motioner_core::MASK.to_owned(),
                frame: None,
            }])
        }
        _ => Ok(Vec::new()),
    }
}

/// Add or remove a per-character animator (D-0034).
///
/// The whole animator list travels as one `text_animators` override, so the
/// editor and the overrides file always agree on the order.
fn text_animator_edits(
    command: &Command,
    document: &Document,
) -> Result<Vec<EditCommand>, CommandError> {
    let (id, current) = match command {
        Command::TextAnimAdd { id, .. } | Command::TextAnimRemove { id, .. } => {
            let node = channels::node_of(document, id)?;
            let Some(animators) = document.text_animators(node) else {
                return Err(CommandError::new(
                    ErrorKind::BadValue,
                    format!("{id} is not a text layer"),
                ));
            };
            (id, animators.to_vec())
        }
        _ => return Ok(Vec::new()),
    };
    let mut animators = current;
    match command {
        Command::TextAnimAdd {
            property,
            from,
            to,
            start,
            stagger,
            duration,
            ease,
            order,
            ..
        } => {
            let rate = document.composition().frame_rate();
            let animator = TextAnimator::new(
                *property,
                animator_value(*property, from)?,
                animator_value(*property, to)?,
                start.frame(rate)?,
                *stagger,
                *duration,
                *ease,
                *order,
            )
            .map_err(|error| CommandError::new(ErrorKind::BadValue, error.to_string()))?;
            animators.push(animator);
        }
        Command::TextAnimRemove { index, .. } => {
            if *index >= animators.len() {
                return Err(CommandError::new(
                    ErrorKind::BadValue,
                    format!(
                        "{id} has {} animator(s); there is no animator at index {index}",
                        animators.len()
                    ),
                ));
            }
            animators.remove(*index);
        }
        _ => {}
    }
    Ok(vec![EditCommand::SetValue {
        id: id.clone(),
        property: TextAnimator::OVERRIDE_PROPERTY.to_owned(),
        value: OverrideValue::Text(TextAnimator::encode_list(&animators)),
    }])
}

/// Converts a command value to the animator endpoint kind of `property`.
fn animator_value(
    property: CharProperty,
    value: &OverrideValue,
) -> Result<CharValue, CommandError> {
    match (property, value) {
        (
            CharProperty::Opacity | CharProperty::Scale | CharProperty::Rotation,
            OverrideValue::Number(number),
        ) => Ok(CharValue::Number(*number)),
        (CharProperty::Offset, OverrideValue::Vec2(vector)) => Ok(CharValue::Vec2(*vector)),
        (CharProperty::Color, OverrideValue::Text(text)) => {
            Color::parse_hex(text).map(CharValue::Color).map_err(|_| {
                CommandError::new(
                    ErrorKind::BadValue,
                    format!("'{}' expects a colour like #RRGGBB", property.name()),
                )
            })
        }
        (
            CharProperty::Opacity | CharProperty::Scale | CharProperty::Rotation,
            OverrideValue::Vec2(_) | OverrideValue::Text(_),
        ) => Err(CommandError::new(
            ErrorKind::BadValue,
            format!("'{}' expects a number", property.name()),
        )),
        (CharProperty::Offset, _) => Err(CommandError::new(
            ErrorKind::BadValue,
            "'offset' expects an (x, y) pair",
        )),
        (CharProperty::Color, _) => Err(CommandError::new(
            ErrorKind::BadValue,
            "'color' expects a colour like #RRGGBB",
        )),
    }
}

/// The override that rewrites a layer's effect stack.
fn effect_stack_edit(id: &str, kinds: &[EffectKind]) -> EditCommand {
    let list = kinds
        .iter()
        .map(|kind| kind.name())
        .collect::<Vec<_>>()
        .join(",");
    EditCommand::SetValue {
        id: id.to_owned(),
        property: motioner_core::EFFECTS.to_owned(),
        value: OverrideValue::Text(list),
    }
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
            let (node, channel) = channels::resolve(document, &key.id, &key.property)?;
            let (value, own) =
                existing_key(document, node, channel, key.frame, &key.id, &key.property)?;
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
    channel: Channel,
    frame: Frame,
) -> EditCommand {
    if channels::keys(document, node, channel).contains(&frame) {
        EditCommand::RemoveKey {
            id: id.to_owned(),
            property: channel.base().clone(),
            axis: channel.axis(),
            frame,
        }
    } else {
        EditCommand::SetKey {
            id: id.to_owned(),
            property: channel.base().clone(),
            axis: channel.axis(),
            frame,
            value: channels::sampled(document, node, channel, frame)
                .unwrap_or(OverrideValue::Number(0.0)),
            ease: Ease::Linear,
        }
    }
}

/// The value a channel key takes: a number for an axis or scalar, a pair
/// (or a uniform number for scale) for a whole vector property, and the value
/// itself for a content field (validated against the node when it is applied).
fn channel_value(channel: Channel, value: &OverrideValue) -> Result<OverrideValue, CommandError> {
    let Channel::Transform(property, axis) = channel else {
        return Ok(value.clone());
    };
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
        let mut pair = match channels::sampled(document, node, Channel::Transform(p, None), frame) {
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
    if property != "blend"
        && Property::from_name(property).is_none()
        && ContentField::from_name(property).is_none()
        && EffectChannel::from_name(property).is_none()
        && property != motioner_core::EFFECTS
    {
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
    if property == motioner_core::NAME {
        return Ok(vec![set_value(document, id, property, value, frame)?]);
    }
    let Some((node, channel)) = resolve_if_animatable(document, id, property)? else {
        return Ok(vec![set_value(document, id, property, value, frame)?]);
    };
    let keyed = !channels::keys(document, node, channel).is_empty()
        || set.entries().iter().any(|entry| {
            let (entry_base, entry_axis) = split_property(&entry.property);
            entry.id == id
                && entry_base == channel.base()
                && matches!(entry.action, OverrideAction::Key { .. })
                && (channel.axis().is_none()
                    || entry_axis.is_none()
                    || entry_axis == channel.axis())
        });
    if !keyed {
        return Ok(vec![set_value(document, id, property, value, frame)?]);
    }
    let value = channel_value(channel, value)?;
    let parts: Vec<(Channel, OverrideValue)> = match (channel, &value) {
        // An animated vector is keyed per axis (see `per_axis`).
        (Channel::Transform(property, None), OverrideValue::Vec2(pair)) => vec![
            (
                Channel::Transform(property, Some(Axis::X)),
                OverrideValue::Number(pair.x),
            ),
            (
                Channel::Transform(property, Some(Axis::Y)),
                OverrideValue::Number(pair.y),
            ),
        ],
        _ => vec![(channel, value)],
    };
    Ok(parts
        .into_iter()
        .map(|(channel, value)| EditCommand::SetKey {
            id: id.to_owned(),
            property: channel.base().clone(),
            axis: channel.axis(),
            frame,
            value,
            ease: channels::key_at(document, node, channel, frame).map_or(Ease::Linear, |(_, e)| e),
        })
        .collect())
}

/// Resolves `property` as an animatable channel of `id`, without failing when
/// the name is a plain (non-animatable) content field such as `text`.
fn resolve_if_animatable(
    document: &Document,
    id: &str,
    property: &str,
) -> Result<Option<(NodeId, Channel)>, CommandError> {
    // Blend modes are discrete node properties; editing one always writes a
    // static override, never a key (D-0031).
    if property == "blend" {
        return Ok(None);
    }
    match channels::resolve(document, id, property) {
        Ok((node, channel)) => Ok(Some((node, channel))),
        Err(error)
            if error.kind == ErrorKind::UnknownProperty
                && ContentField::from_name(property)
                    .is_some_and(|field| !field.is_animatable()) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
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
