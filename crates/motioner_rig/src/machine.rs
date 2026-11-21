//! State machines: inputs, states, transitions and events (D-0053).
//!
//! The machine is stepped sequentially by [`crate::bake`]. Each frame first
//! resolves the stateless property values, then the runner decides whether a
//! transition fires, how far it is, and blends the old and new state values.
//! Replaying from the first frame is the documented way to seek backwards;
//! there is no hidden clock.

use std::collections::BTreeMap;

use crate::{PropKind, PropValue, Rig};

/// How a transition condition compares an input with a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Compare {
    /// `input == value` (numbers compare exactly, enum/bool by identity).
    Equals,
    /// `input != value`.
    NotEquals,
    /// `input > value` (numbers only).
    Greater,
    /// `input < value` (numbers only).
    Less,
    /// A trigger: true when the boolean input changes from false to true.
    Trigger,
}

/// One state: a base state, a pose or a clip.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// State name (unique).
    pub name: String,
    /// Pose index, when the state holds a pose.
    pub pose: Option<usize>,
    /// Clip index, when the state holds a clip.
    pub clip: Option<usize>,
    /// Clip playback speed.
    pub speed: f64,
    /// Whether the clip loops.
    pub looping: bool,
}

/// One transition.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// Source state, or `None` for an any-state transition.
    pub from: Option<usize>,
    /// Target state.
    pub to: usize,
    /// Input property index.
    pub input: usize,
    /// Comparison.
    pub compare: Compare,
    /// Right-hand value.
    pub value: PropValue,
    /// Blend length in frames.
    pub duration: f64,
}

/// A transition or clip event emitted at an exact frame.
#[derive(Debug, Clone, PartialEq)]
pub struct MachineEvent {
    /// Frame the event happened on.
    pub frame: f64,
    /// State left (`None` when there was no state).
    pub from: Option<usize>,
    /// State entered.
    pub to: usize,
}

/// A state machine.
#[derive(Debug, Clone, PartialEq)]
pub struct Machine {
    /// States; the first matching name is the initial state when `default`
    /// is out of range.
    pub states: Vec<State>,
    /// Transitions in evaluation order; the first matching one wins.
    pub transitions: Vec<Transition>,
    /// Initial state index.
    pub default: usize,
}

impl Machine {
    /// Whether a state transition condition matches the current property map.
    #[must_use]
    pub fn matches(
        transition: &Transition,
        properties: &BTreeMap<usize, PropValue>,
        previous: Option<&PropValue>,
    ) -> bool {
        let Some(current) = properties.get(&transition.input) else {
            return false;
        };
        match transition.compare {
            Compare::Equals => *current == transition.value,
            Compare::NotEquals => *current != transition.value,
            Compare::Greater => match (current.number(), transition.value.number()) {
                (Some(a), Some(b)) => a > b,
                _ => false,
            },
            Compare::Less => match (current.number(), transition.value.number()) {
                (Some(a), Some(b)) => a < b,
                _ => false,
            },
            Compare::Trigger => {
                let now = matches!(current, PropValue::Bool(true))
                    || current.number().is_some_and(|value| value != 0.0);
                let before = previous.is_some_and(|value| match value {
                    PropValue::Bool(value) => *value,
                    other => other.number().is_some_and(|value| value != 0.0),
                });
                now && !before
            }
        }
    }
}

/// The sequential machine state during a bake.
#[derive(Debug, Clone)]
pub(crate) struct Runner {
    current: usize,
    entered: f64,
    pending: Option<Pending>,
    previous_inputs: BTreeMap<usize, PropValue>,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    from: usize,
    to: usize,
    start: f64,
    duration: f64,
}

impl Runner {
    pub(crate) fn new(machine: &Machine) -> Self {
        Self {
            current: machine.default.min(machine.states.len().saturating_sub(1)),
            entered: f64::NEG_INFINITY,
            pending: None,
            previous_inputs: BTreeMap::new(),
        }
    }

    /// Steps the machine at `frame` and writes the state values into `values`.
    pub(crate) fn step(
        &mut self,
        rig: &Rig,
        machine: &Machine,
        frame: f64,
        values: &mut BTreeMap<usize, PropValue>,
        events: &mut Vec<MachineEvent>,
    ) {
        // Complete a finished blend before considering a new transition.
        if let Some(pending) = self.pending {
            if frame >= pending.start + pending.duration {
                self.current = pending.to;
                self.entered = pending.start + pending.duration;
                self.pending = None;
            }
        }
        if self.pending.is_none() {
            for transition in &machine.transitions {
                let from_matches = transition.from.is_none_or(|from| from == self.current);
                if !from_matches {
                    continue;
                }
                let previous = self.previous_inputs.get(&transition.input);
                if !Machine::matches(transition, values, previous) {
                    continue;
                }
                let from = self.current;
                events.push(MachineEvent {
                    frame,
                    from: Some(from),
                    to: transition.to,
                });
                if transition.duration <= 0.0 {
                    self.current = transition.to;
                    self.entered = frame;
                } else {
                    self.pending = Some(Pending {
                        from,
                        to: transition.to,
                        start: frame,
                        duration: transition.duration,
                    });
                }
                break;
            }
        }
        for (input, value) in values.clone() {
            if machine
                .transitions
                .iter()
                .any(|transition| transition.input == input)
            {
                self.previous_inputs.insert(input, value);
            }
        }

        let target = self.pending.map_or(self.current, |pending| pending.to);
        let source = self.pending.map_or(self.current, |pending| pending.from);
        let progress = match self.pending {
            Some(pending) => {
                let base = ((frame - pending.start) / pending.duration).clamp(0.0, 1.0);
                transition_ease(base)
            }
            None => 1.0,
        };
        let from_values = state_values(rig, machine, source, frame, self.entered);
        let to_values = state_values(rig, machine, target, frame, self.entered);
        for (property, def) in rig.properties.iter().enumerate() {
            let Some(to) = to_values.get(&property) else {
                continue;
            };
            let from = from_values
                .get(&property)
                .cloned()
                .or_else(|| values.get(&property).cloned())
                .unwrap_or_else(|| def.default.clone());
            let blended = if def.kind == PropKind::Float && def.angle {
                match (&from, to) {
                    (PropValue::Number(a), PropValue::Number(b)) => {
                        PropValue::Number(PropValue::blend_angle(*a, *b, progress))
                    }
                    _ => from.blend(to, progress, def.kind),
                }
            } else {
                from.blend(to, progress, def.kind)
            };
            values.insert(property, def.clamped(blended));
        }
    }
}

/// The blend curve of a machine transition: smoothstep, which reads well for
/// short UI transitions and is deterministic.
fn transition_ease(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// The values a state contributes at a frame.
fn state_values(
    rig: &Rig,
    machine: &Machine,
    state: usize,
    frame: f64,
    entered: f64,
) -> BTreeMap<usize, PropValue> {
    let mut values = BTreeMap::new();
    let Some(state) = machine.states.get(state) else {
        return values;
    };
    if let Some(pose) = state.pose.and_then(|pose| rig.poses.get(pose)) {
        for (property, value) in &pose.values {
            values.insert(*property, value.clone());
        }
    }
    if let Some(clip) = state.clip.and_then(|clip| rig.clips.get(clip)) {
        let mut time = (frame - entered).max(0.0) * state.speed.max(0.0);
        if state.looping {
            let length = clip.length().max(f64::EPSILON);
            time = time.rem_euclid(length);
        }
        for (property, def) in rig.properties.iter().enumerate() {
            if let Some(value) = clip.sample(property, time, def.kind, def.angle) {
                values.insert(property, value);
            }
        }
    }
    values
}
