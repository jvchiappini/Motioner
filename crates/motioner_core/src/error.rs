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
}

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
        }
    }
}

impl std::error::Error for CoreError {}
