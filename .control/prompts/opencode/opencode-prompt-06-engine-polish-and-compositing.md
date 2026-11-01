# opencode prompt 06 — FerrousEngine: polish pass and Pure2D compositing

You are the orchestrator of a long, multi-phase job in the **FerrousEngine**
repository at `C:\Users\jvchi\CARPETAS\FerrousEngine` (Rust, wgpu 23, glam
0.29). The branch `feat/pure2d-text-and-readback` (head `32aea8a`) is already
checked out; stay on it. FerrousEngine has several consumers (`ferrous_engine`
runner, `ferrous_gui`/`ferrous_ui_*`, `ferrous_editor`, `ferrous_web` for
wasm, and the Motioner motion-graphics editor, which pins this repository,
builds `ferrous_renderer` with `default-features = false` and renders video
frames headlessly through `ferrous_renderer::headless::Pure2dHarness`).

The job has two parts:

- **Part A — polish (T-RENDER-0004).** Fix five defects found in review of
  `32aea8a`: a readback ring that does not overlap work and never recovers
  from a failed map, one-pixel holes along hard triangle edges, font handles
  without generations, warnings without default features, and allocating
  text measurement.
- **Part B — compositing.** Give the Pure2D path what a motion-graphics
  compositor needs: premultiplied alpha end to end, per-draw blend modes, and
  offscreen layers (group opacity and group blend), with no per-frame
  allocation in steady state.

The quality bar is a top-tier studio: zero duplication, rustdoc on every
public item, typed errors, no panics on runtime paths, regression tests that
fail without the fix, no warnings in the crates you touch. Work through every
phase in order and run the phase gate before moving on. Do not stop early
unless a stop condition applies.

---

## 0. Ground rules

- Work only inside `C:\Users\jvchi\CARPETAS\FerrousEngine`. Do not touch the
  Motioner repository or any other folder.
- Do not commit, stage, stash, reset, checkout, rebase or push. The reviewer
  commits.
- Build with `CARGO_INCREMENTAL=0`. Free space on C: is about 9.3 GB. If it
  drops below 8 GB, stop and report. Avoid release builds except the one
  release test in the gate; prefer `-p <crate>` builds. Do not run
  `cargo clean` on anything other than this repository's `target/`, and only
  if space requires it (report it).
- The repository is not fmt-clean. Run `rustfmt --edition 2021 --config
  skip_children=true` only on files you edit; never `cargo fmt --all`.
- 25 files are stored with CRLF line endings in the index
  (`git ls-files --eol | grep crlf`). If you edit one of them, keep its CRLF
  endings; do not convert line endings of any file.
- No new dependencies (including dev-dependencies). No `thiserror`: write
  `Display` and `std::error::Error` by hand. Use `log` for diagnostics, never
  `println!`/`eprintln!` in library code. No new `unsafe`.
- Public API changes must be additive, or come with every in-repo caller
  updated in the same job. List each breaking change in the report.
- Determinism: same inputs give the same pixels on the same adapter. No
  randomness at runtime, no clock in rendering decisions, stable tie-breaks.
  Tests that need random input use a tiny seeded PRNG written in the test
  (for example SplitMix64 or xorshift64*) with a fixed seed, printed on
  failure.
- Shaders live in `assets/shaders/` and are included with `include_str!`.
  Shared WGSL goes into one file concatenated at pipeline creation (the
  existing `msdf_common.wgsl` pattern); never copy a WGSL function.
- GPU tests skip cleanly with a printed reason when no adapter is available
  (never `#[ignore]`). Reuse the existing skip helper of
  `crates/ferrous_renderer/tests/` instead of writing a new one.
- Subagents: phases 2, 3 and 4 may run in parallel, each owning only the
  files listed for it. Phases 5 to 9 are sequential and done by the
  orchestrator (or one subagent at a time with the owners listed).

## 1. Verified facts (read the cited code before changing it)

Readback (`crates/ferrous_renderer/src/headless.rs`, 892 lines):
- `OffscreenTarget::read_rgba8_into` (line ~276) and
  `Pure2dHarness::take_readback` (line ~848) each contain their own copy of
  "map, `poll(Maintain::Wait)`, check size, copy rows without padding,
  unmap". `read_pixel_into` (line ~369) is a third variant. `wait_for_map`
  (line ~446) is shared.
- `Pure2dHarness::render_frame` (line ~718) calls `device.poll(Maintain::Wait)`
  after every submit, so the CPU never runs ahead of the GPU.
- `submit_readback` (line ~797) records and submits the copy but does not
  call `map_async`; `take_readback` calls `map_async` and then
  `poll(Maintain::Wait)`. So the copy of frame N cannot overlap anything,
  and the ring gives no speed-up.
- On a map error in `take_readback`, `wait_for_map(rx)?` returns before
  `pending_readbacks.pop_front()`, so the slot stays at the front: every
  later call re-maps the same slot and returns the same error forever. A
  buffer whose map failed may also be left in a mapped-pending state.
- Motioner currently uses only `read_pixels_into` (synchronous) per frame.
  It will move to the ring once the ring overlaps work, so the ring's API
  must be one a video exporter can drive in a simple loop (see phase 2).

Hard edges (`assets/shaders/shape_2d.wgsl`, 675 lines):
- Triangles: in `vs_main` (line ~176) an all-hard triangle (`mask == 0`) is
  emitted as the exact triangle with no padding, so neighbouring triangles
  already tile by the rasteriser's fill rule. Mixed triangles (some edges
  anti-aliased) are grown about the centroid and clipped in the fragment
  shader.
- In the fragment shader, `triangle_masked_alpha` (line ~334) still runs
  `edge_inside` (line ~316) on every hard edge, including for all-hard
  triangles. `edge_inside` treats a band of `EDGE_EPS = 1e-3` world units ×
  `|e|` around the edge as "on the line" and decides it by edge direction.
  For an all-hard triangle that the rasteriser already assigned a pixel to,
  a pixel centre inside the band on the "losing" side is rejected by the
  only triangle that rasterised it: a one-pixel hole. The direction rule is
  also not the rasteriser's fill rule, so mixed triangles that share a hard
  edge with an all-hard neighbour can leave holes or double-blend.

Fonts (`crates/ferrous_renderer/src/renderer_core.rs`, 3251 lines):
- `FontId2d(pub u32)` is defined in `crates/ferrous_2d/src/render/types.rs:86`.
- `register_font_2d` (line ~3109) recycles ids from `freed_font_2d_ids`
  with no generation; `free_font_2d` (line ~3158) pushes the id back. A
  stale `FontId2d` held by a caller silently draws with whatever font got
  the recycled id.
- `measure_text_2d` (line ~3225) allocates a `Vec<GlyphQuad>` per call to
  call `layout_line` (`crates/ferrous_font/src/layout.rs:93`), only to drop
  the quads.
- Sprite texture ids use the same recycle pattern
  (`freed_sprite_texture_ids`, line ~3100). Do not change the sprite id
  type in this job, but put the generational logic in one reusable type
  (phase 3) so sprites can adopt it later; report how.

Warnings and CI:
- `cargo clippy -p ferrous_renderer --no-default-features --all-targets`
  emits about 8 warnings (unused imports and variables, a dead field and a
  dead method behind the `gui` feature). The engine CI
  (`.github/workflows/ci.yml`, workflow `engine-2d-text-and-readback`) only
  lints the default features. It format-checks an explicit list of files.

Pure2D pipelines and drawing:
- `crates/ferrous_2d/src/render/pipeline.rs` has four pipeline structs
  (`SpritePipeline` line 5, `ShapePipeline` line 149, `Text2dPipeline`
  line 271, `GridPipeline` line 418), each with a `new` and a
  `new_with_cache` that repeat the same camera bind-group layout, pipeline
  layout and `RenderPipelineDescriptor` boilerplate. All use
  `wgpu::BlendState::ALPHA_BLENDING` (straight alpha).
- Fragment shaders return straight colour:
  `vec4(in.color.rgb, in.color.a * alpha)` in `shape_2d.wgsl` (lines ~489,
  ~566, ~591, ~674), `tex_color * in.color` in `sprite_2d.wgsl` (line ~76),
  `vec4(in.color.rgb, in.color.a * opacity)` in `text_2d.wgsl` (line ~108).
  Sprite textures are uploaded as straight-alpha RGBA8 sRGB.
- One z-sorted stream: `DrawCommand2d { Shape(usize), Sprite(u32, usize),
  Text(usize) }` (`crates/ferrous_2d/src/render/batch.rs:111`), built in
  `render_internal` (`renderer_core.rs:1770`, draw list built around
  lines 2003–2029) and drawn in painter order, coalescing consecutive
  commands of the same kind and bind group.
- The target is `*Srgb`, so blending happens in linear light. Colours are
  linear RGBA. Pure2D world: origin at the centre, Y up, 1 unit = 1 logical
  pixel.
- Other 2D consumers (GUI via `ferrous_ui_render`, editor) use their own
  pipelines; `ALPHA_BLENDING` in `ferrous_renderer/src/pipeline/*` and
  `renderer_core.rs:456–501` belongs to 3D/GUI paths. Do not change those.

Existing tests: `crates/ferrous_renderer/tests/{pure2d_headless.rs,
headless_readback.rs, headless_text.rs, sprite_texture_recycle.rs}`, bench
`crates/ferrous_renderer/benches/pure2d_headless.rs` (divan with
`AllocProfiler`).

---

## 2. Phases

### Phase 1 — baseline and design (orchestrator, no code)

1. Read every file cited above in full where it matters (headless.rs,
   shape_2d.wgsl, the Pure2D part of renderer_core.rs, pipeline.rs,
   batch.rs, layout.rs, the four test files, the bench, ci.yml).
2. Run the gate (section 4) on the untouched branch. Record every result and
   the warning counts of `ferrous_font`, `ferrous_2d` and `ferrous_renderer`
   with and without default features.
3. Write the design into your final report (not into a file):
   - the readback state machine (slot states, transitions, error recovery);
   - the hard-edge coverage rule for all-hard and mixed triangles and why it
     has no holes and no double coverage;
   - the generational handle type;
   - the premultiplied-alpha contract (inputs, shader outputs, blend states,
     sprite textures);
   - the blend-mode set and the exact `wgpu::BlendState` of each;
   - the layer model (API, ordering, texture format, pool, nesting).
   Then launch phases 2, 3 and 4.

### Phase 2 — overlapping readback ring (subagent R)

Owner: `crates/ferrous_renderer/src/headless.rs` (readback parts only),
`crates/ferrous_renderer/tests/headless_readback.rs`, new
`crates/ferrous_renderer/tests/headless_ring.rs`, and the bench.

1. One private helper does "wait for map, check size, copy rows without
   padding, unmap" for every readback path (`read_rgba8_into`,
   `read_pixel_into`, ring take). No copy of that logic remains.
2. Slot state machine with `N` slots (default 3, configurable at harness
   creation through a constructor option or a builder method; minimum 2):
   `Free → Copying (copy submitted, map_async requested in the same call)
   → Ready (map callback succeeded) | Failed (callback error) → Free`.
   `submit_readback` records the copy, submits it and calls `map_async`
   immediately on that slot. `take_readback` polls with
   `Maintain::Wait` only for the oldest slot (use
   `Maintain::WaitForSubmissionIndex` with the submission index returned by
   `queue.submit`), copies it out, unmaps, frees it.
3. `render_frame` stops blocking: it submits and returns. Keep a
   `render_frame_blocking` (or a flag on `FrameReport` timing) for callers
   that need the old "GPU done" timing semantics; update the bench and
   tests that measured GPU time to use it. Document the change.
4. Error recovery: a failed map frees its slot (unmap if needed), is popped
   from the queue, and returns `ReadbackError::MapFailed` once with the
   frame index of the lost frame; the next take returns the next frame.
   Add `ReadbackError::Busy` documentation: what the caller should do (take
   one, then submit).
5. Exporter-friendly loop: add
   `pub fn readback_frame(&mut self, out: &mut Vec<u8>) -> Result<Option<u64>, ReadbackError>`
   or an equivalent documented pattern that, called once per frame after
   `render_frame`, submits this frame and returns the oldest ready frame
   (with its frame index) when the ring is full; plus
   `pub fn flush_readbacks(&mut self, f: impl FnMut(u64, &[u8]))` (or a
   draining iterator over a reusable buffer) to drain at the end. Frame
   indices are assigned by the harness at submit, starting at 0.
6. Tests (`headless_ring.rs`, GPU tests skip cleanly):
   - frames come out in submission order with the right content for 10
     frames of distinct solid colours, with ring sizes 2, 3 and 4;
   - `submit` with every slot busy returns `Busy` and changes nothing;
   - injected map failure: add a test-only hook
     (`#[cfg(any(test, feature = "test-hooks"))]` is not allowed because it
     changes features; use a `#[doc(hidden)] pub fn inject_map_failure_for_test(&mut self)`
     documented as test-only) that makes the next completed map report an
     error; assert one `MapFailed`, then the following frames are correct
     and no slot leaks (`pending_readbacks()` returns to 0);
   - overlap: with the ring, `render_frame` returns before the GPU finished
     (assert that `submit_readback` does not call `Maintain::Wait`, through
     a debug counter of blocking polls exposed like `staging_allocations`);
   - steady state allocates nothing: extend the bench with the ring loop
     and, in a test, check that staging allocations stay constant over 100
     frames.

### Phase 3 — generational font handles and allocation-free measurement (subagent F)

Owner: `crates/ferrous_2d/src/render/types.rs` (`FontId2d` only), a new
module `crates/ferrous_2d/src/handle.rs` (or `crates/ferrous_core` if a
handle type already exists there: search first and reuse it), the font
section of `renderer_core.rs` (lines ~3104–3245 only),
`crates/ferrous_font/src/layout.rs`, `crates/ferrous_font/src/atlas.rs`
(layout entry points only), `crates/ferrous_renderer/tests/headless_text.rs`.

1. Search the workspace for an existing generational handle/slot-map type
   (`grep -rn "generation" crates`). If one exists, reuse it. Otherwise add
   one small private-field `Handle { index: u32, generation: u32 }` plus a
   `SlotRegistry<T>` (insert, get, get_mut, remove, len, iter) in
   `ferrous_2d`, with unit tests (stale handle after remove and re-insert
   is rejected; generations wrap safely; removing twice is a no-op).
2. `FontId2d` becomes a generational handle. Keep `FontId2d` as the public
   type name; its fields become private with `index()`/`generation()`
   accessors. Update every in-repo caller (bench, tests, headless).
   `draw_text_2d` and `measure_text_2d` with a stale id log one
   `log::warn!` per call and do nothing / return `None`. Replace
   `fonts_2d: HashMap` + `freed_font_2d_ids` + `next_font_2d_id` with the
   registry. Free must still drop that font's pending glyphs.
3. `ferrous_font`: add `pub fn measure_line(font: LayoutFont<'_>, text, size_px, align) -> LineMetrics`
   that computes exactly what `layout_line` returns without producing quads,
   sharing the same pen/kerning loop (one generic inner function with a
   per-glyph callback or a sink trait; no duplicated loop). Add
   `FontAtlas::measure_line`. `measure_text_2d` uses it and allocates
   nothing.
4. Tests: `measure_line` equals `layout_line`'s metrics for 20 strings
   (ASCII, kerning pairs, spaces, missing glyphs, empty) at 3 sizes and all
   alignments; a counting test or the bench shows zero allocations for
   `measure_text_2d`; stale font id after free and re-register does not
   draw (render, compare pixels with an empty frame) and returns `None`
   from measure.

### Phase 4 — hard-edge coverage without holes (subagent E)

Owner: `assets/shaders/shape_2d.wgsl` (triangle coverage only), the
triangle section of `vs_main`, and a new test
`crates/ferrous_renderer/tests/pure2d_hard_edges.rs`.

1. All-hard triangles (`mask == 0`): the fragment shader does no edge test
   at all; the rasteriser's fill rule is the coverage. Remove the band
   logic for this case.
2. Mixed triangles: a hard edge of a mixed triangle must partition pixels
   exactly as the rasteriser would partition the same edge of an all-hard
   neighbour. Implement this by evaluating hard edges in framebuffer space
   from `@builtin(position)` (pixel centres are exact half-integers) against
   the edge endpoints projected to framebuffer space and passed `flat`
   from the vertex shader, using the top-left fill rule in the same
   orientation the rasteriser uses after the viewport Y flip. Snap the
   endpoints to the rasteriser's sub-pixel precision (8 fractional bits is
   the D3D12/Vulkan/Metal minimum; document the value and why). If this
   cannot be made exact, stop and report the measured mismatch instead of
   adding a tolerance band.
3. Keep `EDGE_EPS` only if it is still used elsewhere; otherwise delete it
   and its comment.
4. Seeded test (`pure2d_hard_edges.rs`, GPU, skip cleanly): for 50 seeds,
   generate a random convex polygon or a random grid of points, triangulate
   it (fan or simple grid triangulation written in the test), draw every
   triangle all-hard with colour `(1,1,1,0.5)` over black, read back, and
   assert every pixel whose centre lies strictly inside the polygon (computed
   in the test in f64) has exactly the single-coverage value (no hole = 0,
   no double = 0.75 coverage). Repeat with a random subset of outer
   (boundary) edges marked anti-aliased so mixed triangles share hard
   interior edges with all-hard ones. Include rotated and non-integer
   offsets. Print the seed and the first failing pixel on failure. The
   test must fail on the current shader (verify it does before the fix and
   say so in the report).

Phase gate for 2+3+4: section 4, with the existing tests unchanged in
behaviour (any changed assertion must be justified in the report).

### Phase 5 — premultiplied alpha end to end (orchestrator)

Owners: `crates/ferrous_2d/src/render/pipeline.rs`, the three 2D shaders'
fragment outputs, sprite texture upload in `renderer_core.rs`, docs.

1. Refactor `pipeline.rs` first: one private builder (for example
   `fn build_2d_pipeline(device, desc: Pipeline2dDesc) -> wgpu::RenderPipeline`
   plus one shared camera bind-group-layout constructor) used by all four
   pipelines; each `new` delegates to `new_with_cache`. No repeated
   descriptor boilerplate remains. Behaviour unchanged (run tests before
   the next step).
2. All Pure2D fragment shaders output premultiplied colour
   (`rgb * a, a`) and all Pure2D pipelines use
   `wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING`. Over an opaque
   destination this is mathematically identical to straight alpha: every
   existing pixel test must still pass within ±1 per channel; document any
   test whose tolerance had to change.
3. Sprites: texture data stays straight RGBA8 sRGB on upload (public API
   unchanged); the sprite shader premultiplies after sampling and after
   multiplying by the tint. Document that bilinear filtering of straight
   textures has dark fringes and add an opt-in
   `register_texture_rgba8_premultiplied` (or a flag) for callers that
   upload premultiplied data, so Motioner can use it later.
4. Instance colours stay straight linear RGBA in the public API (no caller
   changes). Write this contract in rustdoc on `ShapeInstance`,
   `SpriteInstance`, `TextInstance` and in `crates/ferrous_2d/docs/`.
5. Tests: a 50 % translucent red rect over a 50 % translucent blue rect on
   a transparent clear colour gives the correct premultiplied result in the
   readback (alpha channel and colour, ± 2), which straight alpha gets
   wrong on a non-opaque target; the old straight-alpha scenes are
   unchanged.

### Phase 6 — blend modes per draw (orchestrator)

Owners: `crates/ferrous_2d/src/render/{types.rs,batch.rs,pipeline.rs,renderer.rs}`,
the Pure2D draw-list part of `renderer_core.rs`, headless draw methods.

1. `pub enum BlendMode2d { Normal, Add, Multiply, Screen }`
   (`#[non_exhaustive]`, `Default = Normal`) with documented formulas in
   premultiplied linear light:
   - Normal: `src + dst·(1−αs)` (`PREMULTIPLIED_ALPHA_BLENDING`);
   - Add: colour `src + dst`, alpha `αs + αd·(1−αs)`;
   - Screen: colour `src + dst·(1−src)` (`One`, `OneMinusSrc`), alpha as
     Normal;
   - Multiply: colour `src·dst + dst·(1−αs)` (`Dst`, `OneMinusSrcAlpha`),
     alpha as Normal. Document that this is exact over an opaque backdrop
     and omits the `src·(1−αd)` term over a transparent one; layers
     (phase 7) composite onto an opaque-or-not backdrop with the same
     state, so state the limitation once.
   Advanced modes (overlay, soft light, colour dodge, hue…) need the
   backdrop in a shader: out of scope; write the design in the report.
2. Each pipeline kind gets one `wgpu::RenderPipeline` per blend mode,
   created lazily or at startup through the phase-5 builder (no duplicated
   descriptors). Blend mode is a per-draw property: add it to the draw
   command (not to the instance struct, to keep the GPU layout unchanged),
   e.g. `DrawCommand2d` carries it or a parallel array does; coalescing
   breaks only when kind, bind group or blend mode change.
3. API: additive methods such as `draw_shape_blend(instance, mode)` or a
   `set_blend_mode_2d(mode)` state that applies to subsequent draws until
   changed and resets each frame. Choose one, justify it, and mirror it on
   the harness. Existing calls keep `Normal`.
4. Tests: each mode over a known backdrop gives the formula's value
   (± 2 per channel) for a 2×2 grid of colours; changing the mode
   mid-frame breaks a batch (draw-call counter); z order is respected
   across modes; determinism (same frame twice, identical bytes).

### Phase 7 — offscreen layers (orchestrator)

Owners: Pure2D part of `renderer_core.rs`, `crates/ferrous_2d/src/render/**`,
a new shader `assets/shaders/composite_2d.wgsl`, headless layer methods,
new test `crates/ferrous_renderer/tests/pure2d_layers.rs`.

1. API (Pure2D renderer, mirrored on the harness):
   `pub fn push_layer_2d(&mut self, desc: Layer2d)` and
   `pub fn pop_layer_2d(&mut self) -> Result<(), LayerError>` where
   `Layer2d { opacity: f32, blend: BlendMode2d, depth: f32 }` (depth places
   the whole layer in the parent's z order). Draws between push and pop go
   into the layer. Nesting is supported (document the maximum depth, at
   least 8; deeper pushes return a typed error at pop or are rejected at
   push, your choice, documented). Unbalanced push at frame end is a typed
   error reported by `render_frame`/`render_to_view` through a counter or
   result, never a panic; unbalanced layers are closed automatically and
   logged.
2. Semantics: the layer's content is z-sorted among itself only; the layer
   as a whole is one `DrawCommand2d::Layer(index)` in its parent stream at
   `depth`. Rendering: each layer renders into an offscreen texture cleared
   to transparent, then a full-target (or bounds-limited, optional)
   composite quad draws it into the parent with
   `colour × opacity` (premultiplied, so opacity scales all four channels)
   and the layer's blend mode, through the phase-6 pipelines' blend states.
   Group opacity must differ from per-child opacity: two overlapping 100 %
   opaque children in a 50 % layer show no internal overlap.
3. Texture format and pool: layer textures use `Rgba16Float` (linear,
   premultiplied, more precision than 8-bit sRGB) with the target's size and
   sample count 1; check the format supports `RENDER_ATTACHMENT |
   TEXTURE_BINDING` and blending on the adapter, fall back to the target
   format if not, and report which path ran on this machine. Textures come
   from a pool keyed by (size, format) and are reused across frames: zero
   texture creation in steady state (debug counter + test). Resize of the
   target invalidates the pool.
4. The sprite, shape and text pipelines must exist for both the target
   format and the layer format (pipeline cache keyed by (kind, blend,
   format); build lazily through the phase-5 builder).
5. Encoding order: render inner layers first (post-order), each in its own
   render pass, then the parent pass. Document the pass count per frame and
   keep the no-layer path exactly as fast as before (one pass, no extra
   textures; check with the bench).
6. Tests (`pure2d_layers.rs`, GPU, skip cleanly):
   - group opacity: two overlapping opaque red/blue rects in a 0.5 layer
     over white: the overlap shows blue at 50 % over white, not a mix with
     red;
   - layer depth ordering: rect(z=1), layer(depth=2){rect}, rect(z=3);
   - nested layers with opacities 0.5 × 0.5 give 0.25 (± 2);
   - layer blend mode Multiply over a backdrop matches the phase-6 formula;
   - text inside a layer renders the same as outside it at opacity 1
     (± 1, within the format precision);
   - empty layer draws nothing and costs no texture after warm-up;
   - unbalanced push/pop gives the typed error and the frame still renders;
   - pool reuse: 100 frames with 3 layers create textures only in frame 1;
   - determinism: same frame twice, identical bytes.

### Phase 8 — zero warnings in both feature sets, CI (orchestrator)

1. Zero compiler and clippy warnings in `ferrous_font`, `ferrous_2d` and
   `ferrous_renderer` both with default features and with
   `--no-default-features`. Fix causes (gate imports and items behind
   `#[cfg(feature = "gui")]` where they belong, delete dead code); any
   `#[allow]` is item-level with a one-line justification.
2. Update `.github/workflows/ci.yml`: add the `--no-default-features`
   clippy and test runs for `ferrous_renderer`
   (`cargo test -p ferrous_renderer --no-default-features --tests`), and
   add every file this job created or edited to the rustfmt list. Keep the
   wasm check.
3. `cargo check --workspace --all-targets` and
   `cargo check -p ferrous_web --target wasm32-unknown-unknown` pass.

### Phase 9 — strict review, docs, final gate (orchestrator)

1. Review the whole diff against `32aea8a` as a strict reviewer:
   duplication (pipeline builders, readback helpers, layout loops), rustdoc
   on every public item, typed errors, panics on runtime paths, per-frame
   allocations, `log` instead of prints, tests that would pass without the
   code (for each new test, state in the report why it fails without the
   change). Fix findings.
2. Docs: update `crates/ferrous_2d/docs/` (premultiplied contract, blend
   modes, layers, ordering) and the readback section of the renderer docs
   (state machine, exporter loop example in rustdoc that compiles as a
   doctest with `no_run`).
3. Run the final gate.

---

## 3. Acceptance criteria

- Ring readback overlaps GPU work, frees a failed slot and reports the lost
  frame once; one shared map/copy helper; zero allocations per frame in
  steady state.
- No holes and no double coverage on seeded all-hard and mixed
  triangulations; the new test fails on the old shader.
- `FontId2d` is generational; stale ids are rejected with a warning; text
  measurement allocates nothing; layout and measurement share one loop.
- All Pure2D pipelines are premultiplied; existing pixel tests unchanged
  within ±1; one pipeline builder, no repeated descriptors.
- Blend modes Normal/Add/Multiply/Screen per draw, with tested formulas.
- Nested offscreen layers with group opacity and blend, pooled textures,
  no extra passes or textures when no layer is used.
- Zero warnings with and without default features; CI lints and tests both.
- No new dependencies, no new `unsafe`, rustdoc on every new public item,
  every consumer and the wasm target still build.

## 4. Gate (end of phases 1, 2+3+4, 5, 6, 7, 8, 9; `CARGO_INCREMENTAL=0`)

- `rustfmt --edition 2021 --config skip_children=true --check <every file you created or edited>`
- `cargo clippy -p ferrous_font -p ferrous_2d -p ferrous_renderer --all-targets -- -D warnings`
  (before phase 8: record counts and make sure you added none)
- `cargo clippy -p ferrous_renderer --no-default-features --all-targets -- -D warnings`
  (from phase 8 on)
- `cargo test -p ferrous_font -p ferrous_2d -p ferrous_renderer`
- `cargo test -p ferrous_renderer --no-default-features --tests`
- `cargo test -p ferrous_renderer --release --test pure2d_headless`
- `cargo check --workspace --all-targets`
- `cargo check -p ferrous_web --target wasm32-unknown-unknown`
- `cargo doc -p ferrous_font -p ferrous_2d -p ferrous_renderer --no-deps`
  with no new rustdoc warnings
- Free space on C: (report it; stop below 8 GB)

## 5. Stop conditions

Stop and report instead of improvising if: a phase gate fails twice after a
fix attempt; exact hard-edge partitioning cannot be achieved without a
tolerance band (phase 4, step 2); `Rgba16Float` and the fallback both fail
to blend on this adapter; a change would break a consumer in a way you
cannot fix inside this repository; GUI or 3D output would change; or free
space on C: drops below 8 GB.

## 6. Final report

1. Phase summaries (which subagent did what).
2. Files created/changed, one line each.
3. Public API additions and changes (exact signatures); every breaking
   change and the callers you updated (expect at least `FontId2d` and
   `render_frame` timing).
4. Design decisions from phase 1 as implemented, and every deviation from
   this prompt with the reason.
5. Tests added per crate, which skip without a GPU, and for each one why it
   fails without the change (state explicitly that the hard-edge test failed
   on the old shader).
6. Bench numbers before/after: frame time and allocations for the
   synchronous readback, the ring, and a frame with 3 layers.
7. Warning counts before/after per crate, with and without default
   features.
8. Gate table for the final run.
9. Layer texture format used on this machine, pass count per frame, and the
   design for advanced blend modes.
10. Free space on C: at the end and open questions for the reviewer.
