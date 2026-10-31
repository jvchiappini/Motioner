//! Sandbox resource limits (spec section 12).

use crate::host::{HostError, HostResult};

/// Bytes of produced string that cost one execution step.
pub const STRING_BYTES_PER_STEP: usize = 64;

/// Hard limits for one program execution.
///
/// Execution time is bounded deterministically by `max_steps` instead of a
/// wall-clock timer, so whether a program hits a limit never depends on
/// machine speed. One step is charged for each executed statement, each
/// collection item visited, copied or materialized, and each
/// `STRING_BYTES_PER_STEP` bytes of string produced. Memory is bounded
/// per value by the string and list caps plus the node cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Maximum execution steps charged in total.
    pub max_steps: u64,
    /// Maximum nested user-function calls.
    pub max_call_depth: usize,
    /// Maximum scene nodes created.
    pub max_nodes: usize,
    /// Maximum string length in bytes.
    pub max_string_len: usize,
    /// Maximum list length in items.
    pub max_list_len: usize,
    /// Maximum container nesting depth for `str()` and equality.
    ///
    /// A scalar has depth 0 and a list or tuple adds 1. Values nested
    /// deeper than this cannot be displayed or compared; exactly this
    /// depth is allowed. Creation is not capped, so dropping deep values
    /// must not recurse.
    pub max_value_depth: usize,
}

impl Default for Limits {
    /// The v0 defaults from the specification.
    fn default() -> Self {
        Self {
            max_steps: 10_000_000,
            max_call_depth: 256,
            max_nodes: 1_000_000,
            max_string_len: 16 * 1024 * 1024,
            max_list_len: 1_000_000,
            max_value_depth: 100,
        }
    }
}

impl Limits {
    /// Fails when a string would exceed [`Limits::max_string_len`].
    ///
    /// # Errors
    ///
    /// Returns a limit error naming the configured cap.
    pub(crate) fn check_string_len(&self, bytes: usize) -> HostResult<()> {
        if bytes > self.max_string_len {
            return Err(HostError::limit(format!(
                "string length limit of {} bytes exceeded",
                self.max_string_len
            )));
        }
        Ok(())
    }

    /// Fails when a collection would exceed [`Limits::max_list_len`].
    ///
    /// # Errors
    ///
    /// Returns a limit error naming the configured cap.
    pub(crate) fn check_list_len(&self, items: usize) -> HostResult<()> {
        if items > self.max_list_len {
            return Err(HostError::limit(format!(
                "list length limit of {} items exceeded",
                self.max_list_len
            )));
        }
        Ok(())
    }
}

/// The remaining execution steps of one run.
///
/// This is the only place that translates work into steps. Charging uses
/// saturating arithmetic and never panics, so a hostile program can only
/// ever reach a limit error.
pub(crate) struct Budget {
    max_steps: u64,
    remaining: u64,
}

impl Budget {
    /// Creates a full budget from the configured limits.
    pub(crate) fn new(limits: &Limits) -> Self {
        Self {
            max_steps: limits.max_steps,
            remaining: limits.max_steps,
        }
    }

    /// Charges `steps`, failing when the budget cannot cover them.
    ///
    /// # Errors
    ///
    /// Returns a limit error once the charge would exceed
    /// [`Limits::max_steps`].
    pub(crate) fn charge(&mut self, steps: u64) -> HostResult<()> {
        if steps > self.remaining {
            self.remaining = 0;
            return Err(HostError::limit(format!(
                "execution step limit of {} exceeded",
                self.max_steps
            )));
        }
        self.remaining = self.remaining.saturating_sub(steps);
        Ok(())
    }

    /// Charges one step per collection item visited, copied or
    /// materialized.
    ///
    /// # Errors
    ///
    /// Returns a limit error once the charge would exceed
    /// [`Limits::max_steps`].
    pub(crate) fn charge_items(&mut self, items: usize) -> HostResult<()> {
        self.charge(u64::try_from(items).unwrap_or(u64::MAX))
    }

    /// Charges one step per `STRING_BYTES_PER_STEP` bytes of produced
    /// string, rounding up.
    ///
    /// # Errors
    ///
    /// Returns a limit error once the charge would exceed
    /// [`Limits::max_steps`].
    pub(crate) fn charge_string(&mut self, bytes: usize) -> HostResult<()> {
        let steps = bytes.div_ceil(STRING_BYTES_PER_STEP);
        self.charge(u64::try_from(steps).unwrap_or(u64::MAX))
    }
}
