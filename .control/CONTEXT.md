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
  motion blur, helpers; D-0037/D-0038). Later phases have their backlog tasks.
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
  premultiplied linear, weighted add, normalised resolve. The clone has it;
  the pin does not yet, so Motioner gates the render hook behind the
  `engine-accumulation` feature (off by default, typed `Unsupported`).
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
  static reference). `motioner_render/tests/golden_timing.rs` pins them; the
  render comparison and the on/off reference check need
  `--features engine-accumulation`.

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

- Pin `e0cc574` on `feat/pure2d-text-and-readback` (committed and pushed):
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
- **Uncommitted engine work (Phase 9)**: `ferrous_2d/src/render/accumulation.rs`
  (`Accumulation2d`, `AccumulationError`, `ACCUMULATION_FORMAT`),
  `assets/shaders/accumulation_2d.wgsl`, mirrors on `Renderer`,
  `Pure2dHarness`, `RenderContext`; `register_texture_2d` with
  `mipmaps:false` builds level 0 only (`build_level_zero`); tests
  `pure2d_accumulation.rs` and the new `pure2d_images.rs` case; docs
  `crates/ferrous_2d/docs/accumulation.md`. The reviewer must commit this and
  bump the pin before `engine-accumulation` can be turned on.
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

Phase 9 (timing): engine `Accumulation2d` + `build_level_zero` + tests/docs;
Motioner `BezierEase`/new eases/`evaluate_fractional`/`render_frame`/
`stagger`/`wiggle`, overrides/bake/query round-trip, editor ease grid and
`motion_blur_preview`, `examples/timing.motion`, two goldens, preview and
editor screenshots in `out/ui/`. Part A: image layer names (file stem) and
row truncation, one label table for inspector/timeline, preview screenshots
with real textures, engine level-0-only mipmaps. Gate green; feature-on
clippy and the motion-blur golden were generated against the clone. Next:
commit the engine clone, bump the pin, turn `engine-accumulation` on, then
Phase 10.
