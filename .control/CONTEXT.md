---
actualizado: 2026-10-31
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0017.

## Working model (from 2026-10-31, session 008)

- Code is written by opencode with DeepSeek V4.1 Flash.
- Claude acts as architect and auditor: writes self-contained English
  prompts for opencode, reviews diffs, runs checks, and only makes small
  fixes. Conversation with the user is in Spanish.
- Prompts live in `.control/prompts/opencode/opencode-prompt-NN-*.md`
  (03 is the last one); the old external `motioner-prompts` folder was
  deleted. Work only inside the Motioner, FerrousEngine and GUIMaker
  folders.
- The user wants LONG multi-phase prompts (several verifiable phases,
  subagents on disjoint files, one final report) and a visible result
  (PNG/MP4) as soon as possible.
- Goals include improving FerrousEngine and GUIMaker upstream with small
  patches, then bumping the pin in Motioner with an ADR.

## Current state (branch feat/m1-foundations, pushed to origin)

- Commits after 757c0b5 (`main` untouched): engine pin 61da7f9 (D-0018),
  core 1e515ea, eval 1462319, dsl 43e31ab, example 1f78243, control
  014ddfc, hardening 480191e (core/eval) and e77a524 (dsl).
- Hardening landed: MAX_ABS_FRAME and `bounds` policy in core, non-finite
  skip in eval, MAX_EXPR_DEPTH 4096, acyclic lists with iterative drop,
  max_value_depth, and a deterministic step budget (`Limits::max_steps`,
  `crates/motioner_dsl/src/limits.rs`) charging statements and all bulk
  work. 91 tests; all gates green.
- Uncommitted on purpose: `crates/motioner_render/Cargo.toml` engine deps,
  `anyhow`/`png`/`sha2` workspace deps and the matching `Cargo.lock`. They
  land with the adapter/CLI so `cargo machete crates` stays green.
- T-DSL-0003/0004 done. T-DSL-0005 backlog (minor findings, memory
  budget). T-RENDER-0002 in_progress: adapter, ffmpeg export and CLI.
  T-ARCH-0007 backlog (GUIMaker architecture proposal pending).

## FerrousEngine improvement candidates (upstream patches)

Verified at 02e6d48:
- `OffscreenTarget::read_rgba8` (`engine/ferrous/crates/ferrous_renderer/src/headless.rs:107`)
  allocates a GPU buffer and a Vec per call and panics on map failure.
  Proposed: `read_rgba8_into(&mut Vec<u8>)` with a persistent staging
  buffer, typed errors, and double-buffered async readback.
- Text: `ferrous_font` already has a font parser and MSDF atlas
  (`engine/ferrous/crates/ferrous_font/src/msdf_gen.rs:227`). Pure2D text
  should reuse it upstream instead of a Motioner-side CPU rasterizer
  (supersedes the earlier CPU-coverage plan; avoids duplicate font code).
- Blending is straight `ALPHA_BLENDING` only (`engine/ferrous/crates/ferrous_2d/src/render/pipeline.rs:102`);
  Motioner needs premultiplied alpha, blend modes and offscreen layers for
  group opacity.
- `render_to_view` requires the default `gui` feature; headless 2D pulls
  GUI/winit. `Pure2dHarness::new` returns `Result<_, String>`.
- `static mut` counters in `engine/ferrous/crates/ferrous_ui_core/src/render_collector.rs:13`
  (unsound), SSAO uses `thread_rng` (non-deterministic 3D), many warnings,
  fmt not clean, no test/clippy CI in the engine repo.

## GUIMaker facts (read via GitHub API, not cloned, 2026-10-31)

- GUIMaker (github.com/jvchiappini/GUIMaker, head 9dcbc28, ~1 MB, Rust)
  is a standalone visual designer BINARY that generates Rust UI code; it
  is not a widget library. Widgets live in the engine (`ferrous_gui`,
  `ferrous_ui_core`, `ferrous_layout`, `ferrous_ui_render`).
- It does not build against the current engine: it imports `ferrous_app`,
  which no longer exists in FerrousEngine main (02e6d48). Its manifest
  uses `../crates/...` path deps (expects to sit inside an engine
  checkout), its docs describe modules (`model.rs`, `codegen.rs`) that
  the code no longer has, and its README says MIT with no LICENSE file.
- Implication: `motioner_ui` depends on the engine UI crates; GUIMaker is
  the design-time tool whose generated code and widget support we
  improve upstream. REF-0001 section 3 and D-0004 say `motioner_ui`
  depends on GUIMaker; correct that when T-ARCH-0007 starts.
- User direction (2026-10-31): GUIMaker must become an egui-class UI
  toolkit AND a SceneBuilder-style visual designer, very professional and
  configurable. Needs an architecture proposal and ADR before any code.
- Dev clones (full history, for upstream branches/PRs):
  `C:\Users\jvchi\CARPETAS\FerrousEngine` (at 02e6d48, same as the pin)
  and `C:\Users\jvchi\CARPETAS\GUIMaker` (at 9dcbc28). Motioner keeps
  using only the pinned submodule.
- Engine UI base already present at 02e6d48: `ferrous_ui_core` (~11.7k
  lines, retained-mode `UiTree`, theme, reactive, animation, reflection,
  widgets incl. dock_layout, split_pane, tabs, tree_view, virtual_list,
  virtual_grid, data_table, color_picker, infinite_canvas, modal,
  tooltip) plus a declarative RON/JSON `.fui` loader
  (`engine/ferrous/crates/ferrous_ui_core/src/fui_loader.rs`), taffy-based
  `ferrous_layout`, `ferrous_ui_render`, `ferrous_gui`. Build on these;
  do not rewrite them.

## Gotchas

- CI `RUSTFLAGS=-D warnings` will fail on engine path deps; use
  `submodules: true` and rely on `clippy -D warnings` instead.
- `scripts/check_boundaries.py` bans ferrous/wgpu prefixes everywhere:
  exempt motioner_render and add a motioner_dsl rule.
- Git Bash rewrites '/x' args: use `MSYS_NO_PATHCONV=1` for
  `git sparse-checkout`.
- Bash heredocs with mixed quotes broke; write files with the editor.
- Disk: ~20 GB free on C: (2026-10-31). Stop below 8 GB. Use CARGO_INCREMENTAL=0.
- D-0003 superseded by D-0011; MDF v0 subset frozen; `while`, `use`,
  dicts, overrides and LaTeX deferred to v0.1.
- `origin` is jvchiappini/Motioner; do not touch Motioner-legacy; no
  push without permission.

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Last relevant session

S-2026-10-31-010 (open) - approved hardening and step budget, committed
480191e and e77a524, closed T-DSL-0003/0004, pushed the branch, issued the
first-render prompt.
