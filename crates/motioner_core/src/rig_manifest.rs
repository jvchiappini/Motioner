//! The rig manifest: what the compiler knows about every rig instance.
//!
//! A rig is authored as code, but editors, agents and the future player need
//! a stable, typed description without reading the program. The compiler
//! builds one entry per rig instance and stores it on the [`crate::Document`]
//! (D-0056). Fields are ordered as authored, so two compiles of the same
//! program produce the same manifest, and `motioner query` can print it
//! deterministically.
//!
//! The manifest is descriptive only: evaluation still comes from the baked
//! keys. It is not serialised into the MDF file; it is rebuilt on every
//! compile.

use crate::color::Color;
use crate::math::Vec2;

/// Schema version of [`RigManifest`], bumped only on a breaking change.
pub const RIG_MANIFEST_VERSION: u32 = 1;

/// Every rig instance in one document.
#[derive(Debug, Clone, PartialEq)]
pub struct RigManifest {
    /// Schema version; see [`RIG_MANIFEST_VERSION`].
    pub version: u32,
    /// Instances in creation order.
    pub rigs: Vec<RigInstanceManifest>,
}

impl Default for RigManifest {
    fn default() -> Self {
        Self {
            version: RIG_MANIFEST_VERSION,
            rigs: Vec::new(),
        }
    }
}

/// One rig instance.
#[derive(Debug, Clone, PartialEq)]
pub struct RigInstanceManifest {
    /// Stable document identity of the rig root.
    pub id: String,
    /// Display name of the rig root.
    pub name: String,
    /// Addressable parts in symbol paint order.
    pub parts: Vec<RigPartManifest>,
    /// Declared properties in declaration order.
    pub properties: Vec<RigPropertyManifest>,
    /// Named poses.
    pub poses: Vec<RigPoseManifest>,
    /// Pose applications in start order.
    pub pose_tracks: Vec<RigPoseTrackManifest>,
    /// Named clips.
    pub clips: Vec<RigClipManifest>,
    /// Clip playbacks in start order.
    pub plays: Vec<RigPlayManifest>,
    /// Procedural layers (today deterministic blink schedules).
    pub layers: Vec<RigLayerManifest>,
    /// Aim constraints.
    pub aims: Vec<RigAimManifest>,
    /// The state machine, when the rig has one.
    pub machine: Option<RigMachineManifest>,
}

/// One addressable part: the symbol id and the document node identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigPartManifest {
    /// Source part id (`body`, `mouth-line`, ...).
    pub id: String,
    /// Stable document identity used by query and edit.
    pub node: String,
}

/// The value type of a property or input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigValueKind {
    /// Continuous number.
    Float,
    /// Whole number.
    Int,
    /// Boolean.
    Bool,
    /// One of a fixed list of names.
    Enum,
    /// sRGB colour with straight alpha.
    Color,
    /// Two-component vector.
    Vec2,
}

impl RigValueKind {
    /// Stable lower-case name used in JSON and the tool schema.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Enum => "enum",
            Self::Color => "color",
            Self::Vec2 => "vec2",
        }
    }
}

/// One typed rig value.
#[derive(Debug, Clone, PartialEq)]
pub enum RigValue {
    /// Float, int and angle values.
    Number(f64),
    /// Boolean values.
    Bool(bool),
    /// Enum values (the variant name).
    Text(String),
    /// Vec2 values.
    Vec2(Vec2),
    /// Colour values.
    Color(Color),
}

/// The side a mirrored property belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigMirrorSide {
    /// The first side of the pair (usually the left one).
    Left,
    /// The second side of the pair (usually the right one).
    Right,
}

impl RigMirrorSide {
    /// Stable lower-case name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// A property's membership in a mirror group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigMirrorManifest {
    /// Group name (`lid`).
    pub group: String,
    /// Which side this property is.
    pub side: RigMirrorSide,
    /// Whether this side is negated by the mirror.
    pub flip: bool,
}

/// One declared property.
#[derive(Debug, Clone, PartialEq)]
pub struct RigPropertyManifest {
    /// Stable name used by keys, poses and drivers.
    pub name: String,
    /// Value type.
    pub kind: RigValueKind,
    /// Whether the number is an angle in degrees.
    pub angle: bool,
    /// Default value.
    pub default: RigValue,
    /// Inclusive lower bound, when the property has a range.
    pub min: Option<f64>,
    /// Inclusive upper bound, when the property has a range.
    pub max: Option<f64>,
    /// Inspector step.
    pub step: f64,
    /// Enum variants, in declaration order.
    pub values: Vec<String>,
    /// Mirror group name, when the property belongs to one.
    pub group: Option<String>,
    /// Mirror membership details, when the property belongs to one.
    pub mirror: Option<RigMirrorManifest>,
}

/// One property bound to a value inside a pose or layer.
#[derive(Debug, Clone, PartialEq)]
pub struct RigPropertyValue {
    /// Property name.
    pub property: String,
    /// Bound value.
    pub value: RigValue,
}

/// One named pose.
#[derive(Debug, Clone, PartialEq)]
pub struct RigPoseManifest {
    /// Pose name.
    pub name: String,
    /// Whether omitted properties return to their defaults.
    pub reset: bool,
    /// Set properties in property order.
    pub values: Vec<RigPropertyValue>,
}

/// One pose application.
#[derive(Debug, Clone, PartialEq)]
pub struct RigPoseTrackManifest {
    /// Pose name.
    pub pose: String,
    /// Start frame.
    pub start: f64,
    /// Blend duration in frames.
    pub duration: f64,
    /// Ease name.
    pub ease: String,
}

/// One key of a clip.
#[derive(Debug, Clone, PartialEq)]
pub struct RigClipKeyManifest {
    /// Property name.
    pub property: String,
    /// Frame relative to the clip start.
    pub frame: f64,
    /// Value at the key.
    pub value: RigValue,
    /// Ease of the arriving segment.
    pub ease: String,
}

/// One named clip.
#[derive(Debug, Clone, PartialEq)]
pub struct RigClipManifest {
    /// Clip name.
    pub name: String,
    /// Length in frames.
    pub length: f64,
    /// Keys in authoring order.
    pub keys: Vec<RigClipKeyManifest>,
}

/// One clip playback.
#[derive(Debug, Clone, PartialEq)]
pub struct RigPlayManifest {
    /// Clip name.
    pub clip: String,
    /// Start frame.
    pub start: f64,
    /// Number of repetitions.
    pub loops: u32,
    /// Speed multiplier.
    pub speed: f64,
}

/// One procedural layer.
#[derive(Debug, Clone, PartialEq)]
pub struct RigLayerManifest {
    /// Layer kind (`blink`).
    pub kind: String,
    /// First frame.
    pub start: f64,
    /// Nominal period in frames.
    pub period: f64,
    /// Maximum deterministic jitter in frames.
    pub jitter: f64,
    /// Hold length in frames.
    pub duration: f64,
    /// Deterministic seed.
    pub seed: u64,
    /// Properties written while the layer is active.
    pub values: Vec<RigPropertyValue>,
}

/// One aim constraint.
#[derive(Debug, Clone, PartialEq)]
pub struct RigAimManifest {
    /// Part id.
    pub part: String,
    /// Vec2 property name.
    pub property: String,
    /// Maximum offset in pixels.
    pub radius: f64,
}

/// The state machine description.
#[derive(Debug, Clone, PartialEq)]
pub struct RigMachineManifest {
    /// Initial state name.
    pub default: String,
    /// Inputs used by at least one transition, in first-use order.
    pub inputs: Vec<RigInputManifest>,
    /// States in declaration order.
    pub states: Vec<RigStateManifest>,
    /// Transitions in evaluation order.
    pub transitions: Vec<RigTransitionManifest>,
    /// Events emitted when a transition fires.
    pub events: Vec<RigEventManifest>,
}

/// One machine input.
#[derive(Debug, Clone, PartialEq)]
pub struct RigInputManifest {
    /// Property name.
    pub name: String,
    /// Value type.
    pub kind: RigValueKind,
    /// Enum variants, when the input is an enum.
    pub values: Vec<String>,
}

/// One machine state.
#[derive(Debug, Clone, PartialEq)]
pub struct RigStateManifest {
    /// State name.
    pub name: String,
    /// Pose name, when the state holds a pose.
    pub pose: Option<String>,
    /// Clip name, when the state holds a clip.
    pub clip: Option<String>,
    /// Clip playback speed.
    pub speed: f64,
    /// Whether the clip loops.
    pub looping: bool,
}

/// One transition.
#[derive(Debug, Clone, PartialEq)]
pub struct RigTransitionManifest {
    /// Source state name, or `None` for an any-state transition.
    pub from: Option<String>,
    /// Target state name.
    pub to: String,
    /// Input property name.
    pub input: String,
    /// Comparison name (`equals`, `not_equals`, `greater`, `less`, `trigger`).
    pub compare: String,
    /// Right-hand value.
    pub value: RigValue,
    /// Blend length in frames.
    pub duration: f64,
}

/// One event emitted when a transition fires.
#[derive(Debug, Clone, PartialEq)]
pub struct RigEventManifest {
    /// State left, or `None` when there was no state.
    pub from: Option<String>,
    /// State entered.
    pub to: String,
}
