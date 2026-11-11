# Domain: motioner_tools

## Purpose

The D-0043 agent tool catalogue and its executors (D-0049). One list of tool
descriptors feeds `motioner tools --schema`; the executors orchestrate the
renderer, the analysis crate and the project I/O behind a `Host` trait.

## Key components

- `catalogue::TOOLS` — every tool with name, description, cost class, family,
  parameter schema, result schema and example:
  `crates/motioner_tools/src/catalogue.rs`.
- `to_json` / `catalogue_json` — descriptors as JSON Schema, merged by
  `motioner_app::tools::schema` into the single published document:
  `crates/motioner_app/src/tools.rs`.
- `Host` — what an executor may read or ask the host to do; mutating methods
  default to a typed `requires_harness` error:
  `crates/motioner_tools/src/host.rs`.
- `execute` — one dispatcher arm per tool; render tools compose
  `motioner_render::tools` images and write them to `--out`:
  `crates/motioner_tools/src/exec.rs`.
- `Runtime` — the set-of-marks map (`#n` from the last overlay) shared by
  calls in one session.
- `ToolError` — typed failures (`unknown_tool`, `invalid_args`,
  `requires_harness`, `unsupported`, `host`):
  `crates/motioner_tools/src/error.rs`.

## Host implementation

`motioner_app::toolhost::AppHost` implements `Host` with the same `Editor`
the window and `motioner edit` use, so `run_commands`, `apply_code_edit`,
`undo`, `redo`, `checkpoint` and `restore` share write-through, undo and
checkpoints. `motioner tool` (`crates/motioner_app/src/toolcmd.rs`) runs one
call or a replayable JSONL batch.

## Boundaries

May depend on core, DSL, eval, render, I/O, analysis and `serde_json`
(D-0050). It never depends on the UI or the application; the UI adapter lives
in `motioner_app`.

## Tests

`crates/motioner_tools/src/catalogue.rs` unit tests (completeness, unique
names, user tools) and `crates/motioner_tools/tests/contracts.rs` (result
schemas, dispatch coverage, typed harness errors, GPU render tools).
