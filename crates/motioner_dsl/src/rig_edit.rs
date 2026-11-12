//! Rig write-through: source plans and manifest expectations (prompt 16b E2).
//!
//! A rig is authored as ordinary MDF calls, so an edit is a byte-range edit on
//! those calls. The planner finds the calls through the block that binds the
//! rig root (recorded by the interpreter), replaces or inserts the smallest
//! call, and returns the same [`Plan`] shape as the property planner. The
//! editor then compiles the result and compares the rig manifest with
//! [`apply_to_manifest`]; a mismatch is a refusal, never a guess.
//!
//! Declaration calls that live inside a used `.motion` module are refused with
//! a reason (D-0057 addendum to D-0036): their spans belong to the module, not
//! to the open program, and module write-through is a separate feature.

#![allow(clippy::assigning_clones)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use motioner_core::{
    format_number, quote, Ease, NodeId, RigInstanceManifest, RigManifest, RigMirrorSide,
    RigPropertyKeyManifest, RigPropertyValue, RigValue, RigValueKind, Vec2,
};

use crate::ast::{Argument, Expr, ExprKind, Program, Stmt, StmtKind, Target};
use crate::code_edit::{CodeEdit, Replacement, Strategy};
use crate::diagnostic::Span;
use crate::origins::ValueOrigins;
use crate::plan::Plan;
use crate::write_text::pair_text;

/// Which side of a mirrored property an edit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RigSide {
    /// The mirror group (both sides); a concrete property edits that property.
    #[default]
    Both,
    /// The first side of the pair.
    Left,
    /// The second side of the pair.
    Right,
}

/// How a transition condition compares its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigCompare {
    /// `equals=`
    Equals,
    /// `not_equals=`
    NotEquals,
    /// `gt=`
    Greater,
    /// `lt=`
    Less,
    /// `trigger=`
    Trigger,
}

impl RigCompare {
    fn arg_name(self) -> &'static str {
        match self {
            Self::Equals => "equals",
            Self::NotEquals => "not_equals",
            Self::Greater => "gt",
            Self::Less => "lt",
            Self::Trigger => "trigger",
        }
    }
}

/// One rig edit. Times are seconds, as in MDF; frames are derived with the
/// composition rate for the manifest.
#[derive(Debug, Clone, PartialEq)]
pub enum RigEdit {
    /// Set a property's static value (clears keys after frame 0).
    Set {
        /// Rig display name or root identity.
        rig: String,
        /// Property or mirror group.
        property: String,
        /// Mirror side; `Both` is the default.
        side: RigSide,
        /// New value.
        value: RigValue,
    },
    /// Add or replace a property key.
    Key {
        /// Rig display name or root identity.
        rig: String,
        /// Property or mirror group.
        property: String,
        /// Mirror side; `Both` is the default.
        side: RigSide,
        /// Composition time in seconds.
        time: f64,
        /// Value.
        value: RigValue,
        /// Ease of the arriving segment.
        ease: Ease,
    },
    /// Remove a property key.
    Unkey {
        /// Rig display name or root identity.
        rig: String,
        /// Property or mirror group.
        property: String,
        /// Mirror side; `Both` is the default.
        side: RigSide,
        /// Composition time in seconds.
        time: f64,
    },
    /// Apply a declared pose at a time.
    ApplyPose {
        /// Rig display name or root identity.
        rig: String,
        /// Pose name.
        pose: String,
        /// Composition time in seconds.
        at: f64,
        /// Blend duration in seconds.
        duration: f64,
        /// Blend ease.
        ease: Ease,
    },
    /// Declare or replace a pose.
    SavePose {
        /// Rig display name or root identity.
        rig: String,
        /// Pose name.
        pose: String,
        /// Whether omitted properties reset to their defaults.
        reset: bool,
        /// Property values; a mirror group expands to both sides.
        values: Vec<(String, RigValue)>,
    },
    /// Delete an unused pose declaration.
    DeletePose {
        /// Rig display name or root identity.
        rig: String,
        /// Pose name.
        pose: String,
    },
    /// Add or replace a key inside a clip.
    ClipKey {
        /// Rig display name or root identity.
        rig: String,
        /// Clip name.
        clip: String,
        /// Concrete property (a mirror group is not accepted).
        property: String,
        /// Time inside the clip, in seconds.
        time: f64,
        /// Value.
        value: RigValue,
        /// Ease of the arriving segment.
        ease: Ease,
    },
    /// Remove a key inside a clip.
    ClipRemoveKey {
        /// Rig display name or root identity.
        rig: String,
        /// Clip name.
        clip: String,
        /// Concrete property.
        property: String,
        /// Time inside the clip, in seconds.
        time: f64,
    },
    /// Change a clip's length.
    ClipLength {
        /// Rig display name or root identity.
        rig: String,
        /// Clip name.
        clip: String,
        /// New length in seconds.
        length: f64,
    },
    /// Add or replace a clip playback.
    Play {
        /// Rig display name or root identity.
        rig: String,
        /// Clip name.
        clip: String,
        /// Composition time in seconds.
        at: f64,
        /// Repetitions.
        loops: u32,
        /// Playback speed.
        speed: f64,
    },
    /// Add a state to the machine.
    AddState {
        /// Rig display name or root identity.
        rig: String,
        /// State name.
        name: String,
        /// Pose, when the state holds one.
        pose: Option<String>,
        /// Clip, when the state holds one.
        clip: Option<String>,
        /// Playback speed.
        speed: f64,
        /// Whether the clip loops.
        looping: bool,
    },
    /// Remove a state and its transitions.
    RemoveState {
        /// Rig display name or root identity.
        rig: String,
        /// State name.
        name: String,
    },
    /// Add a transition.
    AddTransition {
        /// Rig display name or root identity.
        rig: String,
        /// Source state; `None` is an any-state transition.
        from: Option<String>,
        /// Target state.
        to: String,
        /// Input property.
        input: String,
        /// Comparison.
        compare: RigCompare,
        /// Right-hand value.
        value: RigValue,
        /// Blend duration in seconds.
        duration: f64,
    },
    /// Remove a transition.
    RemoveTransition {
        /// Rig display name or root identity.
        rig: String,
        /// Source state; `None` is any-state.
        from: Option<String>,
        /// Target state.
        to: String,
        /// Input property.
        input: String,
    },
    /// Retarget a transition to other states.
    RetargetTransition {
        /// Rig display name or root identity.
        rig: String,
        /// Current source state; `None` is any-state.
        from: Option<String>,
        /// Current target state.
        to: String,
        /// Input property.
        input: String,
        /// New source state.
        new_from: Option<String>,
        /// New target state.
        new_to: String,
    },
    /// Change the default of an input property.
    SetInput {
        /// Rig display name or root identity.
        rig: String,
        /// Input property.
        property: String,
        /// New default.
        value: RigValue,
    },
}

/// Applies `edit` to the manifest description, returning the exact expected
/// instance. Used by the editor after compiling the planned source.
///
/// # Errors
///
/// A message when the rig, property, pose, clip or state does not exist, or
/// when a value has the wrong kind.
#[allow(clippy::too_many_lines)]
pub fn apply_to_manifest(
    manifest: &mut RigManifest,
    edit: &RigEdit,
    fps: f64,
) -> Result<(), String> {
    let rig_name = edit_rig(edit);
    let index = manifest
        .rigs
        .iter()
        .position(|rig| rig.name == rig_name || rig.id == rig_name)
        .ok_or_else(|| format!("no rig named {rig_name:?}"))?;
    let instance = &mut manifest.rigs[index];
    match edit {
        RigEdit::Set {
            property,
            side,
            value,
            ..
        } => {
            for (name, flip) in targets(instance, property, *side)? {
                let property = property_mut(instance, &name)?;
                let value = checked_value(property, value)?;
                let value = if flip { flip_value(value) } else { value };
                property.keys.retain(|key| key.frame > 0.0);
                upsert_key(property, 0.0, value, Ease::Linear.name().to_owned());
            }
        }
        RigEdit::Key {
            property,
            side,
            time,
            value,
            ease,
            ..
        } => {
            for (name, flip) in targets(instance, property, *side)? {
                let property = property_mut(instance, &name)?;
                let value = checked_value(property, value)?;
                let value = if flip { flip_value(value) } else { value };
                upsert_key(property, time * fps, value, ease.name().to_owned());
            }
        }
        RigEdit::Unkey {
            property,
            side,
            time,
            ..
        } => {
            for (name, _) in targets(instance, property, *side)? {
                let property = property_mut(instance, &name)?;
                property
                    .keys
                    .retain(|key| (key.frame - time * fps).abs() > 1e-6);
            }
        }
        RigEdit::ApplyPose {
            pose,
            at,
            duration,
            ease,
            ..
        } => {
            if !instance.poses.iter().any(|item| item.name == *pose) {
                return Err(format!("no pose named {pose:?}"));
            }
            let start = at * fps;
            let duration = duration * fps;
            if let Some(track) = instance
                .pose_tracks
                .iter_mut()
                .find(|track| track.pose == *pose)
            {
                track.start = start;
                track.duration = duration;
                track.ease = ease.name().to_owned();
            } else {
                instance
                    .pose_tracks
                    .push(motioner_core::RigPoseTrackManifest {
                        pose: pose.clone(),
                        start,
                        duration,
                        ease: ease.name().to_owned(),
                    });
            }
        }
        RigEdit::SavePose {
            pose,
            reset,
            values,
            ..
        } => {
            let mut resolved: BTreeMap<usize, (String, RigValue)> = BTreeMap::new();
            for (property, value) in values {
                for (name, flip) in targets(instance, property, RigSide::Both)? {
                    let definition = property_ref(instance, &name)?;
                    let value = checked_value(definition, value)?;
                    let value = if flip { flip_value(value) } else { value };
                    resolved.insert(definition_index(instance, &name)?, (name, value));
                }
            }
            let values: Vec<RigPropertyValue> = resolved
                .into_values()
                .map(|(property, value)| RigPropertyValue { property, value })
                .collect();
            if let Some(existing) = instance.poses.iter_mut().find(|item| item.name == *pose) {
                existing.reset = *reset;
                existing.values = values;
            } else {
                instance.poses.push(motioner_core::RigPoseManifest {
                    name: pose.clone(),
                    reset: *reset,
                    values,
                });
            }
        }
        RigEdit::DeletePose { pose, .. } => {
            if instance.pose_tracks.iter().any(|track| track.pose == *pose) {
                return Err(format!("pose {pose:?} is in use by a pose application"));
            }
            let before = instance.poses.len();
            instance.poses.retain(|item| item.name != *pose);
            if instance.poses.len() == before {
                return Err(format!("no pose named {pose:?}"));
            }
        }
        RigEdit::ClipKey {
            clip,
            property,
            time,
            value,
            ease,
            ..
        } => {
            let property_name = concrete_property(instance, property)?;
            let definition = property_ref(instance, &property_name)?;
            let value = checked_value(definition, value)?;
            let clip = clip_mut(instance, clip)?;
            let frame = time * fps;
            if let Some(key) = clip
                .keys
                .iter_mut()
                .find(|key| key.property == property_name && (key.frame - frame).abs() < 1e-6)
            {
                key.value = value;
                key.ease = ease.name().to_owned();
            } else {
                clip.keys.push(motioner_core::RigClipKeyManifest {
                    property: property_name,
                    frame,
                    value,
                    ease: ease.name().to_owned(),
                });
                clip.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
            }
            if frame > clip.length {
                clip.length = frame;
            }
        }
        RigEdit::ClipRemoveKey {
            clip,
            property,
            time,
            ..
        } => {
            let property = concrete_property(instance, property)?;
            let clip = clip_mut(instance, clip)?;
            let frame = time * fps;
            clip.keys
                .retain(|key| !(key.property == property && (key.frame - frame).abs() < 1e-6));
        }
        RigEdit::ClipLength { clip, length, .. } => {
            let clip = clip_mut(instance, clip)?;
            clip.length = length * fps;
        }
        RigEdit::Play {
            clip,
            at,
            loops,
            speed,
            ..
        } => {
            if !instance.clips.iter().any(|item| item.name == *clip) {
                return Err(format!("no clip named {clip:?}"));
            }
            let start = at * fps;
            if let Some(play) = instance
                .plays
                .iter_mut()
                .find(|play| play.clip == *clip && (play.start - start).abs() < 1e-6)
            {
                play.loops = *loops;
                play.speed = *speed;
            } else {
                instance.plays.push(motioner_core::RigPlayManifest {
                    clip: clip.clone(),
                    start,
                    loops: *loops,
                    speed: *speed,
                });
            }
        }
        RigEdit::AddState {
            name,
            pose,
            clip,
            speed,
            looping,
            ..
        } => {
            let machine =
                instance
                    .machine
                    .get_or_insert_with(|| motioner_core::RigMachineManifest {
                        default: name.clone(),
                        inputs: Vec::new(),
                        states: Vec::new(),
                        transitions: Vec::new(),
                        events: Vec::new(),
                    });
            if machine.states.iter().any(|state| state.name == *name) {
                return Err(format!("state {name:?} already exists"));
            }
            if let Some(pose) = pose {
                if !instance.poses.iter().any(|item| item.name == *pose) {
                    return Err(format!("no pose named {pose:?}"));
                }
            }
            if let Some(clip) = clip {
                if !instance.clips.iter().any(|item| item.name == *clip) {
                    return Err(format!("no clip named {clip:?}"));
                }
            }
            machine.states.push(motioner_core::RigStateManifest {
                name: name.clone(),
                pose: pose.clone(),
                clip: clip.clone(),
                speed: *speed,
                looping: *looping,
            });
        }
        RigEdit::RemoveState { name, .. } => {
            let Some(machine) = instance.machine.as_mut() else {
                return Err("the rig has no state machine".to_owned());
            };
            if machine.states.len() <= 1 {
                return Err("a state machine needs at least one state".to_owned());
            }
            let before = machine.states.len();
            machine.states.retain(|state| state.name != *name);
            if machine.states.len() == before {
                return Err(format!("no state named {name:?}"));
            }
            machine.transitions.retain(|transition| {
                transition.from.as_deref() != Some(name.as_str()) && transition.to != *name
            });
            machine.events = machine
                .transitions
                .iter()
                .map(|transition| motioner_core::RigEventManifest {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                })
                .collect();
            if machine.default == *name {
                machine.default = machine
                    .states
                    .first()
                    .map(|state| state.name.clone())
                    .unwrap_or_default();
            }
        }
        RigEdit::AddTransition {
            from,
            to,
            input,
            compare,
            value,
            duration,
            ..
        } => {
            let property = property_ref(instance, input)?;
            let value = checked_value(property, value)?;
            let machine = instance
                .machine
                .as_mut()
                .ok_or_else(|| "the rig has no state machine".to_owned())?;
            if !machine.states.iter().any(|state| state.name == *to) {
                return Err(format!("no state named {to:?}"));
            }
            if let Some(from) = from {
                if !machine.states.iter().any(|state| state.name == *from) {
                    return Err(format!("no state named {from:?}"));
                }
            }
            machine
                .transitions
                .push(motioner_core::RigTransitionManifest {
                    from: from.clone(),
                    to: to.clone(),
                    input: input.clone(),
                    compare: compare_name(*compare).to_owned(),
                    value,
                    duration: duration * fps,
                });
            machine.events = machine
                .transitions
                .iter()
                .map(|transition| motioner_core::RigEventManifest {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                })
                .collect();
        }
        RigEdit::RemoveTransition {
            from, to, input, ..
        } => {
            let machine = instance
                .machine
                .as_mut()
                .ok_or_else(|| "the rig has no state machine".to_owned())?;
            let before = machine.transitions.len();
            machine.transitions.retain(|transition| {
                !(transition.from == *from && transition.to == *to && transition.input == *input)
            });
            if machine.transitions.len() == before {
                return Err(format!(
                    "no transition from {from:?} to {to:?} on {input:?}"
                ));
            }
            machine.events = machine
                .transitions
                .iter()
                .map(|transition| motioner_core::RigEventManifest {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                })
                .collect();
        }
        RigEdit::RetargetTransition {
            from,
            to,
            input,
            new_from,
            new_to,
            ..
        } => {
            let machine = instance
                .machine
                .as_mut()
                .ok_or_else(|| "the rig has no state machine".to_owned())?;
            if let Some(state) = new_from {
                if !machine.states.iter().any(|item| item.name == *state) {
                    return Err(format!("no state named {state:?}"));
                }
            }
            if !machine.states.iter().any(|item| item.name == *new_to) {
                return Err(format!("no state named {new_to:?}"));
            }
            let transition = machine
                .transitions
                .iter_mut()
                .find(|transition| {
                    transition.from == *from && transition.to == *to && transition.input == *input
                })
                .ok_or_else(|| format!("no transition from {from:?} to {to:?} on {input:?}"))?;
            transition.from = new_from.clone();
            transition.to = new_to.clone();
            machine.events = machine
                .transitions
                .iter()
                .map(|transition| motioner_core::RigEventManifest {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                })
                .collect();
        }
        RigEdit::SetInput {
            property, value, ..
        } => {
            let property = property_mut(instance, property)?;
            property.default = checked_value(property, value)?;
        }
    }
    refresh_machine(instance);
    Ok(())
}

/// The manifest derives `events` and `inputs` from transitions, so every
/// transition edit must refresh both.
fn refresh_machine(instance: &mut RigInstanceManifest) {
    let Some(machine) = instance.machine.as_mut() else {
        return;
    };
    machine.events = machine
        .transitions
        .iter()
        .map(|transition| motioner_core::RigEventManifest {
            from: transition.from.clone(),
            to: transition.to.clone(),
        })
        .collect();
    let mut inputs = Vec::new();
    for transition in &machine.transitions {
        if inputs
            .iter()
            .any(|input: &motioner_core::RigInputManifest| input.name == transition.input)
        {
            continue;
        }
        if let Some(property) = instance
            .properties
            .iter()
            .find(|property| property.name == transition.input)
        {
            inputs.push(motioner_core::RigInputManifest {
                name: property.name.clone(),
                kind: property.kind,
                values: property.values.clone(),
            });
        }
    }
    machine.inputs = inputs;
}

fn edit_rig(edit: &RigEdit) -> &str {
    match edit {
        RigEdit::Set { rig, .. }
        | RigEdit::Key { rig, .. }
        | RigEdit::Unkey { rig, .. }
        | RigEdit::ApplyPose { rig, .. }
        | RigEdit::SavePose { rig, .. }
        | RigEdit::DeletePose { rig, .. }
        | RigEdit::ClipKey { rig, .. }
        | RigEdit::ClipRemoveKey { rig, .. }
        | RigEdit::ClipLength { rig, .. }
        | RigEdit::Play { rig, .. }
        | RigEdit::AddState { rig, .. }
        | RigEdit::RemoveState { rig, .. }
        | RigEdit::AddTransition { rig, .. }
        | RigEdit::RemoveTransition { rig, .. }
        | RigEdit::RetargetTransition { rig, .. }
        | RigEdit::SetInput { rig, .. } => rig,
    }
}

fn compare_name(compare: RigCompare) -> &'static str {
    match compare {
        RigCompare::Equals => "equals",
        RigCompare::NotEquals => "not_equals",
        RigCompare::Greater => "greater",
        RigCompare::Less => "less",
        RigCompare::Trigger => "trigger",
    }
}

fn property_index(instance: &RigInstanceManifest, name: &str) -> Option<usize> {
    instance
        .properties
        .iter()
        .position(|property| property.name == name)
}

fn property_ref<'a>(
    instance: &'a RigInstanceManifest,
    name: &str,
) -> Result<&'a motioner_core::RigPropertyManifest, String> {
    instance
        .properties
        .get(property_index(instance, name).ok_or_else(|| format!("no property named {name:?}"))?)
        .ok_or_else(|| format!("no property named {name:?}"))
}

fn property_mut<'a>(
    instance: &'a mut RigInstanceManifest,
    name: &str,
) -> Result<&'a mut motioner_core::RigPropertyManifest, String> {
    let index =
        property_index(instance, name).ok_or_else(|| format!("no property named {name:?}"))?;
    instance
        .properties
        .get_mut(index)
        .ok_or_else(|| format!("no property named {name:?}"))
}

fn definition_index(instance: &RigInstanceManifest, name: &str) -> Result<usize, String> {
    property_index(instance, name).ok_or_else(|| format!("no property named {name:?}"))
}

fn concrete_property(instance: &RigInstanceManifest, name: &str) -> Result<String, String> {
    let property = property_ref(instance, name)?;
    if property.mirror.is_some() {
        return Err(format!(
            "{name:?} is a mirror group; clip keys need a concrete property name"
        ));
    }
    Ok(property.name.clone())
}

/// The concrete property names an edit touches, plus whether the value is
/// flipped for that side.
fn targets(
    instance: &RigInstanceManifest,
    name: &str,
    side: RigSide,
) -> Result<Vec<(String, bool)>, String> {
    if let Some(property) = instance.properties.iter().find(|item| item.name == name) {
        return Ok(vec![(property.name.clone(), false)]);
    }
    let mirrored: Vec<&motioner_core::RigPropertyManifest> = instance
        .properties
        .iter()
        .filter(|property| property.group.as_deref() == Some(name))
        .collect();
    if mirrored.is_empty() {
        return Err(format!("no property or mirror group named {name:?}"));
    }
    let mut result = Vec::new();
    for property in mirrored {
        let Some(mirror) = &property.mirror else {
            continue;
        };
        let include = match side {
            RigSide::Both => true,
            RigSide::Left => mirror.side == RigMirrorSide::Left,
            RigSide::Right => mirror.side == RigMirrorSide::Right,
        };
        if include {
            let flip = side == RigSide::Both && mirror.side == RigMirrorSide::Right && mirror.flip;
            result.push((property.name.clone(), flip));
        }
    }
    if result.is_empty() {
        return Err(format!("mirror group {name:?} has no {side:?} side"));
    }
    Ok(result)
}

/// The property name or mirror group to write in source for `name`/`side`.
fn call_target(
    instance: &RigInstanceManifest,
    name: &str,
    side: RigSide,
) -> Result<String, String> {
    if side == RigSide::Both {
        if instance
            .properties
            .iter()
            .any(|property| property.name == name)
        {
            return Ok(name.to_owned());
        }
        if instance
            .properties
            .iter()
            .any(|property| property.group.as_deref() == Some(name))
        {
            return Ok(name.to_owned());
        }
    }
    let resolved = targets(instance, name, side)?;
    if resolved.len() == 1 {
        return Ok(resolved[0].0.clone());
    }
    Err(format!("{name:?} resolves to more than one property"))
}

fn checked_value(
    property: &motioner_core::RigPropertyManifest,
    value: &RigValue,
) -> Result<RigValue, String> {
    let ok = match property.kind {
        RigValueKind::Float | RigValueKind::Int => matches!(value, RigValue::Number(_)),
        RigValueKind::Bool => matches!(value, RigValue::Bool(_)),
        RigValueKind::Enum => matches!(value, RigValue::Text(_)),
        RigValueKind::Color => matches!(value, RigValue::Color(_)),
        RigValueKind::Vec2 => matches!(value, RigValue::Vec2(_)),
    };
    if ok {
        Ok(value.clone())
    } else {
        Err(format!(
            "property {:?} expects {}, got {value:?}",
            property.name,
            property.kind.name()
        ))
    }
}

fn flip_value(value: RigValue) -> RigValue {
    match value {
        RigValue::Number(number) => RigValue::Number(-number),
        RigValue::Vec2(vector) => RigValue::Vec2(Vec2::new(-vector.x, vector.y)),
        other => other,
    }
}

fn upsert_key(
    property: &mut motioner_core::RigPropertyManifest,
    frame: f64,
    value: RigValue,
    ease: String,
) {
    if let Some(key) = property
        .keys
        .iter_mut()
        .find(|key| (key.frame - frame).abs() < 1e-6)
    {
        key.value = value;
        key.ease = ease;
    } else {
        property
            .keys
            .push(RigPropertyKeyManifest { frame, value, ease });
        property.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    }
}

fn clip_mut<'a>(
    instance: &'a mut RigInstanceManifest,
    name: &str,
) -> Result<&'a mut motioner_core::RigClipManifest, String> {
    instance
        .clips
        .iter_mut()
        .find(|clip| clip.name == name)
        .ok_or_else(|| format!("no clip named {name:?}"))
}

// ---- source planning ------------------------------------------------------

/// Plans `edit` against `program`. `document` supplies the rig manifest and
/// the composition rate; `origins` locate the block that binds the rig root.
#[must_use]
pub fn plan_rig_edit(
    program: &str,
    origins: &ValueOrigins,
    document: &motioner_core::Document,
    edit: &RigEdit,
) -> Plan {
    let rig_name = edit_rig(edit);
    let Some(instance) = document
        .rig_manifest()
        .rigs
        .iter()
        .find(|rig| rig.name == rig_name || rig.id == rig_name)
    else {
        return Plan::Fallback(format!("no rig named {rig_name:?}"));
    };
    let Some(root) = document.find(&instance.id) else {
        return Plan::Fallback(format!(
            "the rig root {:?} is not in the document",
            instance.id
        ));
    };
    let program_ast = match crate::parse(program) {
        Ok(program) => program,
        Err(error) => return Plan::Fallback(format!("the program does not parse: {error}")),
    };
    let Some(scope) = BindingScope::find(&program_ast, origins, root, program) else {
        return Plan::Fallback(
            "the rig is created inside a module or a helper; edit its declaration in that file"
                .to_owned(),
        );
    };
    let mut calls = Vec::new();
    walk_block(
        scope.body,
        &scope.var,
        &mut BTreeMap::new(),
        scope.after,
        program,
        &mut calls,
    );
    let _ = document.composition().frame_rate().as_f64();
    match plan_calls(&scope, instance, edit, &calls) {
        Ok(edit) => Plan::Code(edit),
        Err(reason) => Plan::Fallback(reason),
    }
}

struct BindingScope<'a> {
    var: String,
    body: &'a [Stmt],
    after: usize,
    indent: String,
}

impl<'a> BindingScope<'a> {
    fn find(
        program: &'a Program,
        origins: &ValueOrigins,
        root: NodeId,
        source: &str,
    ) -> Option<Self> {
        let origin = origins.node_origin(root)?;
        for binding in &origin.bindings {
            let name = origins.name(binding.name).to_owned();
            let Some(text) = source.get(binding.statement.start..binding.statement.end) else {
                continue;
            };
            let trimmed = text.trim_start();
            if !trimmed.starts_with(&name) || !trimmed[name.len()..].trim_start().starts_with('=') {
                continue;
            }
            if let Some((body, index)) = find_assignment(&program.body, binding.statement) {
                let indent = line_of(source, binding.statement.start).2;
                return Some(Self {
                    var: name,
                    body,
                    after: body[index].span.end,
                    indent,
                });
            }
        }
        None
    }
}

fn find_assignment(body: &[Stmt], span: Span) -> Option<(&[Stmt], usize)> {
    for (index, statement) in body.iter().enumerate() {
        if statement.span.start == span.start
            && statement.span.end == span.end
            && matches!(statement.kind, StmtKind::Assign { .. })
        {
            return Some((body, index));
        }
        match &statement.kind {
            StmtKind::Def(function) => {
                if let Some(found) = find_assignment(&function.body, span) {
                    return Some(found);
                }
            }
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (_, branch) in branches {
                    if let Some(found) = find_assignment(branch, span) {
                        return Some(found);
                    }
                }
                if let Some(found) = find_assignment(otherwise, span) {
                    return Some(found);
                }
            }
            StmtKind::For { body, .. } => {
                if let Some(found) = find_assignment(body, span) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Debug, Clone)]
struct CallSite {
    stmt: Span,
    call: Span,
    indent: String,
    var: String,
    method: String,
    property: Option<String>,
    clip: Option<String>,
    /// The variable the call is assigned to (`j = m.clip(...)`).
    bound: Option<String>,
    args: Vec<Argument>,
    line_start: usize,
    line_end: usize,
}

impl CallSite {
    fn keyword(&self, name: &str) -> Option<&Expr> {
        self.args
            .iter()
            .find(|argument| argument.name.as_deref() == Some(name))
            .map(|argument| &argument.value)
    }

    fn positional(&self, index: usize) -> Option<&Expr> {
        self.args
            .iter()
            .filter(|argument| argument.name.is_none())
            .nth(index)
            .map(|argument| &argument.value)
    }

    fn number(&self, keyword: &str, positional: usize) -> Option<f64> {
        self.keyword(keyword)
            .or_else(|| self.positional(positional))
            .and_then(number_of)
    }

    fn text(&self, keyword: &str, positional: usize) -> Option<String> {
        self.keyword(keyword)
            .or_else(|| self.positional(positional))
            .and_then(text_of)
    }
}

fn walk_block(
    body: &[Stmt],
    var: &str,
    clips: &mut BTreeMap<String, String>,
    after: usize,
    source: &str,
    calls: &mut Vec<CallSite>,
) {
    for statement in body {
        if statement.span.start < after {
            continue;
        }
        let bound = match &statement.kind {
            StmtKind::Assign {
                target: Target::Name(name),
                ..
            } => Some(name.to_string()),
            _ => None,
        };
        visit_statement(
            statement,
            var,
            clips,
            bound.as_deref(),
            after,
            source,
            calls,
        );
        if let StmtKind::Assign {
            target: Target::Name(name),
            value,
            ..
        } = &statement.kind
        {
            if let Some((receiver, method, _, args)) = call_of(value) {
                if receiver == var && method == "clip" {
                    if let Some(clip) = args
                        .iter()
                        .find(|argument| argument.name.is_none())
                        .and_then(|argument| text_of(&argument.value))
                    {
                        clips.insert(name.to_string(), clip);
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn visit_statement(
    statement: &Stmt,
    var: &str,
    clips: &BTreeMap<String, String>,
    bound: Option<&str>,
    after: usize,
    source: &str,
    calls: &mut Vec<CallSite>,
) {
    match &statement.kind {
        StmtKind::Expr(expr) => {
            visit_expr(
                expr,
                var,
                clips,
                bound,
                statement.span,
                after,
                source,
                calls,
            );
        }
        StmtKind::Assign { value, .. } | StmtKind::Return(Some(value)) => {
            visit_expr(
                value,
                var,
                clips,
                bound,
                statement.span,
                after,
                source,
                calls,
            );
        }
        StmtKind::If {
            branches,
            otherwise,
        } => {
            for (condition, body) in branches {
                visit_expr(
                    condition,
                    var,
                    clips,
                    None,
                    statement.span,
                    after,
                    source,
                    calls,
                );
                let mut nested = clips.clone();
                walk_block(body, var, &mut nested, after, source, calls);
            }
            let mut nested = clips.clone();
            walk_block(otherwise, var, &mut nested, after, source, calls);
        }
        StmtKind::For { iterable, body, .. } => {
            visit_expr(
                iterable,
                var,
                clips,
                None,
                statement.span,
                after,
                source,
                calls,
            );
            let mut nested = clips.clone();
            walk_block(body, var, &mut nested, after, source, calls);
        }
        StmtKind::Def(_)
        | StmtKind::Return(None)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Pass => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn visit_expr(
    expr: &Expr,
    scope_var: &str,
    clips: &BTreeMap<String, String>,
    bound: Option<&str>,
    statement: Span,
    after: usize,
    source: &str,
    calls: &mut Vec<CallSite>,
) {
    if let Some((receiver, method, property, args)) = call_of(expr) {
        let clip = clips.get(&receiver).cloned();
        let is_scope = receiver == scope_var;
        let is_clip = clip.is_some() && property.is_none() && method == "key";
        if (is_scope || is_clip) && statement.start >= after {
            let (line_start, line_end, indent) = line_of(source, statement.start);
            calls.push(CallSite {
                stmt: statement,
                call: expr.span,
                indent,
                var: receiver,
                method,
                property,
                clip: if is_clip { clip } else { None },
                bound: bound.map(str::to_owned),
                args: args.to_vec(),
                line_start,
                line_end,
            });
        }
    }
    if let ExprKind::Call { args, .. } = &expr.kind {
        for argument in args {
            visit_expr(
                &argument.value,
                scope_var,
                clips,
                None,
                statement,
                after,
                source,
                calls,
            );
        }
    }
}

/// The receiver, method, property chain and arguments of a call.
///
/// `m.look.key(...)` is `("m", "key", Some("look"), args)`; `j.key(...)` is
/// `("j", "key", None, args)` and the caller resolves `j` as a clip variable.
fn call_of(expr: &Expr) -> Option<(String, String, Option<String>, &[Argument])> {
    let ExprKind::Call { callee, args } = &expr.kind else {
        return None;
    };
    let ExprKind::Attribute { object, name } = &callee.kind else {
        return None;
    };
    match &object.kind {
        ExprKind::Name(receiver) => Some((receiver.to_string(), name.to_string(), None, args)),
        ExprKind::Attribute {
            object: inner,
            name: property,
        } => match &inner.kind {
            ExprKind::Name(receiver) => Some((
                receiver.to_string(),
                name.to_string(),
                Some(property.to_string()),
                args,
            )),
            _ => None,
        },
        _ => None,
    }
}

fn line_of(source: &str, offset: usize) -> (usize, usize, String) {
    let start = source[..offset.min(source.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let end = source[offset.min(source.len())..]
        .find('\n')
        .map_or(source.len(), |index| offset.min(source.len()) + index + 1);
    let indent = source[start..offset.min(source.len())]
        .chars()
        .take_while(|character| character.is_whitespace() && *character != '\n')
        .collect();
    (start, end, indent)
}

fn number_of(expr: &Expr) -> Option<f64> {
    match &expr.kind {
        ExprKind::Number(number) => Some(*number),
        ExprKind::Unary { op, operand } if *op == crate::ast::UnaryOp::Neg => {
            number_of(operand).map(|number| -number)
        }
        _ => None,
    }
}

fn text_of(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Str(text) => Some(text.to_string()),
        _ => None,
    }
}

fn value_text(value: &RigValue) -> String {
    match value {
        RigValue::Number(number) => format_number(*number),
        RigValue::Bool(flag) => if *flag { "True" } else { "False" }.to_owned(),
        RigValue::Text(text) => quote(text),
        RigValue::Vec2(pair) => pair_text(*pair),
        RigValue::Color(color) => quote(&color.to_hex()),
    }
}

#[allow(clippy::too_many_lines)]
fn plan_calls(
    scope: &BindingScope<'_>,
    instance: &RigInstanceManifest,
    edit: &RigEdit,
    calls: &[CallSite],
) -> Result<CodeEdit, String> {
    let var = &scope.var;
    let line_end = calls
        .iter()
        .map(|call| call.line_end)
        .max()
        .unwrap_or_else(|| scope.body.last().map_or(0, |statement| statement.span.end));
    match edit {
        RigEdit::Set {
            property,
            side,
            value,
            ..
        } => {
            let target = call_target(instance, property, *side)?;
            let text = format!("{var}.{target}.set({})", value_text(value));
            set_property_call(&target, calls, line_end, &scope.indent, text)
        }
        RigEdit::Key {
            property,
            side,
            time,
            value,
            ease,
            ..
        } => {
            let target = call_target(instance, property, *side)?;
            let text = format!(
                "{var}.{target}.key({}, {}, ease={})",
                format_number(*time),
                value_text(value),
                quote(ease.name())
            );
            Ok(key_property_call(
                &target,
                *time,
                calls,
                line_end,
                &scope.indent,
                &text,
            ))
        }
        RigEdit::Unkey {
            property,
            side,
            time,
            ..
        } => {
            let target = call_target(instance, property, *side)?;
            unkey_property_call(&target, *time, calls)
        }
        RigEdit::ApplyPose {
            pose,
            at,
            duration,
            ease,
            ..
        } => {
            let text = format!(
                "{var}.pose({}, at={}, dur={}, ease={})",
                quote(pose),
                format_number(*at),
                format_number(*duration),
                quote(ease.name())
            );
            let existing = calls.iter().find(|call| {
                call.method == "pose"
                    && call.text("name", 0).as_deref() == Some(pose)
                    && call.number("at", 1).is_some()
            });
            Ok(match existing {
                Some(call) => code_edit(
                    vec![Replacement::new(call.call.start, call.call.end, text)],
                    "apply rig pose",
                ),
                None => insert_after_last_pose(calls, line_end, &scope.indent, &text),
            })
        }
        RigEdit::SavePose {
            pose,
            reset,
            values,
            ..
        } => {
            let text = pose_declaration(var, pose, *reset, values)?;
            if instance.poses.iter().any(|item| item.name == *pose)
                && !calls.iter().any(|call| {
                    call.method == "pose"
                        && call.text("name", 0).as_deref() == Some(pose)
                        && call.number("at", 1).is_none()
                })
            {
                return Err(format!(
                    "pose {pose:?} is declared in a used module; edit that file"
                ));
            }
            let existing = calls.iter().find(|call| {
                call.method == "pose"
                    && call.text("name", 0).as_deref() == Some(pose)
                    && call.number("at", 1).is_none()
            });
            Ok(match existing {
                Some(call) => code_edit(
                    vec![Replacement::new(call.call.start, call.call.end, text)],
                    "save rig pose",
                ),
                None => insert_after_last_pose(calls, line_end, &scope.indent, &text),
            })
        }
        RigEdit::DeletePose { pose, .. } => {
            let Some(call) = calls.iter().find(|call| {
                call.method == "pose"
                    && call.text("name", 0).as_deref() == Some(pose)
                    && call.number("at", 1).is_none()
            }) else {
                return Err(format!(
                    "pose {pose:?} is declared in a used module; edit that file"
                ));
            };
            if instance.pose_tracks.iter().any(|track| track.pose == *pose) {
                return Err(format!("pose {pose:?} is in use by a pose application"));
            }
            Ok(remove_line(call, "delete rig pose"))
        }
        RigEdit::ClipKey {
            clip,
            property,
            time,
            value,
            ease,
            ..
        } => {
            let var = clip_var(calls, clip).ok_or_else(|| {
                format!("clip {clip:?} is declared in a used module; edit that file")
            })?;
            let target = concrete_property(instance, property)?;
            let text = format!(
                "{var}.key({}, {}, {}, ease={})",
                quote(&target),
                format_number(*time),
                value_text(value),
                quote(ease.name())
            );
            let existing = calls.iter().find(|call| {
                call.clip.as_deref() == Some(clip)
                    && call.method == "key"
                    && call.text("property", 0).as_deref() == Some(target.as_str())
                    && call
                        .number("time", 1)
                        .is_some_and(|value| (value - time).abs() < 1e-6)
            });
            Ok(match existing {
                Some(call) => code_edit(
                    vec![Replacement::new(call.call.start, call.call.end, text)],
                    "edit clip key",
                ),
                None => insert_key_call(calls, clip, *time, line_end, &scope.indent, &text),
            })
        }
        RigEdit::ClipRemoveKey {
            clip,
            property,
            time,
            ..
        } => {
            let target = concrete_property(instance, property)?;
            let call = calls
                .iter()
                .find(|call| {
                    call.clip.as_deref() == Some(clip)
                        && call.method == "key"
                        && call.text("property", 0).as_deref() == Some(target.as_str())
                        && call
                            .number("time", 1)
                            .is_some_and(|value| (value - time).abs() < 1e-6)
                })
                .ok_or_else(|| "no matching clip key".to_owned())?;
            Ok(remove_line(call, "remove clip key"))
        }
        RigEdit::ClipLength { clip, length, .. } => {
            let call = calls
                .iter()
                .find(|call| call.method == "clip" && call.text("name", 0).as_deref() == Some(clip))
                .ok_or_else(|| {
                    format!("clip {clip:?} is declared in a used module; edit that file")
                })?;
            let text = format!(
                "{}.clip({}, length={})",
                call.var,
                quote(clip),
                format_number(*length)
            );
            Ok(code_edit(
                vec![Replacement::new(call.call.start, call.call.end, text)],
                "edit clip length",
            ))
        }
        RigEdit::Play {
            clip,
            at,
            loops,
            speed,
            ..
        } => {
            let text = format!(
                "{var}.play({}, at={}, loops={}, speed={})",
                quote(clip),
                format_number(*at),
                loops,
                format_number(*speed)
            );
            let existing = calls.iter().find(|call| {
                call.method == "play"
                    && call.text("name", 0).as_deref() == Some(clip)
                    && call
                        .number("at", 1)
                        .is_some_and(|value| (value - at).abs() < 1e-6)
            });
            Ok(match existing {
                Some(call) => code_edit(
                    vec![Replacement::new(call.call.start, call.call.end, text)],
                    "edit clip playback",
                ),
                None => insert_after_method(calls, "play", line_end, &scope.indent, &text),
            })
        }
        RigEdit::AddState {
            name,
            pose,
            clip,
            speed,
            looping,
            ..
        } => {
            if instance
                .machine
                .as_ref()
                .is_some_and(|machine| machine.states.iter().any(|state| state.name == *name))
            {
                return Err(format!("state {name:?} already exists"));
            }
            let mut text = format!("{var}.state({}", quote(name));
            if let Some(pose) = pose {
                let _ = write!(text, ", pose={}", quote(pose));
            }
            if let Some(clip) = clip {
                let _ = write!(text, ", clip={}", quote(clip));
            }
            if (*speed - 1.0).abs() > f64::EPSILON {
                let _ = write!(text, ", speed={}", format_number(*speed));
            }
            if !*looping {
                text.push_str(", looping=False");
            }
            text.push(')');
            Ok(insert_after_method(
                calls,
                "state",
                line_end,
                &scope.indent,
                &text,
            ))
        }
        RigEdit::RemoveState { name, .. } => {
            let declaration = calls.iter().find(|call| {
                call.method == "state" && call.text("name", 0).as_deref() == Some(name)
            });
            let Some(declaration) = declaration else {
                return Err(format!(
                    "state {name:?} is declared in a used module; edit that file"
                ));
            };
            let mut replacements = vec![Replacement::new(
                declaration.line_start,
                declaration.line_end,
                String::new(),
            )];
            for call in calls {
                if call.method == "transition"
                    && (call.text("from", 0).as_deref() == Some(name)
                        || call.text("to", 1).as_deref() == Some(name))
                {
                    replacements.push(Replacement::new(
                        call.line_start,
                        call.line_end,
                        String::new(),
                    ));
                }
            }
            Ok(code_edit(replacements, "remove rig state"))
        }
        RigEdit::AddTransition {
            from,
            to,
            input,
            compare,
            value,
            duration,
            ..
        } => {
            let mut text = format!(
                "{var}.transition({}, {}",
                quote(from.as_deref().unwrap_or("*")),
                quote(to)
            );
            if *compare == RigCompare::Trigger {
                let _ = write!(text, ", trigger={}", quote(input));
            } else {
                let _ = write!(
                    text,
                    ", input={}, {}={}",
                    quote(input),
                    compare.arg_name(),
                    value_text(value)
                );
            }
            if *duration > 0.0 {
                let _ = write!(text, ", dur={}", format_number(*duration));
            }
            text.push(')');
            Ok(insert_after_method(
                calls,
                "transition",
                line_end,
                &scope.indent,
                &text,
            ))
        }
        RigEdit::RemoveTransition {
            from, to, input, ..
        } => {
            let call = calls
                .iter()
                .find(|call| {
                    call.method == "transition"
                        && call.text("from", 0).as_deref() == Some(from.as_deref().unwrap_or("*"))
                        && call.text("to", 1).as_deref() == Some(to)
                        && (call.text("input", 2).as_deref() == Some(input)
                            || call.text("trigger", 2).as_deref() == Some(input))
                })
                .ok_or_else(|| "no matching transition".to_owned())?;
            Ok(remove_line(call, "remove rig transition"))
        }
        RigEdit::RetargetTransition {
            from,
            to,
            input,
            new_from,
            new_to,
            ..
        } => {
            let call = calls
                .iter()
                .find(|call| {
                    call.method == "transition"
                        && call.text("from", 0).as_deref() == Some(from.as_deref().unwrap_or("*"))
                        && call.text("to", 1).as_deref() == Some(to)
                        && (call.text("input", 2).as_deref() == Some(input)
                            || call.text("trigger", 2).as_deref() == Some(input))
                })
                .ok_or_else(|| "no matching transition".to_owned())?;
            let from_expr = call.keyword("from").or_else(|| call.positional(0));
            let to_expr = call.keyword("to").or_else(|| call.positional(1));
            let Some(from_expr) = from_expr else {
                return Err("the transition has no readable source state".to_owned());
            };
            let Some(to_expr) = to_expr else {
                return Err("the transition has no readable target state".to_owned());
            };
            Ok(code_edit(
                vec![
                    Replacement::new(
                        from_expr.span.start,
                        from_expr.span.end,
                        quote(new_from.as_deref().unwrap_or("*")),
                    ),
                    Replacement::new(to_expr.span.start, to_expr.span.end, quote(new_to)),
                ],
                "retarget rig transition",
            ))
        }
        RigEdit::SetInput {
            property, value, ..
        } => {
            let call = calls
                .iter()
                .find(|call| {
                    (call.method == "input" || call.method == "prop")
                        && call.text("name", 0).as_deref() == Some(property)
                })
                .ok_or_else(|| {
                    format!("input {property:?} is declared in a used module; edit that file")
                })?;
            let default = call
                .keyword("default")
                .or_else(|| call.positional(1))
                .ok_or_else(|| format!("input {property:?} has no readable default"))?;
            Ok(code_edit(
                vec![Replacement::new(
                    default.span.start,
                    default.span.end,
                    value_text(value),
                )],
                "set rig input",
            ))
        }
    }
}

impl BindingScope<'_> {}

fn pose_declaration(
    var: &str,
    pose: &str,
    reset: bool,
    values: &[(String, RigValue)],
) -> Result<String, String> {
    let mut parts = vec![quote(pose)];
    if reset {
        parts.push("reset=True".to_owned());
    }
    for (property, value) in values {
        if !is_identifier(property) {
            return Err(format!("{property:?} is not a valid MDF identifier"));
        }
        parts.push(format!("{property}={}", value_text(value)));
    }
    Ok(format!("{var}.pose({})", parts.join(", ")))
}

fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn set_property_call(
    target: &str,
    calls: &[CallSite],
    line_end: usize,
    indent: &str,
    text: String,
) -> Result<CodeEdit, String> {
    let mut calls: Vec<&CallSite> = calls
        .iter()
        .filter(|call| call.property.as_deref() == Some(target))
        .collect();
    calls.sort_by_key(|call| call.stmt.start);
    if calls.is_empty() {
        return Ok(code_edit(
            vec![insertion(line_end, indent, &text)],
            "set rig property",
        ));
    }
    let existing_set = calls.iter().find(|call| call.method == "set").copied();
    let last_key = calls
        .iter()
        .rev()
        .find(|call| call.method == "key")
        .copied();
    if let Some(set) = existing_set {
        if last_key.is_none() || set.stmt.start > last_key.map_or(0, |key| key.stmt.start) {
            return Ok(code_edit(
                vec![Replacement::new(set.call.start, set.call.end, text)],
                "set rig property",
            ));
        }
    }
    let mut replacements = Vec::new();
    if let Some(set) = existing_set {
        replacements.push(Replacement::new(
            set.line_start,
            set.line_end,
            String::new(),
        ));
    }
    let anchor = last_key
        .or_else(|| calls.last().copied())
        .ok_or_else(|| format!("no {target:?} call to anchor the edit"))?;
    let (offset, indent) = (anchor.line_end, anchor.indent.clone());
    replacements.push(insertion(offset, &indent, &text));
    Ok(code_edit(replacements, "set rig property"))
}

fn key_property_call(
    target: &str,
    time: f64,
    calls: &[CallSite],
    line_end: usize,
    indent: &str,
    text: &str,
) -> CodeEdit {
    let mut calls: Vec<&CallSite> = calls
        .iter()
        .filter(|call| call.property.as_deref() == Some(target))
        .collect();
    calls.sort_by_key(|call| call.stmt.start);
    if let Some(call) = calls.iter().find(|call| {
        call.method == "key"
            && call
                .number("time", 0)
                .is_some_and(|value| (value - time).abs() < 1e-6)
    }) {
        return code_edit(
            vec![Replacement::new(call.call.start, call.call.end, text)],
            "edit rig property key",
        );
    }
    let before = calls.iter().find(|call| {
        call.method == "key" && call.number("time", 0).is_some_and(|value| value > time)
    });
    let replacement = match before {
        Some(call) => Replacement::new(
            call.line_start,
            call.line_start,
            format!("{}{text}\n", call.indent),
        ),
        None => match calls.last().copied() {
            Some(call) => insertion(call.line_end, &call.indent, text),
            None => insertion(line_end, indent, text),
        },
    };
    code_edit(vec![replacement], "add rig property key")
}

fn unkey_property_call(target: &str, time: f64, calls: &[CallSite]) -> Result<CodeEdit, String> {
    let call = calls
        .iter()
        .find(|call| {
            call.property.as_deref() == Some(target)
                && call.method == "key"
                && call
                    .number("time", 0)
                    .is_some_and(|value| (value - time).abs() < 1e-6)
        })
        .ok_or_else(|| format!("no key on {target:?} at {time}"))?;
    Ok(remove_line(call, "remove rig property key"))
}

fn clip_var(calls: &[CallSite], clip: &str) -> Option<String> {
    calls
        .iter()
        .find(|call| call.method == "clip" && call.text("name", 0).as_deref() == Some(clip))
        .and_then(|call| call.bound.clone())
}

fn insert_key_call(
    calls: &[CallSite],
    clip: &str,
    time: f64,
    line_end: usize,
    indent: &str,
    text: &str,
) -> CodeEdit {
    let before = calls.iter().find(|call| {
        call.clip.as_deref() == Some(clip)
            && call.method == "key"
            && call.number("time", 1).is_some_and(|value| value > time)
    });
    if let Some(call) = before {
        code_edit(
            vec![Replacement::new(
                call.line_start,
                call.line_start,
                format!("{}{text}\n", call.indent),
            )],
            "add clip key",
        )
    } else {
        let anchor = calls
            .iter()
            .rfind(|call| call.clip.as_deref() == Some(clip) && call.method == "key");
        match anchor {
            Some(call) => code_edit(
                vec![insertion(call.line_end, &call.indent, text)],
                "add clip key",
            ),
            None => code_edit(vec![insertion(line_end, indent, text)], "add clip key"),
        }
    }
}

fn insert_after_last_pose(
    calls: &[CallSite],
    line_end: usize,
    indent: &str,
    text: &str,
) -> CodeEdit {
    insert_after_method(calls, "pose", line_end, indent, text)
}

fn insert_after_method(
    calls: &[CallSite],
    method: &str,
    line_end: usize,
    indent: &str,
    text: &str,
) -> CodeEdit {
    match calls.iter().rfind(|call| call.method == method) {
        Some(call) => code_edit(
            vec![insertion(call.line_end, &call.indent, text)],
            format!("add rig {method}"),
        ),
        None => code_edit(
            vec![insertion(line_end, indent, text)],
            format!("add rig {method}"),
        ),
    }
}

fn remove_line(call: &CallSite, summary: &str) -> CodeEdit {
    code_edit(
        vec![Replacement::new(
            call.line_start,
            call.line_end,
            String::new(),
        )],
        summary,
    )
}

fn insertion(offset: usize, indent: &str, text: &str) -> Replacement {
    Replacement::new(offset, offset, format!("{indent}{text}\n"))
}

fn code_edit(replacements: Vec<Replacement>, summary: impl Into<String>) -> CodeEdit {
    CodeEdit::new(replacements, summary, Strategy::RigEdit).expect("rig edits do not overlap")
}
