//! Rig definitions and the headless per-frame evaluator (D-0052, D-0055).
//!
//! A rig is pure data: named parts with base transforms, typed custom
//! properties, property keyframes, expression drivers, poses, clips, layers
//! and an optional state machine. [`bake`] evaluates it frame by frame
//! and returns the writes for every part; the editor bakes those writes into
//! ordinary node keys, and a future player uses the same function.
//!
//! Determinism is a hard contract: no clocks, no I/O, no hash-map iteration
//! order (maps are [`BTreeMap`]s) and no floating-point reassociation between
//! runs. Backwards scrubbing is `bake` again: there is no hidden state to
//! rewind.
//!
//! ```
//! use motioner_core::{Ease, Vec2};
//! use motioner_rig::{bake, Key, PartBase, PropertyDef, PropKind, PropValue, Rig};
//!
//! let mut rig = Rig::new("demo");
//! rig.parts.push(PartBase::at("body", Vec2::new(10.0, 10.0)));
//! rig.add_property(PropertyDef::number("size", 1.0, 0.5, 2.0));
//! rig.channels[0].keys.push(Key::new(0.0, PropValue::Number(1.0), Ease::Linear));
//! rig.channels[0].keys.push(Key::new(30.0, PropValue::Number(2.0), Ease::Linear));
//! let frames = bake(&rig, 0.0, 30.0).expect("bake");
//! assert_eq!(frames.len(), 31);
//! assert_eq!(rig.properties[0].kind, PropKind::Float);
//! ```
//!
//! The example above builds a rig by hand; the DSL does the same through
//! `rig()` and is the normal entry point.

mod driver;
mod eval;
mod machine;

pub use driver::{Driver, DriverError, Expr, Field, Write};
pub use eval::{bake, evaluate, FrameOutput, PartOutput, RigError, MAX_BAKE_FRAMES};
pub use machine::{Compare, Machine, MachineEvent, State, Transition};

/// Maximum writes one driver may declare, for the DSL compiler and tools.
#[must_use]
pub const fn driver_max_writes() -> usize {
    driver::MAX_WRITES
}

use std::collections::BTreeMap;

use motioner_core::{Color, Ease, Vec2};

/// The type of a custom property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropKind {
    /// A continuous number.
    Float,
    /// A whole number; sampled values are rounded.
    Int,
    /// A boolean; blends step at the end of a transition.
    Bool,
    /// One of a fixed list of names; steps at the end.
    Enum,
    /// A colour; blends in linear light.
    Color,
    /// A two-component vector.
    Vec2,
}

/// A property value.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    /// Float, int and angle values.
    Number(f64),
    /// Boolean values.
    Bool(bool),
    /// Enum values (the variant name).
    Text(String),
    /// Vec2 values.
    Vector(Vec2),
    /// Colour values.
    Color(Color),
}

impl PropValue {
    /// The numeric value, when this is a number.
    #[must_use]
    pub fn number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// The vector value, when this is a vector.
    #[must_use]
    pub fn vector(&self) -> Option<Vec2> {
        match self {
            Self::Vector(value) => Some(*value),
            _ => None,
        }
    }

    /// Blends two values with `t` in `0..=1` following the property type.
    #[must_use]
    pub fn blend(&self, other: &Self, t: f64, kind: PropKind) -> Self {
        let t = t.clamp(0.0, 1.0);
        match (self, other, kind) {
            (Self::Number(a), Self::Number(b), PropKind::Float | PropKind::Int) => {
                let value = a + (b - a) * t;
                Self::Number(if kind == PropKind::Int {
                    value.round()
                } else {
                    value
                })
            }
            (Self::Number(a), Self::Number(b), PropKind::Bool) => {
                Self::Number(if t < 1.0 { *a } else { *b })
            }
            (Self::Vector(a), Self::Vector(b), _) => Self::Vector(*a + (*b - *a) * t),
            (Self::Color(a), Self::Color(b), _) => Self::Color(Color::lerp(*a, *b, t)),
            (Self::Bool(a), Self::Bool(b), _) => Self::Bool(if t < 1.0 { *a } else { *b }),
            (Self::Text(a), Self::Text(b), _) => {
                Self::Text(if t < 1.0 { a.clone() } else { b.clone() })
            }
            (a, _, _) => a.clone(),
        }
    }

    /// Samples an angle along the shortest arc, in degrees.
    #[must_use]
    pub fn blend_angle(a: f64, b: f64, t: f64) -> f64 {
        let mut delta = (b - a) % 360.0;
        if delta > 180.0 {
            delta -= 360.0;
        } else if delta < -180.0 {
            delta += 360.0;
        }
        a + delta * t
    }
}

/// A property declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDef {
    /// Stable name used by keys, poses, drivers and tools.
    pub name: String,
    /// Value type.
    pub kind: PropKind,
    /// Value when no key or pose sets one.
    pub default: PropValue,
    /// Inclusive lower bound, when the property has a range.
    pub min: Option<f64>,
    /// Inclusive upper bound, when the property has a range.
    pub max: Option<f64>,
    /// Inspector step.
    pub step: f64,
    /// Enum variants, for [`PropKind::Enum`].
    pub values: Vec<String>,
    /// Whether the property is an angle in degrees (shortest-arc blending).
    pub angle: bool,
}

impl PropertyDef {
    /// A float property with a range and the default 0.01 step.
    #[must_use]
    pub fn number(name: impl Into<String>, default: f64, min: f64, max: f64) -> Self {
        Self {
            name: name.into(),
            kind: PropKind::Float,
            default: PropValue::Number(default),
            min: Some(min),
            max: Some(max),
            step: 0.01,
            values: Vec::new(),
            angle: false,
        }
    }

    /// A property with an explicit type and default.
    #[must_use]
    pub fn typed(name: impl Into<String>, kind: PropKind, default: PropValue) -> Self {
        Self {
            name: name.into(),
            kind,
            default,
            min: None,
            max: None,
            step: 1.0,
            values: Vec::new(),
            angle: false,
        }
    }

    /// Clamps a numeric value into the declared range.
    #[must_use]
    pub fn clamped(&self, value: PropValue) -> PropValue {
        match value {
            PropValue::Number(number) => {
                let mut number = number;
                if let Some(min) = self.min {
                    number = number.max(min);
                }
                if let Some(max) = self.max {
                    number = number.min(max);
                }
                PropValue::Number(if self.kind == PropKind::Int {
                    number.round()
                } else {
                    number
                })
            }
            other => other,
        }
    }
}

/// One property keyframe.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    /// Frame time in composition frames (may be fractional while posing).
    pub frame: f64,
    /// Value at the key.
    pub value: PropValue,
    /// Ease of the segment arriving at this key.
    pub ease: Ease,
}

impl Key {
    /// A keyframe.
    #[must_use]
    pub fn new(frame: f64, value: PropValue, ease: Ease) -> Self {
        Self { frame, value, ease }
    }
}

/// The keys of one property on one rig instance.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Channel {
    /// Sorted, strictly increasing frames.
    pub keys: Vec<Key>,
}

impl Channel {
    /// Samples the channel; a channel without keys returns `None`.
    ///
    /// # Panics
    ///
    /// Panics only if the key list is emptied between two internal checks,
    /// which cannot happen through the public API.
    #[must_use]
    pub fn sample(&self, frame: f64, kind: PropKind, angle: bool) -> Option<PropValue> {
        if self.keys.is_empty() {
            return None;
        }
        let first = &self.keys[0];
        if frame <= first.frame {
            return Some(first.value.clone());
        }
        let last = self.keys.last().expect("non-empty");
        if frame >= last.frame {
            return Some(last.value.clone());
        }
        let index = self
            .keys
            .partition_point(|key| key.frame <= frame)
            .saturating_sub(1);
        let from = &self.keys[index];
        let to = &self.keys[(index + 1).min(self.keys.len() - 1)];
        let span = to.frame - from.frame;
        let base = if span <= f64::EPSILON {
            1.0
        } else {
            (frame - from.frame) / span
        };
        let t = motioner_eval::ease(to.ease, base.clamp(0.0, 1.0));
        match (&from.value, &to.value, angle) {
            (PropValue::Number(a), PropValue::Number(b), true) => {
                Some(PropValue::Number(PropValue::blend_angle(*a, *b, t)))
            }
            _ => Some(from.value.blend(&to.value, t, kind)),
        }
    }
}

/// A mirrored property group: one name edits both sides.
#[derive(Debug, Clone, PartialEq)]
pub struct Mirror {
    /// Group name (`lid`).
    pub name: String,
    /// The two concrete property names.
    pub sides: [String; 2],
    /// Whether the right side is the negation of the left (the `MIR` table).
    pub flip: bool,
}

/// The base transform of one part.
#[derive(Debug, Clone, PartialEq)]
pub struct PartBase {
    /// Symbol id (or a stable generated name).
    pub name: String,
    /// Node position that reproduces the imported geometry.
    pub position: Vec2,
    /// Node rotation in degrees.
    pub rotation: f64,
    /// Node scale.
    pub scale: Vec2,
    /// Node anchor, in local coordinates.
    pub anchor: Vec2,
    /// Node opacity.
    pub opacity: f64,
    /// Optional base fill/stroke the rig may override.
    pub fill: Option<Color>,
    /// Optional base stroke.
    pub stroke: Option<Color>,
    /// Base stroke width.
    pub stroke_width: f64,
}

impl PartBase {
    /// A part with an identity transform and the identity anchored at `anchor`.
    #[must_use]
    pub fn at(name: impl Into<String>, anchor: Vec2) -> Self {
        Self {
            name: name.into(),
            position: anchor,
            rotation: 0.0,
            scale: Vec2::ONE,
            anchor,
            opacity: 1.0,
            fill: None,
            stroke: None,
            stroke_width: 0.0,
        }
    }
}

/// A partial property set applied over a duration.
#[derive(Debug, Clone, PartialEq)]
pub struct Pose {
    /// Pose name.
    pub name: String,
    /// Property index and target value.
    pub values: BTreeMap<usize, PropValue>,
    /// Whether properties the pose omits return to their declared defaults
    /// (the controller's `expand()` presets behave this way).
    pub reset: bool,
}

/// One application of a pose at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct PoseTrack {
    /// Index into [`Rig::poses`].
    pub pose: usize,
    /// Start frame.
    pub start: f64,
    /// Duration in frames.
    pub duration: f64,
    /// Ease of the blend.
    pub ease: Ease,
}

/// One key of a clip, relative to the clip start.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipKey {
    /// Property index.
    pub property: usize,
    /// Frame relative to the clip start.
    pub frame: f64,
    /// Value at the key.
    pub value: PropValue,
    /// Ease of the arriving segment.
    pub ease: Ease,
}

/// A reusable keyframed sequence on rig properties.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// Clip name.
    pub name: String,
    /// Keys in authoring order; sampling sorts by frame.
    pub keys: Vec<ClipKey>,
    /// Clip length in frames (`0` derives it from the last key).
    pub length: f64,
}

impl Clip {
    /// The clip length, from the last key when not given.
    #[must_use]
    pub fn length(&self) -> f64 {
        if self.length > 0.0 {
            return self.length;
        }
        self.keys
            .iter()
            .map(|key| key.frame)
            .fold(0.0_f64, f64::max)
    }

    /// Samples one property of the clip at `time` (frames since the start).
    #[must_use]
    pub fn sample(
        &self,
        property: usize,
        time: f64,
        kind: PropKind,
        angle: bool,
    ) -> Option<PropValue> {
        let mut keys: Vec<&ClipKey> = self
            .keys
            .iter()
            .filter(|key| key.property == property)
            .collect();
        keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        if keys.is_empty() {
            return None;
        }
        // Before a clip's first key the property keeps its base value; a clip
        // only overrides the properties it actually keys from that point on.
        if time < keys[0].frame {
            return None;
        }
        let last = keys[keys.len() - 1];
        if time >= last.frame {
            return Some(last.value.clone());
        }
        let index = keys
            .partition_point(|key| key.frame <= time)
            .saturating_sub(1);
        let from = keys[index];
        let to = keys[(index + 1).min(keys.len() - 1)];
        let span = to.frame - from.frame;
        let base = if span <= f64::EPSILON {
            1.0
        } else {
            (time - from.frame) / span
        };
        let t = motioner_eval::ease(to.ease, base.clamp(0.0, 1.0));
        if angle {
            if let (PropValue::Number(a), PropValue::Number(b)) = (&from.value, &to.value) {
                return Some(PropValue::Number(PropValue::blend_angle(*a, *b, t)));
            }
        }
        Some(from.value.blend(&to.value, t, kind))
    }
}

/// One playback of a clip.
#[derive(Debug, Clone, PartialEq)]
pub struct Play {
    /// Index into [`Rig::clips`].
    pub clip: usize,
    /// Start frame.
    pub start: f64,
    /// Number of repetitions (`1` plays once).
    pub loops: u32,
    /// Speed multiplier.
    pub speed: f64,
}

/// An aim constraint: a part follows a vec2 property with a radius clamp.
#[derive(Debug, Clone, PartialEq)]
pub struct Aim {
    /// Part index.
    pub part: usize,
    /// Vec2 property index.
    pub property: usize,
    /// Maximum offset in pixels.
    pub radius: f64,
}

/// A deterministic blink schedule layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Blink {
    /// First blink frame.
    pub start: f64,
    /// Nominal period in frames.
    pub period: f64,
    /// Maximum deterministic jitter in frames.
    pub jitter: f64,
    /// Closed-eye hold in frames.
    pub duration: f64,
    /// Seed for the jitter sequence.
    pub seed: u64,
    /// Property and closed value written while blinking.
    pub props: Vec<(usize, PropValue)>,
}

/// A rig instance plus its definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct Rig {
    /// Display name.
    pub name: String,
    /// Parts in build order; part indices are positions in this list.
    pub parts: Vec<PartBase>,
    /// Property declarations; property indices are positions in this list.
    pub properties: Vec<PropertyDef>,
    /// One channel per property, same order.
    pub channels: Vec<Channel>,
    /// Mirrored groups.
    pub mirrors: Vec<Mirror>,
    /// Expression drivers.
    pub drivers: Vec<driver::Driver>,
    /// Named poses.
    pub poses: Vec<Pose>,
    /// Pose applications.
    pub tracks: Vec<PoseTrack>,
    /// Named clips.
    pub clips: Vec<Clip>,
    /// Clip playbacks.
    pub plays: Vec<Play>,
    /// Aim constraints.
    pub aims: Vec<Aim>,
    /// Blink layers.
    pub blinks: Vec<Blink>,
    /// The optional state machine.
    pub machine: Option<Machine>,
}

impl Rig {
    /// An empty rig with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parts: Vec::new(),
            properties: Vec::new(),
            channels: Vec::new(),
            mirrors: Vec::new(),
            drivers: Vec::new(),
            poses: Vec::new(),
            tracks: Vec::new(),
            clips: Vec::new(),
            plays: Vec::new(),
            aims: Vec::new(),
            blinks: Vec::new(),
            machine: None,
        }
    }

    /// Adds a property and its empty channel, returning the property index.
    pub fn add_property(&mut self, def: PropertyDef) -> usize {
        let index = self.properties.len();
        self.channels.push(Channel::default());
        self.properties.push(def);
        index
    }

    /// Adds a part, returning its index.
    pub fn add_part(&mut self, part: PartBase) -> usize {
        self.parts.push(part);
        self.parts.len() - 1
    }

    /// Looks up a property by name.
    #[must_use]
    pub fn property(&self, name: &str) -> Option<usize> {
        self.properties
            .iter()
            .position(|property| property.name == name)
    }

    /// Looks up a part by symbol id.
    #[must_use]
    pub fn part(&self, name: &str) -> Option<usize> {
        self.parts.iter().position(|part| part.name == name)
    }

    /// Looks up a pose by name.
    #[must_use]
    pub fn pose(&self, name: &str) -> Option<usize> {
        self.poses.iter().position(|pose| pose.name == name)
    }

    /// Looks up a clip by name.
    #[must_use]
    pub fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|clip| clip.name == name)
    }

    /// The static value of a property before any pose, clip or machine.
    #[must_use]
    pub fn base_value(&self, property: usize) -> PropValue {
        let def = &self.properties[property];
        self.channels
            .get(property)
            .and_then(|channel| channel.sample(0.0, def.kind, def.angle))
            .unwrap_or_else(|| def.default.clone())
    }

    /// Samples every property at `frame`, applying tracks, clips and layers.
    ///
    /// This is the stateless part of evaluation; the state machine adds the
    /// sequential state on top in [`bake`].
    #[must_use]
    pub fn sample_properties(&self, frame: f64) -> BTreeMap<usize, PropValue> {
        let mut values = BTreeMap::new();
        for (index, def) in self.properties.iter().enumerate() {
            let value = self
                .channels
                .get(index)
                .and_then(|channel| channel.sample(frame, def.kind, def.angle))
                .unwrap_or_else(|| def.default.clone());
            values.insert(index, value);
        }
        // Tracks apply in start order; each blend starts from the value the
        // previous tracks produced at its own start, so pose B follows pose A
        // instead of jumping back to the keyed base.
        let mut order: Vec<&PoseTrack> = self.tracks.iter().collect();
        order.sort_by(|a, b| a.start.total_cmp(&b.start));
        for (index, track) in order.iter().enumerate() {
            let Some(pose) = self.poses.get(track.pose) else {
                continue;
            };
            if frame < track.start {
                break;
            }
            let progress = if track.duration <= 0.0 {
                1.0
            } else {
                ((frame - track.start) / track.duration).clamp(0.0, 1.0)
            };
            let t = motioner_eval::ease(track.ease, progress);
            let covered: Vec<usize> = if pose.reset {
                (0..self.properties.len()).collect()
            } else {
                pose.values.keys().copied().collect()
            };
            for property in covered {
                if property >= self.properties.len() {
                    continue;
                }
                let def = &self.properties[property];
                let target = pose
                    .values
                    .get(&property)
                    .cloned()
                    .unwrap_or_else(|| def.default.clone());
                let from = self.pose_value_at(property, track.start, &order[..index]);
                let blended = if def.angle {
                    if let (PropValue::Number(a), PropValue::Number(b)) = (&from, &target) {
                        PropValue::Number(PropValue::blend_angle(*a, *b, t))
                    } else {
                        from.blend(&target, t, def.kind)
                    }
                } else {
                    from.blend(&target, t, def.kind)
                };
                values.insert(property, def.clamped(blended));
            }
        }
        for play in &self.plays {
            let Some(clip) = self.clips.get(play.clip) else {
                continue;
            };
            if frame < play.start {
                continue;
            }
            let length = clip.length().max(f64::EPSILON);
            let local = (frame - play.start) * play.speed.max(0.0);
            let time = if play.loops == 0 {
                local.rem_euclid(length)
            } else {
                let total = length * f64::from(play.loops);
                if local >= total {
                    total
                } else {
                    local.rem_euclid(length)
                }
            };
            for (property, def) in self.properties.iter().enumerate() {
                if let Some(value) = clip.sample(property, time, def.kind, def.angle) {
                    values.insert(property, def.clamped(value));
                }
            }
        }
        values
    }

    /// Applies procedural layers (blinks) on top of the machine.
    /// The value a property has at `frame` after the keyed base and the given
    /// earlier tracks, used as the start of the next pose blend.
    fn pose_value_at(&self, property: usize, frame: f64, tracks: &[&PoseTrack]) -> PropValue {
        let def = &self.properties[property];
        let mut value = self
            .channels
            .get(property)
            .and_then(|channel| channel.sample(frame, def.kind, def.angle))
            .unwrap_or_else(|| def.default.clone());
        for track in tracks {
            if frame < track.start {
                continue;
            }
            let Some(pose) = self.poses.get(track.pose) else {
                continue;
            };
            let target = if pose.reset {
                pose.values
                    .get(&property)
                    .cloned()
                    .unwrap_or_else(|| def.default.clone())
            } else {
                match pose.values.get(&property) {
                    Some(value) => value.clone(),
                    None => continue,
                }
            };
            let progress = if track.duration <= 0.0 {
                1.0
            } else {
                ((frame - track.start) / track.duration).clamp(0.0, 1.0)
            };
            let t = motioner_eval::ease(track.ease, progress);
            value = if def.angle {
                match (&value, &target) {
                    (PropValue::Number(a), PropValue::Number(b)) => {
                        PropValue::Number(PropValue::blend_angle(*a, *b, t))
                    }
                    _ => value.blend(&target, t, def.kind),
                }
            } else {
                value.blend(&target, t, def.kind)
            };
        }
        def.clamped(value)
    }

    /// Applies procedural layers (blinks) on top of already resolved values.
    pub fn apply_layers(&self, frame: f64, values: &mut BTreeMap<usize, PropValue>) {
        self.apply_blinks(frame, values);
    }

    fn apply_blinks(&self, frame: f64, values: &mut BTreeMap<usize, PropValue>) {
        for blink in &self.blinks {
            if frame < blink.start || blink.period <= 0.0 {
                continue;
            }
            // Deterministic per-cycle jitter: the cycle index feeds a
            // splitmix step, so the schedule does not need a clock or RNG.
            let cycle = ((frame - blink.start) / blink.period).floor().max(0.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // cycle >= 0
            let offset = jitter(blink.seed, cycle as u64, blink.jitter);
            let cycle_start = blink.start + cycle * blink.period + offset;
            let inside = frame >= cycle_start && frame < cycle_start + blink.duration;
            if inside {
                for (property, closed) in &blink.props {
                    if *property < self.properties.len() {
                        values.insert(*property, closed.clone());
                    }
                }
            }
        }
    }
}

/// Jitter in `-amount..=amount` from a seed and a cycle index.
fn jitter(seed: u64, index: u64, amount: f64) -> f64 {
    let mut state = seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut result = state;
    result ^= result >> 30;
    result = result.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    result ^= result >> 27;
    result = result.wrapping_mul(0x94D0_49BB_1331_11EB);
    result ^= result >> 31;
    #[allow(clippy::cast_precision_loss)] // 53 bits fit an f64 mantissa
    let unit = (result >> 11) as f64 / (1u64 << 53) as f64;
    (unit * 2.0 - 1.0) * amount
}
