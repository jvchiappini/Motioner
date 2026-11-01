---
actualizado: 2026-11-01
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0029.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor (prompts, reviews, checks, small fixes). Conversation
  with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- Prompt 10 is a 12-phase job. **ADR, Phase 1 and Phase 2 are done.** The
  user asked to stop after Phase 2, compact and resume. Every later phase
  has its backlog task (T-RENDER-0008..0013, T-CORE-0002, T-UI-0006/0007,
  T-APP-0003, T-PERF-0002) with its design in the body.

## Current state (branch feat/m1-foundations, sessions 015 + 016 uncommitted)

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
- **Professional motion model (D-0028, session 015)**: content fields are
  animatable channels (`Animatable<f64|Color|Option<Color>>`), colour
  interpolation is linear-light sRGB, optional colours hold on a `none`
  endpoint. `NodeContent::channel(field)` is the one field table.
  `examples/animatable.motion`, golden `animatable_f45.png`.
- **Vector paths (D-0029, session 016)**: `path(points, closed, fill, stroke,
  stroke_width, cap, join, trim_start, trim_end, trim_offset, dash,
  dash_offset)`, helpers `polygon`, `star`, `arc`. A point is `(x, y)` or
  `(x, y, in_dx, in_dy, out_dx, out_dy)` with relative handles.
  - Geometry lives in `motioner_core::path`: adaptive flattening (0.1 px),
    trim as a wrapping arc-length window, dash on=gap, even-odd containment,
    local bounds. One implementation for render, bounds, tests.
  - `NodeContent::Path` holds the geometry plus five animatable numeric
    channels; `cap`/`join` are discrete content fields (settable, not keyable).
  - The render plan has a path arena: `ItemShape::Path { index }` +
    `RenderPlan::path(index)` → `ResolvedPath { geometry, trim, dash, paint }`
    so items stay `Copy`.
  - `motioner_render::paths` tessellates fill/stroke pieces with
    `lyon_tessellation` into local triangles, marks internal edges hard,
    caches per node by a hash of geometry+trim+dash+width+cap/join and
    transforms per frame. `path_bounds` is public for the editor.
  - Editor: path layers in the outline/inspector/timeline; trim/dash rows with
    stopwatches and keys; cap/join text fields; selection bounds and picking
    against fill or stroke; gizmo uses the node transform. Canvas point
    editing is T-UI-0006.
  - `examples/paths.motion` (960×540, 2 s): star, self-drawing arc, cubic
    ribbon with a moving trim window, dashed hexagon with marching dashes.
    Goldens `paths_f45.png` (SHA `d42669a9…`) and `paths_f80.png`
    (SHA `c646efcd…`), tolerance 2/0.1 %.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...]`.
- `query`: the `channels` object carries sampled values and keys; the
  `content` object carries the authored **static** values (fixed in session
  016; it used to emit malformed JSON for animatable content).

## Gotchas

- CI `RUSTFLAGS=-D warnings` will fail on engine path deps; use
  `submodules: true` and rely on `clippy -D warnings` instead.
- `scripts/check_boundaries.py`: `motioner_ui` may not depend on
  `motioner_io`, so the app passes a `PngWriter` fn to the UI.
- **GPU tests flake when run in parallel** (`wgpu` device creation panic in
  `motioner_render` integration tests). Run the suite with
  `-- --test-threads=1`; the gate passes that way.
- Disk: C: was ~6.0 GB free after Phase 2; deleting
  `target/debug/incremental` brought it to ~7.7 GB. Keep `CARGO_INCREMENTAL=0`
  and `-j 2`; stop below 5 GB.
- `lyon` path builders must end the subpath: `builder.close()` for closed
  pieces and `builder.end(false)` for open ones, or `build()` panics in debug.
- lyon fill/stroke tessellators return triangles with exact shared vertices;
  the edge map relies on that to mark internal edges hard (no seams).
- Windows PowerShell 5.1 drops/duplicates `"` in native args. For
  `ui_shot.ps1 -Commands`, pass plain `'select("ring")'` and let the script
  escape it; adding `\"` yourself produces `\\"` and a parse error.
- Bash heredocs containing `'''` break in this harness: write scripts to the
  scratchpad with the Write tool.
- PowerShell `Add-Content` appends *after* a module's closing brace; when
  adding a test to a file that ends with `mod tests { ... }`, remove the
  closing brace first and re-add it after the test.
- PowerShell `Get-Content -Raw` + `Set-Content` with `\n` in a single-quoted
  string inserts literal backslash-n; use the edit tool for multi-line edits.
- `core.autocrlf=true`: working copies are CRLF, commits are LF; mixing
  `Set-Content` with existing LF files makes `cargo fmt --check` fail with
  "Incorrect newline style"; run `cargo fmt` after.
- 8 GB RAM: cargo with default parallelism can fail linking (`os error 1455`).
- Clippy pedantic: `option_option`, `float_cmp`, `similar_names`,
  `neg_cmp_op_on_partial_ord`, `cast_precision_loss`,
  `cast_possible_truncation`, `too_many_lines` are all denied. Prefer
  `f64::from(u32)`, tolerances instead of `assert_eq!` on floats, and split
  long functions.

## FerrousEngine state

- Pin `82f7210` on `feat/pure2d-text-and-readback` (pushed, not merged).
- **Untouched by phases 1–2.** Pure2D already has the triangle SDF
  (`ShapeInstance::polygon_triangle`), so paths render for real without an
  engine change or a pin bump. The clone
  `C:\Users\jvchi\CARPETAS\FerrousEngine` has no new commits from this work.
- Likely needed later: gradient fills (Phase 3), blend pipelines (Phase 4),
  offscreen layers + separable blur (Phase 5), masks/stencil (Phase 6),
  TTF loading by family (Phase 7), image textures (Phase 8), motion-blur
  accumulation (Phase 9).
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
  `theme.rs` are the only colour literals; layouts are pure and shared by
  drawing and hit testing. See `crates/motioner_ui/README.md`.
- One key-marker language (`widgets::key_marker`): filled gold = key, dark
  core = shared on a layer row, accent = selected, hollow = inherited/none.
- Visibility strips: solid where drawn, outline where not. Eased segments
  are solid lines following the ease. Inspector ease grid with curves.
- Orphans rows: check icon = Rebind, error icon = delete.
- Content rows (`fill`, `width`, `size`, `trim_start`, `dash`, ...) draw the
  same controls as transform rows; `text`/`align`/`cap`/`join` stay static.
- Path layers use the "Path" icon (zigzag with end points); the outline
  detail is "N pts[, closed]"; the inspector has Path, Fill sections.

## Last relevant session

S-2026-11-01-006 (opencode): prompt 10, Phase 2 (vector paths) complete,
gates green. Next: Phase 3 (T-RENDER-0008, gradients) — first decide whether
gradients can be done on the CPU (per-vertex/per-pixel) or need a Pure2D
shader change, and ADR the gradient value model.
