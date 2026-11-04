---
actualizado: 2026-11-03
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## Prompt 11 status (2026-11-04, partial at a phase boundary)

Fases 0–4 completas; **fases 5 (audio), 6 (vídeo) y 7 (demo/docs/release)
pendientes** para la próxima ejecución. Informe:
`out/reports/PHASE-11-PARTIAL-REPORT.md`.

- D-0045 “Project folder v1”: `<name>.motioner/` con `project.motion` (MDF),
  `compositions/`, `assets/<sha256>.<ext>`, `cache/`, `backups/`, `traces/`,
  `editor.motion`; registro con clave estable; MDF resuelve por clave o
  nombre original; estado de app en `%APPDATA%\Motioner\app.motion`; escrituras
  atómicas y migraciones explícitas.
- D-0046: `rfd` solo en `motioner_app` (selector de carpeta), detrás de
  `ProjectHost`.
- `motioner_io::project` (manifiesto, `EditorState`, `AppState`, validación,
  `clean_temp`) y `motioner_io::assets` (`ProjectImages`, import/dedupe/
  rename/remove, fuentes).
- CLI: `new`, `import <project> <file>...`, `backup <project>`, `migrate`,
  `info`; todos los comandos aceptan carpeta/manifiesto/composición.
- Pantalla de inicio + menú File + `ProjectHost` (`motioner_app::host::Host`);
  `Editor::blank`/`replace_session`; comandos `new_project`, `open_project`,
  `close_project`, `save`, `save_as`.
- Panel **Layers/Assets** con lista/rejilla virtualizada, import (menú, `+`,
  doble clic/arrastre), conversión ffmpeg a `cache/`, picos de audio, CLEAN y
  place; **0 alloc/frame** (`out/ui/p11-05-alloc.png`).
- Autosave: overrides 300 ms; `motioner_io::backups` (snapshot cada 120 s,
  20/200 MB, poda) y lock `cache/session.lock` con heartbeat; banner de
  recuperación con Restore/Dismiss.
- Comandos nuevos de assets: `import_asset`, `remove_asset`, `rename_asset`,
  `place_asset`, `remove_unused_assets`, `restore_backup`,
  `dismiss_recovery`; documentados en `docs/agents.md`.
- Capturas: `p11-01-start`, `p11-02-assets`, `p11-03-placed`, `p11-04-recovery`,
  `p11-05-alloc` en `out/ui/`.

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
- Prompt 10 is a 12-phase job and **all 12 phases are done**. Phases 1-8
  landed by pin e0cc574; phase 9 added easing/motion blur/helpers (D-0037/
  D-0038); phase 10 added names, outline filter/collapse/solo/shy, the graph
  editor, 1000-layer performance and the live blur preview (D-0044), plus its
  review fixes (engine lines, in-place blur texture, cached chrome). Phase 11
  added `examples/showcase_pro.motion`, six goldens, the MP4 and the contact
  sheet; the Phase 11 review (T-PERF-0003) fixed the radial-gradient radius,
  did the Showcase Pro art pass and landed the engine batch recycle. Phase 12
  (T-PERF-0002) recorded release numbers, updated the docs, closed/backlogged
  tasks and delivered the Spanish report.
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

## Phase 12 release numbers (reference machine)

- Release build once: `cargo build --release -p motioner_app -j 2` with
  `CARGO_INCREMENTAL=0`, 5m56s.
- `showcase.motion`: 720 frames in 8.079 s, **7.778 ms/frame**
  render+readback, 3.065 ms/frame encode.
- `showcase_pro.motion`: 1440 frames in 26.621 s, **16.499 ms/frame**
  render+readback (8-sample motion blur), 1.763 ms/frame encode.
- Preview (release + `alloc-count`, 1600x900, Pro reel playing): **59 UI fps**,
  eval 0.03 ms, **0 chrome allocations/frame**, canvas at a 39 % fit.
  Captures: `out/ui/p12-01-alloc.png`, `out/ui/p12-02-fps-1600.png`.
- Report (Spanish): `out/reports/PHASE-12-REPORT.md`.

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
- **Phase 11 review re-pins (2026-11-03)**: the radial fix changed
  `gradients_f30` (`7fbd4bf4…`) and `gradients_f105` (`e5d237d9…`), and the
  art pass + radial fix changed the six showcase goldens (`394e63df…`,
  `a6a227c5…`, `12c61ef0…`, `7b322626…`, `1560326b…`, `0be8f9a1…` in
  frame order). `hello_f120.png` is still byte-identical.
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
- **Pinned clone commit `6a30e7d` (Phase 10 review fixes, in Motioner history)**:
  `GuiBatch::line`/`polyline` with `LINE_BIT (1 << 9, shader 0x200)`,
  round-capped anti-aliased segments in `gui.wgsl`/`gui_web.wgsl`, docs in
  `ferrous_ui_render/docs/RENDER.md` and two batch tests;
  `Renderer::update_sprite_texture` with `TextureUpdateError` (in-place
  `queue.write_texture` for same-size frames), docs in
  `ferrous_2d/docs/images.md` and the `pure2d_images` test.
- **Clone commit `e482b3c` (Phase 12, not yet in Motioner history)**: the
  `gui.wgsl`/`gui_web.wgsl` comment now says `0x200`; `GuiBatch::polyline`
  documents the translucent double-cap joint and
  `polyline_joints_overlap_at_the_shared_endpoint` pins the geometry;
  `FramePacket::remove`, `FrameBuilder::take_ui_batch` and
  `Renderer::take_recycled_ui_batch` let `runner/frame.rs` clear and reuse one
  `GuiBatch` instead of building a fresh one, so Motioner's counter reads
  0/frame. Engine tests for `ferrous_ui_render`, `ferrous_render_graph`,
  `ferrous_renderer` and `ferrous_engine` pass. The reviewer must commit the
  submodule pointer (working tree is checked out at `e482b3c`).
- **Clone commit `cab7035` (prompt 11, not yet in Motioner history)**:
  `FileDropEvent`/`FileDropKind`, `FerrousApp::on_file_drop`; the runner queues
  `HoveredFile`/`HoveredFileCancelled`/`DroppedFile` and delivers them in the
  frame's `AppContext`; docs in `ferrous-app-trait.md` and the README, test
  `ferrous_engine/tests/file_drop.rs`. The reviewer must push and bump the pin
  (T-APP-0005); OS drops stay behind the pin until then.
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

Phase 12 + Phase 11 review (T-PERF-0002/T-PERF-0003, session
`S-2026-11-03-005`):

- **A1**: `motioner_render::gradients::geometry` transformed the centre and
  then `centre + radius` again; the radius folded the item translation, so an
  off-centre radial ramp never reached zero alpha at the silhouette and
  additive orbs read as hard discs. It now transforms the centre and the local
  `+x` edge separately. Unit + GPU regression tests cover all seven blend
  modes; `gradients_f30/f105` and six showcase goldens re-pinned; `hello_f120`
  byte-identical.
- **A2**: Showcase Pro art pass: REVEAL optically centred from
  `text_bounds`, all captions 28-30 px and brighter, orbs dimmer and retired
  for the mask scene, light leaks as faint wide + bright core capsules, scene 1
  monogram apex joined and the halo moved outside the mark. MP4 and contact
  sheet re-rendered.
- **A3**: engine clone `e482b3c` (comment fix, polyline limitation + test,
  `GuiBatch` recycle). The editor alloc counter reads **0/frame** (was 9).
- **Phase 12**: release numbers above; docs (spec, agents, READMEs); tasks
  T-PERF-0002/0003 done; backlog T-AUDIO-0001, T-NLE-0001, T-RENDER-0014,
  T-PHYS-0001 plus the existing T-UI-0007; Spanish report at
  `out/reports/PHASE-12-REPORT.md`; C: free ~25.8 GB.
- Gate green: fmt, clippy pedantic `-D warnings`, workspace tests, rustdoc
  `-D warnings`, boundaries, machete, `pctl validate`.
- Reviewer: commit the `engine/ferrous` pointer at `e482b3c` (or the pushed
  clone head) and keep the pin bump in one commit.

## Last relevant session (prompt 11, partial)

Session `S-2026-11-03-006`. Fases 0-4 del prompt 11 implementadas y con gate
verde: D-0045/D-0046, proyecto + assets + backups/recuperación, CLI
`new/import/backup/migrate`, pantalla de inicio, menú File, panel de assets con
import/conversión/colocación, comandos y docs. `hello_f120.png` sigue
byte-idéntico (`C2173BADD7F23933461473C5D682E40FC9FC1DCBD4EBBE10A1C7C1C7DE5CF15C`).
C: libre ~19.6 GB. Engine clone `cab7035` listo para el bump (file-drop
events); Motioner mantiene los drops del SO detrás del pin. Fases 5-7
(T-AUDIO-0002, T-RENDER-0015, demo/goldens/release) quedan para la próxima
ejecución; el informe parcial está en
`out/reports/PHASE-11-PARTIAL-REPORT.md`.