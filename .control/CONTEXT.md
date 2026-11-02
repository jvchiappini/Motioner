---
actualizado: 2026-11-01
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0031.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor. Conversation with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- Prompt 10 is a 12-phase job. **ADR and Phases 1-4 are done.** Every later
  phase has its backlog task (T-RENDER-0010..0013, T-CORE-0002, T-UI-0006/0007,
  T-APP-0003, T-PERF-0002) with its design in the body.
- **FerrousEngine is a general-purpose engine, not a Motioner backend.** A
  missing render capability is built in the clone as a public engine feature
  with rustdoc, `crates/<crate>/docs/`, an `mkdocs.yml` entry and engine
  tests; Motioner only consumes it. The user commits the clone and bumps the
  pin.

## Current state (branch feat/m1-foundations)

- `motioner render|still|info|bake|preview|query|edit|ctl`.
- The editor EDITS through overrides (D-0022); the program only changes with
  `bake`.
- **Agent control (D-0024)**: one `Command` enum, MDF call syntax,
  `motioner_ui::command`, executed only by the window-free `Editor`.
  `motioner query` (JSON scene), `motioner edit` (all-or-nothing,
  `--dry-run`), `preview --control` + `motioner ctl`. Docs: `docs/agents.md`.
- **Canvas view (D-0026)**: `ViewState.zoom/pan/fit_zoom`;
  `viewport::Viewport` is the one window → composition mapping.
- **Orphans (D-0025)**: `Editor::orphan_suggestions()` ranks live nodes.
- **Hybrid bake (D-0027)**: value origins + in-place literal edits + re-key +
  verify.
- **Professional motion model (D-0028)**: content fields are animatable
  channels (`Animatable<f64|Color|Option<Color>>`), colour interpolation is
  linear-light sRGB. `NodeContent::channel(field)` is the one field table.
- **Vector paths (D-0029)**: `path`, `polygon`, `star`, `arc`, animatable
  trim/dash, lyon tessellation with a per-node cache.
- **Gradients (D-0030)**: `linear_gradient`/`radial_gradient` for fill, stroke
  and text; 2..16 stops with animatable offset/colour; linear-light
  interpolation; plan gradient arena; per-frame engine table. Gradients are
  **unconditional** since pin 857b9df (the `engine-gradients` feature is gone).
  Text gradients use the engine text pass; the temporary
  `engine-text-gradient` feature is on by default since pin e9fb90d.
- **Blend modes (D-0031)**: `normal`, `add`, `multiply`, `screen`, `overlay`,
  `darken`, `lighten` on every node (discrete property, overridable, not
  keyable). Motioner wraps a non-normal node in an engine layer; the first four
  use fixed-function composite, the last three need the engine backdrop pass.
  Since pin e9fb90d all seven render; the temporary `engine-composite`
  feature is on by default (without it the last three return
  `RenderError::Unsupported { feature: BlendMode, node }`).
  - Engine API: `CompositeMode2d` (7), `Layer2d::new(opacity, impl Into<
    CompositeMode2d>, depth)`, `BackdropCompositePipeline`, and
    `Renderer::render_to_texture_view` (needed so the compositor can snapshot
    the destination).
  - The advanced path copies the destination into a scratch texture and runs a
    full-target composite pass with blending disabled (one copy + one pass per
    advanced composite).
  - `examples/blend.motion` (7 two-tone swatches over #808080); golden
    `blend_f30.png` (SHA `052dd593…`).
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...]`.
- **GPU tests**: `motioner_testkit::gpu::lock()` serializes them; the normal
  `cargo test --workspace` gate is green without `--test-threads=1`.

## Gotchas

- **PowerShell**: double-quoted strings expand `` `n `` (backtick-n) but keep
  `\n` literal; single-quoted keep both literal. Use here-strings or the edit
  tool for multi-line file edits.
- CI `RUSTFLAGS=-D warnings` fails on engine path deps; use submodules and rely
  on clippy.
- `scripts/check_boundaries.py`: `motioner_ui` may not depend on `motioner_io`.
- 8 GB RAM: cargo needs `-j 2`; `CARGO_INCREMENTAL=0`.
- `core.autocrlf=true`: working copies CRLF; `cargo fmt` after editing.
- Disk: keep C: above 5 GB; clean `target/debug/incremental` below 6 GB.
- Clippy pedantic: `option_option`, `float_cmp`, `similar_names`,
  `implicit_clone`, `items_after_statements`, `doc_markdown`,
  `too_many_arguments` are denied. Every temporary feature is documented.
- Engine clippy `-D warnings` fails on pre-existing lints in `ferrous_assets`
  and `ferrous_ecs`; use `--no-deps` on the crates being touched.
- `Layer2d` struct literals in engine tests use `CompositeMode2d` now
  (`BlendMode2d` still converts with `.into()`).
- The engine harness renders through `render_to_texture_view`; `render_to_view`
  has no texture and falls back to `Normal` for backdrop composites.
- The inspector scroll command does not move the panel in `preview` (the
  selection reveal resets it); capture tall inspector content by making the
  row first (blend lives at the top).

## FerrousEngine state

- Pin `857b9df` (Pure2D gradients), pushed.
- **Uncommitted in the clone** (all LF, rustfmt on edited files):
  - `crates/ferrous_2d/src/render/gradient.rs`: `Gradient2d`,
    `GradientKind2d`, `GradientStop2d`, `GradientTable2d` (general encoder,
    512 slots), unit tests. Exported from `render`.
  - Text gradients: `TextInstance.gradient` (+ vertex attribute 7),
    `TextInstance::with_gradient`, `Renderer::draw_text_2d_with_gradient`,
    `Pure2dHarness::draw_text_with_gradient`; shared WGSL moved to
    `assets/shaders/pure2d_common.wgsl` (camera layout + gradient helpers),
    prepended to `shape_2d.wgsl` and `text_2d.wgsl`.
  - `CompositeMode2d` (7 modes), `Layer2d.blend`, `CompositeInstance.mode`,
    `BackdropCompositePipeline`, `assets/shaders/composite_2d_blend.wgsl`,
    `render_to_texture_view`, plan splitting with destination snapshots,
    `COPY_SRC` on layer textures.
  - Docs: `crates/ferrous_2d/docs/gradients.md`, `blend.md`, updated
    `text.md`/`README.md`, `mkdocs.yml` nav.
  - Tests: `pure2d_gradients.rs` (incl. text), `pure2d_blend.rs` (exact values
    per mode), `pure2d_layers.rs` updated.
- Likely needed later: offscreen effects (Phase 5), masks/stencil (Phase 6),
  TTF by family (Phase 7), image textures (Phase 8), motion blur (Phase 9).
- Golden `hello_f120.png` SHA `c2173bad...` (still byte-identical).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Editor UI

- Chrome is an immediate painter on the engine `GuiBatch`; tokens in
  `theme.rs` are the only colour literals.
- One key-marker language (`widgets::key_marker`).
- The inspector starts with a **Composite / Blend** seven-tile grid (4 + 3),
  accent on the active mode; the timeline shows content rows below.
- Gradient stops add one row pair each ("Stop N offset"/"Stop N colour").

## Last relevant session

Phase 4 (blend modes) complete: engine `CompositeMode2d` + backdrop compositing
in the clone, text gradients, gradient table API reviewed, Motioner model/DSL/
renderer/UI/query/overrides, `examples/blend.motion`, goldens, captures. Engine
committed as e9fb90d (mojibake in 5 engine files repaired first), pin bumped,
both temporary features on by default. Next: Phase 5 (T-RENDER-0010, effects),
which first deletes the two features and the golden skip paths.
