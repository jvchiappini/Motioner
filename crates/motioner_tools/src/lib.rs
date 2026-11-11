//! The D-0043 agent tool catalogue and its executors (D-0049).
//!
//! There is exactly one catalogue: the descriptors here are what
//! `motioner tools --schema` prints and what the in-app harness (prompt 14)
//! converts for each provider. Commands are not one tool each — `run_commands`
//! carries D-0024 command text — so the published list stays small.
//!
//! Executors orchestrate the other crates (render, analysis, I/O) and mutate
//! a project only through [`Host`], the trait `motioner_app` implements with
//! `Editor`. That keeps write-through, checkpoints, undo and traces identical
//! between the window, the CLI and the agent.

pub mod catalogue;
pub mod error;
pub mod exec;
pub mod host;

pub use catalogue::{catalogue_json, result_schema, spec, to_json, Cost, Param, ToolSpec, TOOLS};
pub use error::ToolError;
pub use exec::{execute, Runtime, ToolOutput};
pub use host::Host;
