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
  (05 is the last one). Work only inside the Motioner, FerrousEngine and
  GUIMaker folders.
- The user wants LONG multi-phase prompts (several verifiable phases,
  subagents on disjoint files, one final report) and a visible result
  (PNG/MP4) as soon as possible.
- Goals include improving FerrousEngine and GUIMaker upstream with small
  patches, then bumping the pin in Motioner with an ADR.

## Current state (branch feat/m1-foundations, pushed to origin)

- FIRST RENDER WORKS: `cargo run --release -p motioner_app -- render
  examples/hello.motion --out out/hello.mp4` gives 300 frames 1920x1080
  60/1 yuv420p in 2.75 s (RTX 4050). CLI: `motioner render|still|info`.
- Commits after 757c0b5 (`main` untouched): D-0018 pin 61da7f9, core,
  eval, dsl, example, hardening 480191e/e77a524, control a8a34a5, engine
  pin bump 0b0737b (D-0019), first render 3fea9e3.
- Crates: `motioner_render` (Pure2D adapter: flip F, text F*A*S, strictly
  increasing z, linear colour, centred strokes, engine MSDF text,
  `read_pixels_into`), `motioner_io` (PNG, streaming ffmpeg H.264 BT.709),
  `motioner_app` (CLI), `motioner_testkit` golden
  `crates/motioner_testkit/golden/hello_f120.png`. 131 tests; all gates
  green incl. `cargo machete crates`. `ferrous_renderer` is built with
  `default-features = false`.
- Done: T-DSL-0003/0004, T-RENDER-0002, T-RENDER-0003. Backlog:
  T-RENDER-0004 (engine polish), T-DSL-0005, T-ARCH-0007 (GUIMaker
  architecture proposal pending).

## FerrousEngine state

- Pin `32aea8a` = head of engine branch `feat/pure2d-text-and-readback`
  (pushed, NOT merged to engine main). Adds Pure2D MSDF text, font
  metrics/kerning/layout, typed buffer-reusing readback, HarnessError,
  headless without `gui`, engine CI (`.github/workflows/ci.yml`).
- Open (T-RENDER-0004): readback ring does not overlap work; hard-edge
  band in `shape_2d.wgsl` can leave 1-px holes; FontId2d without
  generations; 8 warnings without default features; measure allocates.
- Still missing for Motioner: premultiplied alpha, blend modes and
  offscreen layers (straight `ALPHA_BLENDING` only); per-axis ring
  thickness. SSAO uses `thread_rng` (3D only).
- Engine repo stores 25 files with CRLF in the index; keep their CRLF
  when committing (core.autocrlf=true).

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
- Disk: only ~9.3 GB free on C: (2026-10-31, after first render). Stop
  below 8 GB; avoid release builds until space is freed. CARGO_INCREMENTAL=0.
- 8 GB RAM: cargo with default parallelism can fail linking with
  `os error 1455` (paging file too small). Use `-j 2` for builds/tests.
- D-0003 superseded by D-0011; MDF v0 subset frozen; `while`, `use`,
  dicts, overrides and LaTeX deferred to v0.1.
- `origin` is jvchiappini/Motioner; do not touch Motioner-legacy; no
  push without permission.

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Last relevant session

S-2026-10-31-010 (closed) - engine prompt 05 reviewed, engine 32aea8a
pushed, pin bumped (D-0019), prompt 04 rev 2 executed by opencode,
reviewed, first render committed 3fea9e3 and pushed.
