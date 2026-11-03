---
actualizado: 2026-11-03
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0035.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor. Conversation with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- Prompt 10 is a 12-phase job. **Phases 1-8 are done** (phase 8 images and
  the phase 7 leftovers landed at pin e0cc574). **Phase 9 is done** (easing,
  motion blur, helpers; D-0037/D-0038). **Phase 10 is done** (names, outline
  filter/collapse/solo/shy, graph editor, 1000-layer performance, live blur
  preview; D-0044) and its review fixes are done (Part A: engine lines,
  in-place blur texture, cached chrome, full name paths). **Phase 11 is done**
  (`examples/showcase_pro.motion`, six goldens, MP4 and contact sheet).
  Phase 12 remains.
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
- **Vector paths (D-0029)**, **gradients (D-0030)**, **blend modes (D-0031)**,
  **layer effects (D-0032)**, **masks (D-0033)** as before.
- **Typography (D-0034)**: Inter 400/600/700 in `assets/fonts/`; `FontSet`;
  `TextMetrics` in core + `CpuTextMetrics` in render; `text_width`/`text_bounds`;
  `node.animate_chars(...)`; the inspector Text section. **Leftovers done**:
  `text_anim_add`/`text_anim_remove`, the `text_animators` override,
  `node.clear_chars()`, bake emitting `animate_chars`, the inspector Animators
  section, `query` reporting the list, and a bake round-trip test.
- **Images (D-0035)**: `image(source, width=None, height=None, fit=...)` with
  `contain|cover|fill`; PNG decoded by `motioner_io::ImageStore` (palette,
  grey and 16-bit expanded to RGBA8; gAMA/iCCP ignored); SHA-256 content keys;
  `Document::resolve_images` fills key/natural size; the plan carries key,
  destination rect and `uv_rect`; the renderer registers mipmapped textures on
  first use and reuses them by key; missing files fail `still`/`render` with
  `UnresolvedImage` and draw a placeholder in the preview. `examples/images.motion`
  and two goldens (`images_fit_f030.png`, `images_composite_f150.png`).
- **Timing (D-0037)**: `Ease` keeps `Copy + Eq + Hash`; `BezierEase` quantises
  the four control values to 1e-6 as `i32`. New presets `hold`,
  `elastic_out`, `bounce_out`, `expo_in_out`, `sine_in_out`; `bezier(...)` is
  parsed in MDF, the overrides file and commands. The solver is 8
  Newton–Raphson steps then 64 bisection steps. `comp(..., motion_blur=True,
  shutter=180, samples=8)`; `Evaluator::evaluate_fractional` samples fractional
  times; `render_frame` averages sub-frames through the engine accumulation
  API. `stagger` and `wiggle` emit ordinary keys (SplitMix64 for `wiggle`).
  Editor: ease grid includes the new presets; a bezier key shows its four
  numbers; `motion_blur_preview(on)` and a top-bar indicator.
- **Engine accumulation (D-0038)**: `Accumulation2d` in `ferrous_2d`, mirrored
  on `Renderer`, `Pure2dHarness` and `RenderContext`; `Rgba16Float`,
  premultiplied linear, weighted add, normalised resolve. The pin now has it
  and `engine-accumulation` is gone; `still`/`render` really blur.
- **Editor quality of life (D-0044)**: `Node::name` + binding-derived defaults
  (`motioner_dsl::default_name`), `name` override, bake and `query`;
  `filter_layers`/collapse/`solo`/`shy`/`show_shy` (preview only);
  `graph_open`/`graph_view`/`move_key_value` and editable bezier fields;
  virtualised timeline rows + cached `OutlineRow::visible_runs`; optional
  `alloc-count` feature; live motion-blur preview (`app/blur.rs`) through
  `FrameRenderer::render_preview_frame`, 4 samples playing / full paused.
- **Phase 10 review fixes**: graph curve/stems are real anti-aliased
  `Painter::line` segments sampled per screen column (engine `LINE_BIT`);
  `graph_open` fits the key range and the caption lives above the plot.
  `BlurPreview` rewrites its sprite texture in place
  (`Renderer::update_sprite_texture`) and `FrameRenderer` reuses one
  `Evaluator`/`RenderPlan`. Timeline values, labels, identity and header/ruler
  strings are cached; alloc reading is per-thread and always shown with
  `alloc-count`. Default names use the full loop/call path and a `HashSet`.
- **Measurement fix**: `ferrous_font::LayoutData` (engine clone) is the CPU
  table source; `FontAtlas` builds through it. `CpuTextMetrics` caches one
  `LayoutData` per face and calls `measure_block`; `measure_parser` is gone.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...] [-Select ROW]`.
- **GPU tests**: `motioner_testkit::gpu::lock()` serializes them; the normal
  `cargo test --workspace` gate is green without `--test-threads=1`.

## Goldens

- `hello_f120.png` (byte-identical), `blend_f30.png`,
  `gradients_f30/f105.png`, effect/mask/path/animatable stills.
- Typography (D-0034): `typography_tracking_f30.png`,
  `typography_lines_f150.png`, `typography_chars_f260.png`.
- Images (D-0035): `images_fit_f030.png` (fit modes),
  `images_composite_f150.png` (alpha badge, shadow, mask, multiply).
  `motioner_render/tests/golden_images.rs` pins their SHA-256, checks overlap
  pixels and compares the render against the pin. The badge asset is
  256 px drawn at 160 px, so the example goes through the mip chain.
- Timing (D-0037): `timing_eases_f030.png` (hold dot at its old position,
  elastic overshoot) and `timing_motion_blur_f400.png` (0.75-coverage streak,
  static reference). `motioner_render/tests/golden_timing.rs` pins them; both
  run without a feature after the pin bump.
- Showcase Pro (Phase 11): `showcase_pro_f180/f330/f600/f810/f1086/f1320.png`
  (1920x1080) pinned by `motioner_render/tests/golden_showcase_pro.rs`.
  Deliverables: `out/showcase_pro.mp4` and `out/showcase_pro_sheet.png`.
- `hello_f120.png` stays byte-identical after Phases 10-11:
  SHA-256 `C2173BADD7F23933461473C5D682E40FC9FC1DCBD4EBBE10A1C7C1C7DE5CF15C`.

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
  with a `\n` → `\r\n` pass. Motioner files are LF (`.gitattributes eol=lf`).
- Disk: keep C: above 5 GB; clean `target` below 6 GB. A full `cargo clean`
  after the phase 8 build freed 30 GB; the linker failed with LNK1318 at 0
  bytes free.
- Clippy pedantic: `too_many_lines`, `too_many_arguments`, `option_option`,
  `float_cmp`, `implicit_clone`, `doc_markdown`, `must_use`, `match_same_arms`,
  `single_match_else` and `arc_with_non_send_sync` are easy hits.
- Inspector order: Composite → Effects → Mask → Transform → Text → Animators →
  Image.
- `motioner_ui` must NOT depend on `motioner_io` (boundary check). The image
  loader enters the UI as `Rc<dyn motioner_core::ImageResolver>` through
  `PreviewOptions.image_resolver`; the app creates `CachedImages`.

## FerrousEngine state

- Pin `7e95367` on `feat/pure2d-text-and-readback` (committed and pushed):
  temporal accumulation (`Accumulation2d`) and level-zero texture upload.
  - `ferrous_font/src/layout_data.rs`: `LayoutData::from_bytes/from_parser`,
    `layout_font`, `measure_*`/`layout_*`, `into_parts`. `FontAtlas::new` builds
    through it (bit-for-bit measurement test in
    `ferrous_renderer/tests/pure2d_text_layout.rs`, Roboto and a kern-stripped
    Roboto).
  - `ferrous_2d/src/render/texture.rs`: `TextureOptions2d`, `TextureFilter2d`,
    `TextureError`, `MipChain`, `build_mip_chain` (linear premultiplied box
    filter, exact area weights for odd sizes).
  - `Renderer::register_texture_2d`, `texture_bytes_2d`, `store_sprite_texture`;
    `Pure2dHarness` and `RenderContext` mirrors. `mipmaps:false` uploads level
    0 only (fixed during the 4K manual check).
  - Tests: `ferrous_renderer/tests/pure2d_images.rs` (9: checker mip, odd
    chain, premultiplied texel, `uv_rect`, layer opacity, masked layer, drop
    shadow alpha, budget, oversized error).
  - Docs: `crates/ferrous_2d/docs/images.md` (linked from the README and
    `mkdocs.yml`), `ferrous_font/docs/README.md`.
- **Clone commit `6a30e7d` (Phase 10 review fixes, not yet in Motioner history)**:
  `GuiBatch::line`/`polyline` with `LINE_BIT (1 << 9, shader 0x200)`,
  round-capped anti-aliased segments in `gui.wgsl`/`gui_web.wgsl`, docs in
  `ferrous_ui_render/docs/RENDER.md` and two batch tests;
  `Renderer::update_sprite_texture` with `TextureUpdateError` (in-place
  `queue.write_texture` for same-size frames), docs in
  `ferrous_2d/docs/images.md` and the `pure2d_images` test. Motioner's
  `engine/ferrous` working tree is checked out at `6a30e7d`; the reviewer must
  commit the pin.
- **Flaky GPU tests**: the first parallel run after a build can fail with
  `wgpu error: Validation Error` / `create_buffer ... 'Particles Storage
  Buffer' ... Not enough memory left` or FXC out-of-memory. It is device
  contention; `tests/common/mod.rs` has a process-wide `gpu::lock`.

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
- Animators section: one row per animator with a Del tile, plus Fade/Rise/
  Colour add tiles (`text_anim_add`/`text_anim_remove`).
- Image section: Source (text), Natural size (read only), Width/Height with
  stopwatches, Fit cycle. Image layers use a picture icon in layers/outline.

## Last relevant session

Phase 10 review fixes (T-UI-0009): engine line primitive and in-place sprite
update (clone `6a30e7d`); anti-aliased graph curve/stems sampled per screen
column, key-fit view and a caption outside the plot; blur preview at 28 ms
paused full / 9 ms playing 4 samples at both 960x540 and 1080p (the old
"UI 1 fps" was the one-second first paused render, not idle redraw); cached
values/labels/chrome with a per-thread alloc reading of 9/frame (engine
`GuiBatch`, D-0044 amended); names use the full loop/call path.

Phase 11 (T-APP-0003): `examples/showcase_pro.motion` (24 s 1080p60, 80
nodes, six scenes), six goldens pinned by
`motioner_render/tests/golden_showcase_pro.rs`, `out/showcase_pro.mp4`
(1440 frames) and `out/showcase_pro_sheet.png`. Captures
`out/ui/p10-11-graph-fixed.png`, `p10-12/13-alloc-*`, `fps-960/1080-*`.
Gate green; `hello_f120.png` still `C2173BAD...` byte-identical. Next:
Phase 12 (release numbers, docs, final report). Reviewer must commit the
engine clone and bump the pin.
