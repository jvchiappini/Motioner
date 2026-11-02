---
actualizado: 2026-11-02
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0034.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor. Conversation with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- Prompt 10 is a 12-phase job. **Phases 1-6 are done. Phase 7 (typography) is
  implemented and green, with the engine bump pending.** Every later phase has
  its backlog task (T-RENDER-0013, T-CORE-0002, T-UI-0006/0007, T-APP-0003,
  T-PERF-0002) with its design in the body.
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
  verify. Verification compares content, transform, **blend, effects, masks**.
- **Professional motion model (D-0028)**: content fields are animatable
  channels (`Animatable<f64|Color|Option<Color>>`), colour interpolation is
  linear-light sRGB. `NodeContent::channel(field)` is the one field table.
- **Vector paths (D-0029)**: `path`, `polygon`, `star`, `arc`, animatable
  trim/dash, lyon tessellation with a per-node cache.
- **Gradients (D-0030)**: `linear_gradient`/`radial_gradient` for fill, stroke
  and text; 2..16 stops with animatable offset/colour; linear-light
  interpolation; plan gradient arena; per-frame engine table.
- **Blend modes (D-0031)**: seven modes on every node (discrete property).
- **Layer effects (D-0032)**: ordered stack `drop_shadow` / `blur` / `glow`,
  params animatable, rendered via `push_layer_2d_with_effects`.
- **Masks (D-0033)**: `node.mask(source, mode=..., invert=...)`, stored in the
  `mask` override, rendered since pin e8b9ba7.
- **Typography (D-0034)**: Inter 400/600/700 in `assets/fonts/` (OFL.txt);
  `FontSet` lookup (project first, bundled last, nearest-weight warning, typed
  missing-family error); `TextMetrics` in core + `CpuTextMetrics` in render;
  `text(..., font=, weight=, tracking=, line_height=)`; `\n` multi-line;
  `text_width`/`text_bounds`; `node.animate_chars(...)` for opacity / offset /
  scale / rotation / colour. Tracking and line height are animatable channels;
  weight is discrete. Styled draw and per-glyph draw render since pin
  06a8adf. The inspector Text section has Family, Weight,
  Size, Tracking, Line height (stopwatches) and `\n` escaping. **Remaining:**
  `text_anim_add`/`text_anim_remove` commands, the animators override and bake.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...] [-Select ROW]`.
- **GPU tests**: `motioner_testkit::gpu::lock()` serializes them; the normal
  `cargo test --workspace` gate is green without `--test-threads=1`.

## Goldens

- `hello_f120.png` (byte-identical), `blend_f30.png`,
  `gradients_f30/f105.png`, effect stills, mask stills, path/animatable stills.
- Typography (D-0034): `typography_tracking_f30.png`,
  `typography_lines_f150.png`, `typography_chars_f260.png`. The fixture test
  validates the pixels and compares the render against the pin.
  `examples/typography.motion` has the three beats.

## Gotchas

- **PowerShell**: double-quoted strings and here-strings expand backticks and
  `$`, so never pass Python code containing `` ` ``, `\"` or `${...}` through
  `python -c @"..."@`; write a `.py` script with the editor tool instead.
  Native args drop unescaped `"`, which breaks `ctl select("@288")` and
  `scroll("inspector", n)`; use `--select ROW` and a taller window instead.
- `scripts/ui_shot.ps1` escapes quotes but the outer PowerShell invocation may
  still strip them; prefer `-Select`.
- 8 GB RAM: cargo needs `-j 2`; `CARGO_INCREMENTAL=0`.
- `core.autocrlf=true`: working copies CRLF; engine files edited with Python
  `newline=''` keep their EOL. rustfmt may turn a CRLF file into LF; restore it
  with a `\n` → `\r\n` pass.
- Disk: keep C: above 5 GB; clean `target/debug/incremental` below 6 GB.
- Clippy pedantic: `too_many_lines`, `too_many_arguments`, `option_option`,
  `float_cmp`, `implicit_clone`, `doc_markdown` and `must_use` are easy hits.
- Inspector order: Composite → Effects → Mask → Transform → Text.
- The inspector Text rows are Text, Family, Weight, Size, Tracking, Line height,
  Colour, Align. Family/Weight are one-tile cycle pickers.

## FerrousEngine state

- Pin `06a8adf` on `feat/pure2d-text-and-readback` (typography; committed
  and pushed):
  - `ferrous_font/src/parser.rs`: GPOS `PairPos` formats 1 and 2 (horizontal
    advance), `GposKerning`, typed `ParseError` (`Unsupported` for CFF/CFF2),
    `get_gpos_kerning`.
  - `ferrous_font/src/layout.rs`: `TextStyle` (tracking, line_height),
    `layout_block`/`measure_block`, `GlyphLayout`, `TextBounds`, `TextMetrics`
    and per-line metrics; `layout_line`/`measure_line` unchanged.
  - `ferrous_font/src/atlas.rs`: `GlyphMetrics.ink_bounds`, `FontAtlas.gpos`,
    block layout methods.
  - `ferrous_font/src/lib.rs`: `FontError::Unsupported`, re-exports.
  - `ferrous_2d/src/render/text.rs`: `TextStyle2d`, `GlyphLayout2d`,
    `TextMetrics`, `GlyphDraw2d`.
  - `ferrous_renderer/src/renderer_core.rs` + `headless.rs`: styled draw,
    `layout_text_2d`, `measure_text_block_2d` (block; the reviewer kept
    `measure_text_2d` single-line so the public API does not break),
    `draw_glyphs_2d`, `Pure2dHarness` mirrors.
  - Tests: `pure2d_text_layout.rs` (8), GPOS/CFF parser tests, flaky fix.
  - Docs: `crates/ferrous_2d/docs/text.md`, READMEs.
- **Flaky GPU tests**: the first parallel run after a build can fail with
  `wgpu error: Validation Error` / `create_buffer ... 'Particles Storage
  Buffer' ... Not enough memory left` or FXC out-of-memory. It is device
  contention (each harness owns a full renderer with a 32 MB particle buffer),
  not shared state. `tests/common/mod.rs` now has a process-wide `gpu::lock`
  used by `pure2d_effects`, `pure2d_masks` and `pure2d_text_layout`; the
  project_name mojibake line in `ferrous_ui_core/src/text_field_state.rs` was
  repaired.
- `CpuTextMetrics` still re-implements the block arithmetic over
  `FontParser` (no GPOS, re-parses the face per call); Phase 8 replaces it
  with the engine's `measure_block`.

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Editor UI

- Chrome is an immediate painter on the engine `GuiBatch`; tokens in
  `theme.rs` are the only colour literals.
- One key-marker language (`widgets::key_marker`).
- Mask rows: Source cycles through `Document::can_mask` candidates, Mode is
  Alpha/Luma, Invert is Off/On/Clear.
- Text rows: Family/Weight cycle tiles, Tracking/Line height numeric with
  stopwatches, and the content field shows `\n` for line breaks.

## Last relevant session

Phase 7 (typography) implemented: engine GPOS + block layout + per-glyph draw
(FerrousEngine 06a8adf, the pin), Motioner core/DSL/eval/render/editor, Inter
assets, three typography goldens and `examples/typography.motion`. The
reviewer removed the `engine-text-layout` gap with the bump. Next: Phase 8
(images) plus the Phase 7 leftovers.
