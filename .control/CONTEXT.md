---
actualizado: 2026-11-01
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0030.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor (prompts, reviews, checks, small fixes). Conversation
  with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- Prompt 10 is a 12-phase job. **ADR and Phases 1-3 are done.** Every later
  phase has its backlog task (T-RENDER-0009..0013, T-CORE-0002, T-UI-0006/0007,
  T-APP-0003, T-PERF-0002) with its design in the body.

## Current state (branch feat/m1-foundations, sessions 015-017 uncommitted)

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
  `examples/animatable.motion`, golden `animatable_f45.png`.
- **Vector paths (D-0029)**: `path(points, closed, fill, stroke,
  stroke_width, cap, join, trim_start, trim_end, trim_offset, dash,
  dash_offset)`, helpers `polygon`, `star`, `arc`. A point is `(x, y)` or
  `(x, y, in_dx, in_dy, out_dx, out_dy)` with relative handles.
  - Geometry lives in `motioner_core::path`: adaptive flattening (0.1 px),
    trim as a wrapping arc-length window, dash on=gap, even-odd containment,
    local bounds. One implementation for render, bounds, tests.
  - `NodeContent::Path` holds the geometry plus five animatable numeric
    channels; `cap`/`join` are discrete content fields.
  - The render plan has a path arena: `ItemShape::Path { index }` +
    `RenderPlan::path(index)`.
  - `motioner_render::paths` tessellates fill/stroke pieces with
    `lyon_tessellation` into local triangles, hard internal edges, per-node
    cache, public `path_bounds`.
  - Editor: path layers; trim/dash rows with stopwatches and keys; cap/join
    text fields; selection bounds and picking against fill or stroke.
  - `examples/paths.motion`, goldens `paths_f45.png` / `paths_f80.png`.
- **Gradients (D-0030, session 017)**:
  - `linear_gradient(start, end, stops)` and `radial_gradient(center,
    radius, stops)` are paint values for `fill`, `stroke` and text `color`.
    2..16 `(offset, "#rrggbb[aa]")` stops; offsets clamp to 0..1 and are
    sorted at sample time; interpolation is linear-light sRGB. Two stops at
    one offset make a hard edge.
  - Each stop is an ordinary content channel: `fill_stop<n>_offset`,
    `fill_stop<n>_color`, `stroke_stop<n>_offset`, `stroke_stop<n>_color`.
    `ContentField` is now partly dynamic (`name()` returns `String`).
  - `PaintSpec` stores a solid fallback plus `fill_gradient`/`stroke_gradient`;
    `NodeContent::Text` stores an optional gradient. A solid colour clears the
    gradient; `fill="none"` clears both.
  - `ContentField::field_exists` is the write check (a stop only exists when
    the gradient and index do); `Document::set_content`/`add_content_keyframe`
    use it.
  - Eval pushes `ResolvedGradient { kind, stops }` into `RenderPlan.gradients`;
    `ResolvedPaint` is `Copy` (`Solid(Color) | Gradient(usize)`).
  - Render builds `GradientTable` (`motioner_render::gradients`) with the
    gradient transformed by the owning item's world transform (local space).
    On the pinned engine `FrameRenderer::render` returns
    `RenderError::Unsupported { feature: Feature::Gradient, node }`.
  - The `engine-gradients` feature (off by default) uploads the table and
    calls `ShapeInstance::with_gradient`. It requires the clone's Engine Part
    A; text gradients are still refused even with the feature.
  - Editor: one row per stop offset/colour with the same stopwatch, diamonds
    and eased lines; inspector sections Fill/Stroke/Text show "Gradient:
    linear/radial, N stops"; labels read "Stop N offset/colour"; `query`
    serialises the gradient object and stop channels.
  - `examples/gradients.motion`; goldens `gradients_f30.png` (SHA
    `466dceb4…`) and `gradients_f105.png` (SHA `d0c73e8…`);
    `golden_gradients.rs` compares when the renderer supports gradients and
    skips on the pinned engine.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...]`.
- **GPU tests**: `motioner_testkit::gpu::lock()` is a process-wide mutex held
  for a whole GPU test; the normal `cargo test --workspace` gate is green
  without `--test-threads=1`.

## Gotchas

- CI `RUSTFLAGS=-D warnings` will fail on engine path deps; use
  `submodules: true` and rely on `clippy -D warnings` instead.
- `scripts/check_boundaries.py`: `motioner_ui` may not depend on
  `motioner_io`, so the app passes a `PngWriter` fn to the UI.
- Disk: C: was ~6.0 GB after Phase 2; deleting `target/debug/incremental`
  brought it back to ~7.7 GB. Keep `CARGO_INCREMENTAL=0` and `-j 2`; stop
  below 5 GB.
- `lyon` path builders must end the subpath: `builder.close()` for closed
  pieces and `builder.end(false)` for open ones, or `build()` panics in debug.
- Windows PowerShell 5.1 drops/duplicates `"` in native args. Call
  `scripts/ui_shot.ps1` with `& ./scripts/ui_shot.ps1 -Commands
  'select("ring")','scroll("timeline", 235)'` **from PowerShell**; wrapping it
  in `powershell -File` loses the quotes and joins the commands. The script
  escapes each command for `ctl`.
- **PowerShell double-quoted strings treat backticks as escape characters and
  single quotes keep `\n` literal**. Do multi-line edits with the editor tool,
  never with `Set-Content`/`Replace` on code or backticked markdown.
- `core.autocrlf=true`: working copies are CRLF, commits are LF; run
  `cargo fmt` after.
- 8 GB RAM: cargo with default parallelism can fail linking (`os error 1455`).
- Clippy pedantic: `option_option`, `float_cmp`, `similar_names`,
  `neg_cmp_op_on_partial_ord`, `cast_precision_loss`,
  `cast_possible_truncation`, `too_many_lines`, `items_after_statements`,
  `implicit_clone`, `unnested_or_patterns`, `doc_markdown` are denied. Prefer
  `f64::from(u32)`, tolerances, and split long functions.
- `engine-gradients` only compiles against the clone (the pinned
  `ShapeInstance` has no `gradient` field). Never enable it in the default
  workspace build until the pin bump.

## FerrousEngine state

- Pin `82f7210` on `feat/pure2d-text-and-readback` (pushed, not merged).
- **Engine Part A for gradients is implemented in the clone, uncommitted**:
  - `ShapeGpu` is 68 bytes with a `gradient: u32`; `ShapeInstance` has
    `gradient` and `with_gradient(base)`/`gradient_base()`.
  - `Uniform2d` carries `gradients: [[f32; 4]; 512]` (`GRADIENT_TABLE_VEC4`);
    `Renderer2d::set_gradient_table` + `Renderer::set_2d_gradient_table`.
  - `shape_2d.wgsl` evaluates sorted stops in linear light, dithers with an
    8x8 Bayer matrix that fades at the stops, and shades all shape paths.
  - Test `crates/ferrous_renderer/tests/pure2d_gradients.rs` (stops, linear
    midpoint, banding) passes.
  - The text pass is untouched; text gradients are unsupported.
  - Files are LF; `rustfmt --edition 2021 --config skip_children=true` was run
    on the edited engine files.
- Likely needed later: blend pipelines (Phase 4), offscreen layers + blur
  (Phase 5), masks/stencil (Phase 6), TTF by family (Phase 7), image textures
  (Phase 8), motion-blur accumulation (Phase 9), text gradients.
- Golden `hello_f120.png` SHA `c2173bad...` (still byte-identical).

## GUIMaker facts (read via GitHub API, not cloned, 2026-10-31)

- GUIMaker is a standalone visual designer BINARY that generates Rust UI
  code; it does not build against the current engine (`ferrous_app` removed).
- User direction: egui-class toolkit AND SceneBuilder-style designer; needs
  an architecture proposal and ADR before code (T-ARCH-0007).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Editor UI

- Chrome is an immediate painter on the engine `GuiBatch`; tokens in
  `theme.rs` are the only colour literals.
- One key-marker language (`widgets::key_marker`).
- Content rows (`fill`, `width`, `size`, `trim_start`, `dash`, ...) draw the
  same controls as transform rows. Gradient stops add one pair of rows each,
  labelled "Stop N offset" / "Stop N colour" in the timeline and "Stop N
  Offset/Colour" in the inspector; the inspector shows a read-only "Gradient"
  summary row per slot.
- Path layers use the "Path" icon; the outline detail is "N pts[, closed]".

## Last relevant session

S-2026-11-01-007 (opencode): prompt 10, Phase 3 (gradients) complete. GPU tests
serialized, ADR D-0030, core/DSL/eval/editor/query/bake, engine Part A in the
clone, goldens and UI captures. Next: Phase 4 (T-RENDER-0009, blend modes).
