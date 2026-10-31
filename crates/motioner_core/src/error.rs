//! Typed errors for document construction.

use std::fmt;

use crate::node::{NodeId, Property};

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
        }
    }
}

impl std::error::Error for CoreError {}
