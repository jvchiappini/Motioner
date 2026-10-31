# opencode prompt 05 — FerrousEngine: native Pure2D text, robust headless readback

You are the orchestrator of a long, multi-phase job in the **FerrousEngine**
repository at `C:\Users\jvchi\CARPETAS\FerrousEngine` (Rust, wgpu 23, glam
0.29). The branch `feat/pure2d-text-and-readback` is already checked out; stay
on it. FerrousEngine is a custom engine used by several consumers
(`ferrous_engine` runner, `ferrous_gui`/`ferrous_ui_*`, `ferrous_editor`,
`ferrous_web` for wasm, and the Motioner motion-graphics editor, which pins
this repository and renders headlessly through `ferrous_renderer::headless`).

Goal: give the engine first-class, deterministic 2D text in the Pure2D path,
built on the existing MSDF font stack, and make headless readback
allocation-free, typed and pipelined. Motioner will use both to render video
frames; every other consumer must keep compiling and behaving the same.

The quality bar is a top-tier studio: no duplication, rustdoc on every public
item, typed errors, regression tests for every behavior, no warnings in the
crates you touch. Work through every phase in order and run the phase gate
before moving on. Do not stop early unless a stop condition applies.

---

## 0. Ground rules

- Work only inside `C:\Users\jvchi\CARPETAS\FerrousEngine`. Do not touch the
  Motioner repository.
- Do not commit, stage, stash, reset, checkout, rebase or push. The reviewer
  commits.
- Build with `CARGO_INCREMENTAL=0`. If free space on C: drops below 8 GB,
  stop and report. Prefer `-p <crate>` builds over the whole workspace except
  in the gates.
- Do not reformat files you do not otherwise change (the repository is not
  fmt-clean). Run `rustfmt` only on files you edit
  (`rustfmt --edition 2021 <files>`), never `cargo fmt --all`.
- New dependencies: none, except `thiserror` is NOT allowed either; write
  `Display` and `std::error::Error` by hand. `log` is already a workspace
  dependency and may be used instead of `println!`/`eprintln!`.
- Public API changes must be additive or come with every in-repo caller
  updated in the same job. List each breaking change in the report.
- Determinism: same inputs → same pixels on the same adapter. No randomness,
  no clock in rendering decisions, stable ordering for ties.
- Shaders live in `assets/shaders/` (included with `include_str!`). Do not
  duplicate WGSL logic: shared MSDF functions go into one file that both text
  shaders include by string concatenation at pipeline creation.
- Subagents may be used for phases 2 and 3 in parallel; each owns only the
  files assigned below. Phases 4–6 are sequential.

## 1. Verified facts (read the cited code before changing it)

Readback / harness (`crates/ferrous_renderer/src/headless.rs`):
- `OffscreenTarget::read_rgba8` (line ~107) creates a new GPU buffer and a
  new `Vec` on every call, `assert!`s the format and `expect`s the map result
  (panics on failure). `read_pixel` reads the whole frame for one pixel.
- `Pure2dHarness::new(width, height, format) -> Result<Self, String>` creates
  a windowless `EngineContext`, a Pure2D `Renderer` and an `OffscreenTarget`;
  the clear colour is hard-coded opaque black. `render_frame` calls
  `Renderer::render_to_view`, which exists only with the default `gui`
  feature (`crates/ferrous_renderer/src/renderer_core.rs:1636`, `gui` gates
  at lines 39, 197, 515, 528, 576).
- Existing tests: `crates/ferrous_renderer/tests/pure2d_headless.rs`,
  `crates/ferrous_renderer/tests/sprite_texture_recycle.rs`; bench
  `crates/ferrous_renderer/benches/pure2d_headless.rs`.

Pure2D drawing:
- `ShapeInstance` / `SpriteInstance` (`crates/ferrous_2d/src/render/types.rs`)
  carry a column-major model matrix; depth is `transform_c3[2]`. Colours are
  linear RGBA; the target is `*Srgb`. Blending is straight alpha.
- Shapes and sprites are merged into one command stream sorted by ascending z
  with a deterministic tie-break
  (`DrawCommand2d`, `crates/ferrous_2d/src/render/batch.rs:86-110`; the
  per-frame list is `draw_list_scratch` in
  `crates/ferrous_renderer/src/renderer_core.rs:324`). Read how the coalesced
  painter-order upload is built in `render_internal`
  (`renderer_core.rs:1655`) before adding a new command kind.
- Pure2D world: origin at the centre, Y up, 1 unit = 1 logical pixel.

Fonts and text:
- `crates/ferrous_font`: `FontParser` reads `units_per_em`, glyph outlines
  (normalised to em units) and advances (`parser.rs`), but not
  `hhea` ascender/descender/lineGap, per-glyph bounds, or kerning.
  `FontAtlas::new` (`atlas.rs:26`) packs 64 px MSDF cells, `println!`s, and
  stores `GlyphMetrics { uv, size, advance }`; the em → cell mapping is
  implicit in `msdf_gen.rs:227-240`. `Font::from_bytes_and_chars`
  (`lib.rs`) `expect`s parse success.
- `assets/shaders/text.wgsl` decodes the MSDF with `median(R,G,B)` and the
  `screenPxRange` method; it is used by the GUI text pipeline
  (`crates/ferrous_ui_render/src/pipelines/text.rs`).
- GUI layout `crates/ferrous_ui_render/src/text_utils.rs` places glyphs with
  magic constants (`box_scale = 1.6`, offsets `0.3 * size`) and no kerning.
- `assets/fonts/Roboto-Regular.ttf` is available for tests (Apache-2.0).

---

## 2. Phases

### Phase 1 — baseline and design (orchestrator, no code)

1. Read every file cited above, plus the GUI text pipeline and its shader.
2. Run the gate (section 4) on the untouched branch and record every result,
   including the current warning counts of `ferrous_font`, `ferrous_2d` and
   `ferrous_renderer`.
3. Write the design into your final report (not into a file): the public API
   of each phase, the em → atlas-cell mapping you will make explicit, the
   text draw-command ordering rule, and the double-buffered readback state
   machine. Then launch phases 2 and 3.

### Phase 2 — headless readback and harness (subagent R)

Owner: `crates/ferrous_renderer/src/headless.rs`,
`crates/ferrous_renderer/tests/pure2d_headless.rs`, new test files under
`crates/ferrous_renderer/tests/` named `headless_*.rs`, and the bench file.

1. `pub enum ReadbackError` (unsupported format, map failure carrying the
   wgpu `BufferAsyncError` message, device lost/channel closed, size
   mismatch) with `Display` + `Error`.
2. `OffscreenTarget` owns a persistent staging buffer sized for its padded
   rows, created once. New
   `pub fn read_rgba8_into(&self, device, queue, out: &mut Vec<u8>) -> Result<(), ReadbackError>`
   reuses both the staging buffer and `out`'s allocation (resize, then copy
   rows without padding). Keep `read_rgba8` as a thin wrapper over it so
   existing callers compile; its panic becomes an `expect` with a clear
   message only in that legacy wrapper, documented as such. `read_pixel`
   copies a single texel region (1x1 copy into a small persistent buffer)
   instead of the whole frame.
3. Pipelined readback for video: a two-slot ring of staging buffers on the
   harness. `pub fn submit_readback(&mut self) -> Result<(), ReadbackError>`
   records the copy of the current target into the next slot;
   `pub fn take_readback(&mut self, out: &mut Vec<u8>) -> Result<bool, ReadbackError>`
   maps and returns the oldest pending slot (false when none pending). Frame
   N's copy overlaps frame N+1's rendering. Document the latency (one frame)
   and that results come out in submission order. No per-frame allocation in
   steady state; prove it with the bench (divan `AllocProfiler` already
   installed there) or a counting test.
4. `pub enum HarnessError` replaces `String` in `Pure2dHarness::new`
   (device creation, unsupported format). Update every in-repo caller.
5. `Pure2dHarness::with_clear_color(width, height, format, wgpu::Color)` or a
   `set_clear_color` method; keep `new` defaulting to opaque black.
6. Tests: `read_rgba8_into` equals `read_rgba8` byte for byte on a scene;
   the staging buffer is reused across 100 reads (expose a debug counter or
   check buffer identity through a test-only accessor); pipelined readback
   returns frames in order with the right content (render red, green, blue
   frames; read back in order); `read_pixel` matches the full readback;
   unsupported format returns `ReadbackError::UnsupportedFormat` instead of
   panicking. GPU tests skip cleanly with a printed reason when no adapter
   is available (never `#[ignore]`).

### Phase 3 — font metrics and CPU text layout (subagent F, parallel with phase 2)

Owner: `crates/ferrous_font/**`.

1. Parser: read `hhea` (ascender, descender, lineGap), `head` (unitsPerEm,
   bounding box), per-glyph bounds from `glyf` headers, and kerning pairs
   from the `kern` table format 0 (GPOS kerning is out of scope: detect it,
   log once, and report it). Expose a `pub struct FontMetrics` in em units.
2. Atlas: make the em → cell mapping explicit and public: each
   `GlyphMetrics` gains plane bounds (left, bottom, right, top in em units of
   the glyph's quad relative to the pen position on the baseline), keeping
   the existing fields for compatibility. Expose the atlas `px_range` (the
   distance range used by `generate_msdf`, in atlas pixels) so shaders do not
   hard-code it. Replace `println!` with `log::debug!`. Add
   `Font::try_load_bytes(...) -> Result<Font, FontError>` (typed error) and
   make the existing `load_bytes` delegate to it.
3. Pure CPU layout, new module `layout.rs`:
   `pub fn layout_line(font_metrics/atlas, text: &str, size_px: f32, align: TextAlign, out: &mut Vec<GlyphQuad>) -> LineMetrics`
   placing one quad per visible glyph with kerning, baseline at y = 0, the
   pen starting so that `Left`/`Center`/`Right` alignment is relative to the
   origin, whitespace advancing without quads, and missing glyphs drawn as
   the `.notdef` glyph if present or skipped and counted in `LineMetrics`.
   `GlyphQuad` holds the quad rectangle in pixel units (Y up, baseline
   origin) and the atlas UV rectangle. Reuses `out`'s allocation. Multi-line
   text is out of scope (report how it would extend).
4. Tests on pure data (no GPU): Roboto ascender/descender values match the
   font file (compare with known Roboto-Regular metrics read from the file);
   "AV" is narrower with kerning than without (if Roboto has a `kern` table;
   if it only has GPOS, assert the detection and use a synthetic font table
   test for the `kern` parser); centre alignment is symmetric; `layout_line`
   is deterministic and allocation-reusing; bounds of "o" contain its hole.

### Phase 4 — Pure2D text rendering (orchestrator or subagent T, after 2 and 3)

Owners: `crates/ferrous_2d/src/render/**`, `crates/ferrous_renderer/src/renderer_core.rs`
(only the Pure2D text additions), `crates/ferrous_renderer/src/headless.rs`
(only the harness text methods, after phase 2 merged), `assets/shaders/`
(new `text_2d.wgsl` and the shared MSDF include), and the GUI text pipeline
only to make it use the shared MSDF include.

1. Shared WGSL: extract `median` and the `screenPxRange` opacity function
   from `assets/shaders/text.wgsl` into `assets/shaders/msdf_common.wgsl`;
   the GUI text pipeline and the new 2D text pipeline both build their
   module source as `msdf_common.wgsl` + their own file. GUI output must not
   change (verify by reasoning and by any existing GUI test).
2. `TextInstance` in `ferrous_2d` (one per glyph quad): column-major model
   matrix like `ShapeInstance` (so rotation, non-uniform scale and parents
   compose exactly the same way), atlas UV rect, linear RGBA colour, and the
   parameters the shader needs for `screenPxRange`. A `text_2d.wgsl`
   pipeline with straight-alpha blending and the same target format/sample
   count as the shape pipeline.
3. Renderer API (Pure2D):
   `pub fn register_font_2d(&mut self, bytes: &[u8], charset) -> Result<FontId2d, FontError>`
   (builds the atlas once; atlases are freed with `free_font_2d`), and
   `pub fn draw_text_2d(&mut self, font: FontId2d, text: &str, transform: glam::Mat4, size_px: f32, align: TextAlign, color: [f32; 4])`
   which lays out with `ferrous_font::layout_line` into a reusable scratch
   buffer and pushes one `TextInstance` per glyph whose model matrix is
   `transform × glyph-quad placement`. Depth for ordering is
   `transform`'s translation z, shared by all glyphs of the call.
4. Ordering: add `DrawCommand2d::Text` to the unified z-sorted stream so text
   interleaves with shapes and sprites by z. Document the full tie-break for
   equal z (keep existing shape/sprite order; text after sprites; within a
   kind, submission order). Coalesce consecutive glyphs of the same atlas
   into one draw call.
5. Harness: `register_font`, `draw_text` mirroring the renderer API.
6. No per-frame allocation in steady state (scratch buffers reused; GPU
   instance buffer grows geometrically and is reused).
7. Headless tests (skip cleanly without an adapter):
   - `text_renders_at_the_baseline`: "H" at origin, size 100, white on black;
     lit pixels lie between the baseline and the cap height (from metrics),
     nothing below the baseline beyond 1 px.
   - `text_alignment`: left/centre/right place the ink box correctly
     (±1 px).
   - `text_interleaves_by_z`: rect(z=1), text(z=2), rect(z=3) overlapping;
     check the colour of each overlap.
   - `text_transform`: 90° rotation and 2× scale move and scale the ink box
     as the matrix says.
   - `text_is_sharp_when_scaled`: at size 32 scaled 8×, the edge transition
     across a vertical stem is at most 2 px wide.
   - `text_is_deterministic`: the same frame twice gives identical bytes.
   - `text_colour_is_linear`: 50 % opacity white text over black matches the
     expected sRGB byte for straight alpha in linear light (± 2).
8. Write `crates/ferrous_2d/docs/text.md` (or extend the existing 2D docs if
   a docs folder exists): API, coordinate contract, ordering, limitations.

### Phase 5 — headless without the GUI stack (orchestrator)

Investigate whether `ferrous_renderer` can build and run the Pure2D headless
path with `default-features = false` (no `gui`, no winit/GUI crates). If the
change is contained (gating, moving `render_to_view`'s Pure2D branch out of
the `gui` cfg), do it and add a CI-style check
`cargo test -p ferrous_renderer --no-default-features --test pure2d_headless`.
If it needs a large refactor, do not do it: report the exact blockers with
file:line and a proposed plan.

### Phase 6 — warnings, consumers, CI, review (orchestrator)

1. Zero compiler and clippy warnings in `ferrous_font`, `ferrous_2d` and
   `ferrous_renderer` with `cargo clippy -p <crate> --all-targets -- -D warnings`
   (fix the pre-existing ones too: unused imports/variables, dead fields,
   private type in public API, unnecessary parentheses; remove truly dead
   code rather than silencing it; any `#[allow]` must be item-level with a
   one-line justification).
2. Every consumer still builds: `cargo check --workspace --all-targets` and
   `cargo check -p ferrous_web --target wasm32-unknown-unknown` (the wasm
   target is installed).
3. Add `.github/workflows/ci.yml` (keep the existing workflows): on push and
   pull request, Windows and Ubuntu, run rustfmt check **only on the files
   this job touched or created** (list them explicitly), clippy `-D warnings`
   and tests for `ferrous_font`, `ferrous_2d`, `ferrous_renderer`, plus the
   wasm check. GPU tests already skip without an adapter.
4. Review the whole diff as a strict reviewer (duplication, rustdoc, typed
   errors, panics on runtime paths, per-frame allocations, tests that would
   pass without the code), fix findings, run the final gate.
5. Update `crates/ferrous_renderer/docs/` (or the nearest existing docs) for
   the readback API and the harness changes, and add a CHANGELOG-style
   section to the report listing every public API addition and change.

---

## 3. Acceptance criteria

- `read_rgba8_into` and the pipelined readback allocate nothing per frame in
  steady state and return typed errors; no panic on runtime paths except the
  documented legacy wrapper.
- Pure2D draws text with correct baseline, alignment, kerning (where the
  font has a `kern` table), transforms and z-interleaving; the GUI text path
  shares the MSDF shader code and is otherwise unchanged.
- All new tests pass on this machine (RTX 4050) and skip cleanly without a
  GPU.
- Zero warnings in the three crates; workspace and wasm checks pass.
- No new dependencies; no `unsafe`; rustdoc on all new public items.

## 4. Gate (end of phases 1, 2+3, 4, 5, 6; `CARGO_INCREMENTAL=0`)

- `rustfmt --edition 2021 --check <every file you created or edited>`
- `cargo clippy -p ferrous_font -p ferrous_2d -p ferrous_renderer --all-targets -- -D warnings`
  (from phase 6 on; before that, record the warning count and make sure you
  did not add any)
- `cargo test -p ferrous_font -p ferrous_2d -p ferrous_renderer`
- `cargo test -p ferrous_renderer --release --test pure2d_headless`
- `cargo check --workspace --all-targets`
- `cargo check -p ferrous_web --target wasm32-unknown-unknown`
- `cargo doc -p ferrous_font -p ferrous_2d -p ferrous_renderer --no-deps`
  with no new rustdoc warnings

## 5. Stop conditions

Stop and report instead of improvising if: a phase gate fails twice after a
fix attempt; a change would break a consumer in a way you cannot fix inside
this repository; the `kern`/`hhea` data of Roboto contradicts the design;
free space on C: < 8 GB; or the GUI text output would change.

## 6. Final report

1. Phase summaries (which subagent did what).
2. Files created/changed, one line each.
3. Public API additions and changes (exact signatures), with every breaking
   change and the callers you updated.
4. Design decisions (em → cell mapping, ordering tie-break, readback state
   machine, shader sharing) and deviations from this prompt.
5. Tests added per crate and which skip without a GPU.
6. Phase 5 outcome (done, or blockers with file:line).
7. Warning counts before/after per crate.
8. Gate table for the final run.
9. Free space on C: at the end and open questions.
