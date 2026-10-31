//! Sandbox resource limits (spec section 12).

/// Hard limits for one program execution.
///
/// Execution time is bounded deterministically by `max_statements`
/// instead of a wall-clock timer, so whether a program hits a limit never
/// depends on machine speed. Memory is bounded per value by the string
/// and list caps plus the node cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Maximum statements executed in total.
    pub max_statements: u64,
    /// Maximum nested user-function calls.
    pub max_call_depth: usize,
    /// Maximum scene nodes created.
    pub max_nodes: usize,
    /// Maximum string length in bytes.
    pub max_string_len: usize,
    /// Maximum list length in items.
    pub max_list_len: usize,
}

impl Default for Limits {
    /// The v0 defaults from the specification.
    fn default() -> Self {
        Self {
            max_statements: 10_000_000,
            max_call_depth: 256,
            max_nodes: 1_000_000,
            max_string_len: 16 * 1024 * 1024,
            max_list_len: 1_000_000,
        }
    }
}
