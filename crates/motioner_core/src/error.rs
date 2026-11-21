//! Typed errors for document construction.

use std::fmt;

use crate::node::{NodeId, Property};
use crate::time::{Frame, MAX_ABS_FRAME};

/// An invalid operation on the document model.
#[derive(Debug, Clone, PartialEq)]
pub enum CoreError {
    /// A frame rate with a zero term.
    InvalidFrameRate {
        /// Requested numerator.
        numerator: u32,
        /// Requested denominator.
        denominator: u32,
    },
    /// Canvas size outside `1..=MAX_CANVAS_SIZE`.
    InvalidCanvasSize {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },
    /// A composition with no frames.
    InvalidDuration {
        /// Requested duration in frames.
        frames: i64,
    },
    /// A node ID that does not exist in this document.
    UnknownNode(NodeId),
    /// A value of the wrong shape for a property.
    PropertyType {
        /// The property being written.
        property: Property,
        /// What the property accepts.
        expected: &'static str,
    },
    /// A non-finite number written to a property.
    NonFiniteValue {
        /// The property being written.
        property: Property,
    },
    /// A finite number outside the allowed magnitude of a property.
    ValueOutOfRange {
        /// The property being written.
        property: Property,
        /// The rejected value.
        value: f64,
    },
    /// Authored seconds that do not convert to a frame within
    /// `[-MAX_ABS_FRAME, MAX_ABS_FRAME]`.
    TimeOutOfRange {
        /// The authored time in seconds.
        seconds: f64,
    },
    /// A keyframe frame outside `[-MAX_ABS_FRAME, MAX_ABS_FRAME]`.
    FrameOutOfRange {
        /// The rejected frame.
        frame: Frame,
    },
    /// Negative or non-finite geometry, or a zero text size.
    InvalidGeometry {
        /// The offending constructor field.
        field: &'static str,
    },
    /// A geometry key whose point count or closed flag differs from the
    /// channel's fixed topology (D-0058).
    GeometryTopologyMismatch {
        /// Point count expected by the channel.
        expected_points: usize,
        /// Point count in the rejected geometry.
        got_points: usize,
        /// Closed flag expected by the channel.
        expected_closed: bool,
        /// Closed flag in the rejected geometry.
        got_closed: bool,
    },
    /// The document already holds `u32::MAX` nodes.
    TooManyNodes,
    /// A child added to a node that is not a group.
    NotAGroup(NodeId),
    /// Reparenting that would make a node its own ancestor.
    CyclicParent {
        /// The node being moved.
        child: NodeId,
        /// The requested parent.
        parent: NodeId,
    },
    /// An explicit identity that does not match `[A-Za-z_][A-Za-z0-9_.-]*`.
    InvalidIdentity {
        /// The rejected name.
        name: String,
    },
    /// An explicit identity used by two nodes in one document.
    DuplicateId {
        /// The repeated name.
        name: String,
    },
    /// No node has the requested identity.
    UnknownIdentity {
        /// The requested identity.
        id: String,
    },
    /// A content field that the node's kind does not have.
    WrongContentField {
        /// The node kind ("rect", "text", ...).
        kind: &'static str,
        /// The rejected field name.
        field: String,
    },
    /// An axis was given for a scalar property, or a vector write omitted it.
    WrongAxis {
        /// The property written.
        property: &'static str,
        /// The axis suffix, when one was given.
        axis: Option<&'static str>,
    },
    /// A gradient whose geometry or stops are invalid (D-0030).
    InvalidGradient {
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A content value of the wrong shape or out of range.
    InvalidContentValue {
        /// The rejected field name.
        field: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// The node already carries an effect of this kind.
    DuplicateEffect {
        /// The repeated kind name.
        kind: &'static str,
    },
    /// The node carries no effect of this kind.
    MissingEffect {
        /// The requested kind name.
        kind: &'static str,
    },
    /// The node has no effect of this kind.
    InvalidEffectOrder {
        /// The requested position.
        index: usize,
        /// Number of effects in the stack.
        len: usize,
    },
    /// A node was set to mask itself.
    MaskSelfReference {
        /// The node.
        node: NodeId,
    },
    /// The mask source is an ancestor of the masked node, so drawing the mask
    /// would draw the masked node again.
    MaskSourceIsAncestor {
        /// The masked node.
        node: NodeId,
        /// The rejected source.
        source: NodeId,
    },
    /// The mask would close a cycle in the mask graph.
    MaskCycle {
        /// The masked node.
        node: NodeId,
        /// The rejected source.
        source: NodeId,
    },
    /// Motion-blur settings outside `(0, 360]` shutter degrees and
    /// `1..=64` samples.
    InvalidMotionBlur {
        /// Requested shutter in degrees.
        shutter: f64,
        /// Requested sub-frame count.
        samples: u32,
    },
}

#[allow(clippy::too_many_lines)] // one arm per typed error, kept together
impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFrameRate {
                numerator,
                denominator,
            } => write!(f, "invalid frame rate {numerator}/{denominator}"),
            Self::InvalidCanvasSize { width, height } => write!(
                f,
                "invalid canvas size {width}x{height}: each side must be 1..={}",
                crate::MAX_CANVAS_SIZE
            ),
            Self::InvalidDuration { frames } => {
                write!(f, "invalid duration of {frames} frames: must be positive")
            }
            Self::UnknownNode(id) => write!(f, "unknown node {id}"),
            Self::PropertyType { property, expected } => {
                write!(f, "property '{}' expects {expected}", property.name())
            }
            Self::NonFiniteValue { property } => {
                write!(f, "property '{}' must be a finite number", property.name())
            }
            Self::ValueOutOfRange { property, value } => {
                let bounds = crate::bounds::property_bounds(*property);
                write!(
                    f,
                    "property '{}' value {value} is out of range: it must be between {} and {}",
                    property.name(),
                    bounds.min,
                    bounds.max
                )
            }
            Self::TimeOutOfRange { seconds } => write!(
                f,
                "time {seconds} s is out of range: the frame must be between {} and {}",
                -MAX_ABS_FRAME, MAX_ABS_FRAME
            ),
            Self::FrameOutOfRange { frame } => write!(
                f,
                "frame {frame} is out of range: it must be between {} and {}",
                -MAX_ABS_FRAME, MAX_ABS_FRAME
            ),
            Self::InvalidGeometry { field } => {
                write!(f, "'{field}' must be a finite, non-negative number")
            }
            Self::GeometryTopologyMismatch {
                expected_points,
                got_points,
                expected_closed,
                got_closed,
            } => write!(
                f,
                "geometry topology mismatch: expected {expected_points} points ({}), got {got_points} ({})",
                if *expected_closed { "closed" } else { "open" },
                if *got_closed { "closed" } else { "open" }
            ),
            Self::TooManyNodes => write!(f, "the document cannot hold more nodes"),
            Self::NotAGroup(id) => write!(f, "node {id} is not a group and cannot hold children"),
            Self::CyclicParent { child, parent } => {
                write!(
                    f,
                    "cannot add node {child} to {parent}: it would contain itself"
                )
            }
            Self::InvalidIdentity { name } => {
                write!(f, "invalid id {name:?}: ids match [A-Za-z_][A-Za-z0-9_.-]*")
            }
            Self::DuplicateId { name } => write!(f, "the id {name:?} is already used"),
            Self::UnknownIdentity { id } => write!(f, "no node has the identity {id:?}"),
            Self::WrongContentField { kind, field } => {
                write!(f, "a {kind} has no field '{field}'")
            }
            Self::WrongAxis { property, axis } => match axis {
                Some(axis) => write!(f, "property '{property}' has no axis '{axis}'"),
                None => write!(f, "property '{property}' needs an axis ('x' or 'y')"),
            },
            Self::InvalidGradient { reason } => write!(f, "invalid gradient: {reason}"),
            Self::InvalidContentValue { field, reason } => {
                write!(f, "'{field}' is invalid: {reason}")
            }
            Self::DuplicateEffect { kind } => {
                write!(f, "the node already has a '{kind}' effect")
            }
            Self::MissingEffect { kind } => write!(f, "the node has no '{kind}' effect"),
            Self::MaskSelfReference { node } => {
                write!(f, "node {node} cannot use itself as its mask source")
            }
            Self::MaskSourceIsAncestor { node, source } => write!(
                f,
                "node {source} cannot mask {node}: it is an ancestor, so the mask would draw {node} again"
            ),
            Self::MaskCycle { node, source } => write!(
                f,
                "node {source} cannot mask {node}: the mask chain would form a cycle"
            ),
            Self::InvalidEffectOrder { index, len } => write!(
                f,
                "effect position {index} is out of range: the stack has {len} effect(s)"
            ),
            Self::InvalidMotionBlur { shutter, samples } => write!(
                f,
                "invalid motion blur: shutter must be in (0, 360] degrees and samples in 1..=64, got {shutter} and {samples}"
            ),
        }
    }
}

impl std::error::Error for CoreError {}
