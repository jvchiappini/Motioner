//! Typed tool failures.

use std::fmt;

/// Why a tool call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// No tool has this name.
    Unknown(String),
    /// The arguments do not match the parameter schema.
    InvalidArgs(String),
    /// The tool is declared here but only the prompt 14 harness executes it
    /// (the user tools and the editor screenshot).
    RequiresHarness(String),
    /// The build does not implement the tool yet.
    Unsupported(String),
    /// The host (editor, project or renderer) reported a failure.
    Host(String),
}

impl ToolError {
    /// Stable lower-case kind used in JSON errors.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Unknown(_) => "unknown_tool",
            Self::InvalidArgs(_) => "invalid_args",
            Self::RequiresHarness(_) => "requires_harness",
            Self::Unsupported(_) => "unsupported",
            Self::Host(_) => "host",
        }
    }

    /// The human message.
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Unknown(message)
            | Self::InvalidArgs(message)
            | Self::RequiresHarness(message)
            | Self::Unsupported(message)
            | Self::Host(message) => message,
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for ToolError {}

/// Wraps a host error with the tool name that raised it.
#[must_use]
pub fn host_error(tool: &str, message: impl fmt::Display) -> ToolError {
    ToolError::Host(format!("{tool}: {message}"))
}
