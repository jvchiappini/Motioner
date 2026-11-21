---
actualizado: 2026-11-21
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## Prompt 18 status (engine performance)

Session `S-2026-11-17-001` (open). **P0-P7 done; final report written.**
Engine through **`840a8b8`** (P7, on top of `7e623a9`; the whole run
`23278a2..840a8b8` is not pushed); the Motioner submodule is checked out at
`840a8b8` and the parent pin is **not committed**. No Motioner commits. Reports:
`out/reports/PHASE-18-P0-REPORT.md` through `PHASE-18-P7-REPORT.md` and
`PHASE-18-REPORT.md` (Spanish); ledger `out/reports/phase18/ledger.md`.

P7 (2026-11-21) — guards, docs, report (six fix lanes + integration):

- **Fix 5/6, MDF compile**: `collect_assigned_later` was O(S^3) (S = statements)
  and bloated the heap; rewritten to one reverse pass per block (O(S)) and the
  origin lookups are indexed by node (O(1)). The 3,000-layer fixture compiles in
  **0.46 s / 23 MB peak** (was 28.6 s / 488 MB). Scaling test: 3,000 < 3.5x
  1,000 and < 1 s in release. heavy_ui_list 3,000 **333.0 MB private** (P6
  798.5; target <=400) and 11.8 MB above the 1,000-layer run (target <=50).
- **Fix 1/2, UI damage and scrub**: per-panel hover codes (a hit another panel
  paints leaves the key at 0), a `panel_hover` path that damages the union of
  the old/new hovered row/widget rects, and a `TimelineLive` panel for the
  playhead/readouts/ruler labels split out of the Timeline key. Hover **5.776 %**
  (target <=5.87), damage **3.3 %** (was 100 %); scrub collection **1.735 ms**
  (target <=1.8) and CPU **6.955 %** in a clean A/B (target <=7.23; the full-run
  sample read 8.031, machine state), damage 10.1 %.
- **Fix 3, segment coalescing** (engine): `GuiBatch::extend_from`/`extend` merge
  a copied segment with the previous one on the same scissor and contiguous
  ranges, with a `layer_start` barrier. heavy_ui **23 draws / 14 segments** (was
  62/38) and binds 18/20/20 (P5 20/22/22); collection 4.240 ms in a clean A/B
  (P6 4.688). Draws are 1 over the P5 22; binds below.
- **Fix 4, eval cache**: a testkit test evaluates 45 fixture documents
  (~7-8k frames) cached vs uncached and requires equal plans, plus overrides and
  a revision bump; a session test asserts every successful mutation bumps
  `generation` exactly once and failed/persistence/frame paths do not. No key
  bug found.
- **Fix 7, playback**: 4K30 proxy scenario added (0 dropped, 27.8 fps, private
  406 MB); decode-ahead hit rate **95.0 %** at 1080p60 and **89.6 %** at 4K30;
  `memory-owners` per-owner line (document/plan/panel/video bytes).
- **Fix 8, offscreen present** (engine): `AppConfig::persistent_ui` +
  break-even and a `UiPresenter` that reprojects the whole frame into a
  persistent texture and copies/blits it (pixel-identical, tested). It is **off
  by default**: a clean back-to-back A/B measured it **31.8 % worse on scrub**
  and **66.8 % worse on hover** (heavy_ui +2.4 %, noise), so a full repaint +
  full copy is not a win; a true scissored partial update is the change that
  would make it one.
- **Wasm**: `getrandom`/`rand` removed from `ferrous_renderer` (deterministic
  SSAO PRNG). `cargo check -p ferrous_engine --target wasm32-unknown-unknown`
  and `-p ferrous_web` are green; the check joins the engine gate.
- **Guards/docs**: `scripts/perf_budgets.json` (P7-after-clean medians + 15 %,
  `--check` green) and `scripts/bench_baselines.json`; D-0063 "Performance
  policy" (retained ranges chosen over `UiSystem`, with the numbers and what
  would change it); T-PERF-0001 closed as superseded; engine `performance.md`
  and `present.md` in the mkdocs nav; `docs/performance.md`; this section.
- **Measured** (`p7-after-clean.json`, release 1600x900, median of 3): hover
  5.776 % / 3.3 % damage / 719 frames; scrub 6.955 % / 1.735 ms / 10.1 %;
  playback 1080p 18.234 % / 54.4 fps / 0 dropped; 4K30 11.963 % / 27.8 fps / 0
  dropped; heavy_ui ~54.9 % / 4.240 ms / 23 draws; heavy_ui_list 3,000 9.236 % /
  0.154 ms / 333 MB; heavy_composition 9.955 % / 0.103 ms / 50.8 MB; export
  17.02 fps.
- **Gate**: Motioner fmt, clippy `-D warnings`, tests (incl. the new eval-cache
  and scaling tests), doc `-D warnings`, boundaries, encoding, machete, validate,
  refs; engine workspace check, touched-crate tests, wasm, clippy no new
  warnings (pre-existing 27/13 debt), touched-file rustfmt, docs. Goldens
  byte-identical.
- **Disk**: 47.9 GB at the P7 baseline; **lowest free space seen 37.2 GB**
  (engine clone target + Motioner debug/release/doc simultaneously); `cargo
  clean` at close removed 5.0 + 4.2 GiB and left **46.65 GB free**. The harness
  does not sample commit: the largest private value is 333.0 MB (heavy_ui_list
  3,000; P6 was 798.5) and WS 610.1 MB, so there is no reliable commit peak.

P6 (2026-11-20) — Motioner hot paths (six parallel lanes + integration):

- **heavy_ui_list (3,000 layers, scrolled)**: collection **0.232 ms** (<=2 ms target)
  and flat vs 1,000 (0.155 ms, delta 0.077); the pre-P6 binary never reached a steady
  frame (~57 s to the first: 27.5 s MDF compile + 29 s asset usage compile, the latter
  memoised and moved off the first frame now).
- **Retained panels** (`paint.rs` `PanelCache`, `GuiBatch::extend_from`, per-row keys
  with the exact sub-widget hit and timeline duration/rate): hover **695 frames /
  6.896 % [4.943-7.005]** (P3 720 / 8.67, met), static hover 12 frames / 2.783 %.
  Scrub **8.759 % [8.242-9.269]**, collection **2.866 ms** (P3 7.23 %, 1.769 ms: missed;
  the timeline repaints every scrub frame and pays the batch copy).
- **Culling O(items)** via a per-item effect-layer flag in the plan (rotated rect, sharp
  miter, masked item and missing path/mesh tests); eval static-subtree cache keyed by the
  session generation (heavy_composition collection 0.164 -> **0.085 ms**, CPU 13.16 ->
  11.58 %).
- **Lazy 2D target** (engine): VRAM start 18.6 -> **7.6 MB**, editor 30.3 -> 19.3,
  hover 32.2 -> 21.2, scrub 35.3 -> 24.3, playback 37.5 -> 21.7, showcase 61.8 -> 50.8.
  Adaptive blur-first layer scale (K = 9 px keeps the 9 px golden byte-identical; showcase
  has no blur-first layer).
- **Playback** paced by the composition period on a wake chain: 60 fps during the 6 s demo
  (361 frames in 6.79 s wall incl. startup), 0 dropped, p99 2.37 ms. **Export 14.95 ->
  17.19 fps** with the readback ring in pipeline.
- **Damage** (chrome-only): hover 100 %, scrub 41.6 %, playback 39.8 %, heavy_ui 3.3 %.
  PersistentUiTarget fed but **not wired** (hover above the 71-74 % break-even; needs an
  offscreen whole-composition present, P7). Input-to-present: hover p50 0.98/p99 1.76 ms,
  scrub p50 3.86/p99 5.66 ms.
- **heavy_ui collection 4.688 vs 4.281 ms** (same throughput 17.20 vs 16.96 fps): the
  overdraw fixture repaints the composition every frame and the chrome cache is copied;
  segments 12 -> 38. P7 backlog with the UiSystem alternative.
- **Gate** in a clean worktree (`git worktree add ../Motioner-gate HEAD` + the P6 patch +
  submodule 7e623a9): fmt, clippy `-D warnings`, 88 test groups, doc `-D warnings`,
  boundaries, encoding, machete, validate, refs — all exit 0; goldens byte-identical.
  Engine: check of the touched crates, clippy no new warnings in touched files, tests
  (core 38, 2d 41, renderer 18+3+6, ui_render 40, engine 79), touched-file rustfmt, docs
  without new warnings; wasm still blocked (P7).
- **Disk**: lowest free space seen **5.60 GB** (release build + measurement with both
  targets alive), then cleaned; 53.82 GB free at close. Peak commit ~15.3 GB (8 GB RAM +
  pagefile peak 7.27 GB, best available; the harness does not sample commit).
- **Left for P7**: scrub collection, heavy_ui 4.7 -> 2 ms (single-batch retention/UiSystem),
  persistent target wiring, MDF compile of 3,000-node programs, real counting-allocator
  tests for the eval cache, engine lint/wasm debt.

P5 (2026-11-20) — GPU efficiency (six parallel lanes, engine + Motioner):

- **Instruments**: per-frame `gpu_frame` in the frame trace (draw calls, passes,
  pipeline/BG/buffer binds, scissors, GUI segments), parsed by the harness;
  `MAX_PASSES` 8 -> 24. The GPU is **not** the bottleneck in any Motioner
  scenario (2 passes, <2 ms); heavy UI is 4.1 ms of collection.
- **State binds**: the GUI pass hoists BG0/quad VB/index per pass and reuses the
  pipeline for icons; `DrawState2d` skips unchanged 2D binds. Hover 27 draws with
  **73 -> 25 buffer binds, 52 -> 26 BG binds** (pixels identical); heavy UI
  68 -> 22. Draw calls unchanged (22-30).
- **VRAM**: the 2D modes disable the inert GUI depth (-5.76 MB; pipelines are
  rebuilt with/without depth): start 24.1 -> **18.6 MB**, editor 35.8 -> 30.3,
  showcase 67.3 -> 61.8. Motioner runs Pure2D at `sample_count = 1`: there is no
  4x MSAA/HDR target in the live app. `RenderTarget` color+depth (11.52 MB) is
  still eager in 2D (`LazyRenderTarget` exists, unwired; P6).
- **Layers**: `Pure2D Layered Main` is timed (0.406 ms; GUI 0.276). Opt-in
  `layer_resolution_scale` / `FERROUS_LAYER_SCALE=0.5`: showcase VRAM 61.8 ->
  **37.1 MB**, private 403.7 -> 379.9, same fps (softer text; not default).
- **Partial redraw**: `PersistentUiTarget` + `partial_redraw` example: 1600x900
  full 0.455 vs 0.025 ms (18.45x), 4K 2.597 vs 0.025 (102x), break-even damage
  ~71-74% of the window. **Rejected wiring**: Motioner damage is the whole window
  until per-panel damage (P6).
- **Overdraw**: additive debug view (`FERROUS_OVERDRAW=1`) and `Layer2d::bounds`.
  Front-to-back early-z and covered-node skip are rejected (painter order; no
  per-quad bounds/identity in the direct-paint path).
- **Present**: `PresentMode` + `FERROUS_PRESENT_MODE` (Fifo/FifoRelaxed/Mailbox/
  AutoNoVsync). Playback: all 55-57 fps, 0 dropped; Mailbox p99 2.07 ms but CPU
  20.8% vs Fifo 17.4%. Keep **Fifo**.
- **P4 regressions**: not reproducible. Hover 720/720/728 frames and 6.67-6.93%
  (P4 reported 1124); scrub 8.41% median inside the 5.63-13.49% spread. Lane F
  still removes the 25 ms image-reload wake (video keys are never "missing",
  failures are sticky, the wake only re-arms while a decode is in flight) and
  culls items outside the canvas.
- **Heavy UI**: all 3,000 rows are inside 1600x900; only 120 distinct positions,
  painted 25x each. It is overdraw, not off-screen work; no cull can help.
- **Gate**: Motioner fmt, clippy `-D warnings`, full tests (**goldens
  byte-identical**), doc `-D warnings`, boundaries, encoding, machete, validate,
  refs. Engine: clippy on touched crates (no new warnings), tests for
  renderer/ui_render/core/2d/engine, touched-file rustfmt, docs without new
  warnings; wasm blocked by the pre-existing getrandom/feature issue.
- **Lanes**: six in parallel, 22 min wall (23:30-23:52); integration + measures
  + gate ~3 h 15. C: hit **0 bytes** during the engine test link (5.2 + 5.9 GB
  cleaned); **5.43 GB free** at close; peak: 31.3 GB RAM, 5.91 GB target.

P4 (2026-11-19) - CPU algorithms (five parallel lanes, engine + Motioner):

- **Lanes A/B (engine UI) do not run in Motioner**: the editor paints with
  `GuiBatch` directly, never `UiSystem`/`RenderCollector`, so heavy UI is
  Motioner's own painter plus `Renderer` text. Keep that in mind for P5-P7.
- **Text allocations**: `draw_text_2d*` cloned `TextMetrics` per call (one alloc
  per text per frame) and `count_missing` measured full text. New
  `FontAtlas::layout_block_glyphs` (draw-only, no metrics copy) and
  `count_missing` (char scan) plus `Renderer::missing_glyphs_2d` /
  `Canvas2d::missing_glyphs`: heavy UI **3,154 -> 154 allocs/frame**, 135 -> 39.5 KB.
- **Proxies**: `ProxyTarget::PerImage` sizes per axis (crop x node scale x zoom x
  DPI, capped), 1.25x/2x hysteresis, 4K@400 % test `per_image_proxy.rs`. Keys
  whose revision changes (video) become **dynamic** and upload at natural size
  in place; without that every video frame was resampled on the frame thread
  (scrub collection 1.79 -> 6.83 -> 1.78 ms). Re-decode worker
  `motioner_ui/src/image_reload.rs` + `ProjectHost::image_worker`.
- **Engine features**: `LayoutCache::get` O(1) (arena + intrusive LRU), ASCII
  fast path with per-atlas tables, retained node batch ranges in `GuiBatch`
  (hover rebuilds one node), layout early-out + rect diffing + clip-aware
  spatial index, path tessellation cache by geometry hash, `AtlasOccupancy`
  (best-fit packer measured and rejected for golden safety), `frame_ms` p99 in
  the frame trace, `FERROUS_MEMORY_HINTS` override.
- **Measured**: heavy UI collection 4.72/4.54 -> **4.06 ms**, 17.6 -> 18.3 fps
  (the <2 ms target is not met: 3,000 visible rows, ~150k glyph instances, no
  occlusion culling; P5/P6). Scrub collection back to 1.784. Playback 55.39 fps
  p99 2.44; heavy composition p99 1.19. MemoryHints A/B: `performance` 568.9 /
  532.1 MB vs `memory_usage` 434.0 / 398.7 MB private, p99 equal or better.
- **Gate**: Motioner fmt, clippy, full tests (goldens byte-identical), doc,
  boundaries, encoding, machete, validate, refs. Engine: workspace check, CI
  clippy (-D warnings) + no-default lib/tests, tests for all touched crates,
  wasm, docs without new warnings, touched-file rustfmt. Pre-existing debt
  confirmed: no-default `--all-targets` clippy fails on `benches/subsystems.rs`
  (imports optional `ferrous_ui_render`), `ferrous_ui_core` doctests with box
  drawing characters, unused-import warnings in ferrous_web/editor/ecs.
- **Lanes**: 2 h 12 min wall for five lanes; session ~4 h 45. C: 5.63 GB free at
  close after cleaning both targets and deleting the phase binaries.

P3 (2026-11-19) ? bytes, layout and ownership (five parallel lanes):

- **`MemoryHints`**: `EngineContext::from_adapter_with(hints)`; `AppConfig`
  resolves `MemoryUsage` for Flat2D/Pure2D and `Performance` for Game3D. wgpu-hal 23
  reserved 128 MB free-list chunks per memory type; the Flat2D renderer build falls
  427.6 -> 265.3 MB private with identical pixels. Start idle **465.0 -> 331.0 MB
  private (-28.8 %), WS 324.1 -> 204.3**; editor 490.1 -> 354.8; playback 597.8 ->
  435.0; heavy UI 557.9 -> 477.0; showcase 594.7 -> 400.2. Real dedicated GPU memory
  (editor) **160.2 -> 144.0 MB (-10 %)**. Halving is not reachable: 126 MB is the
  Vulkan driver floor (instance 88 + adapter 24 + device 14) and ~124 MB are the
  4x MSAA HDR target + GUI depth at 1600x900 (P5 owns MSAA/resolution).
- **Inventory**: `Subsystem::RenderTarget`; render-target attachments, the 3-D group
  and the 2-D layer/effect pools are registered and released; flat-inventory test
  `ferrous_renderer/tests/memory_flat.rs` (256 B tolerance over 30 frames).
- **Fonts**: per-atlas layout LRU (16 MiB) + table budget + CPU inventory; heavy UI
  collection **19.47 -> 4.49 ms/frame**, 16.1 -> 18.2 fps.
- **Motioner**: decoded-image CPU LRU (256 MiB) with inventory and release, `ProxyTarget`
  (Fit in the canvas, Full headless), image textures freed on set replacement, face ids
  cleared on font-set change, P2 cache keys audited with tests (`cache_keys.rs`).
- **Idle**: 1 frame / 1 submit; residual 0.1-0.4 % of a core attributed by thread start
  address to NVIDIA driver threads (`nvoglv64.dll`/`nvwgf2umx.dll`), 1-4 quanta/15 s;
  the harness now excludes teardown, is DPI-aware and injects pointer moves.
- **Hover**: static 2.52 % total -> **0.52 % tail, 12 frames/30 s**; moving 6.54 -> 4.59 %
  tail. `CursorEntered`/`CursorLeft` follow the pointer opt-out (engine `a13c9d3`).
- **Lanes** (wall 46 min vs 2 h 29 min sequential, -69 %): L1 formats 18m52, L2 textures
  44m34, L3 ownership 28m39, L4 Motioner bytes 38m44, L5 idle/hover 18m.
- **Gate**: Motioner fmt, clippy, **87 test groups**, doc, boundaries, encoding, machete,
  validate, refs; engine check, clippy (touched crates + no-default engine), tests,
  `memory_flat`, wasm, doc, touched-file rustfmt. Goldens byte-identical. **C: 2.0 GB free**
  at the gate; cleaned both targets at close (9.48 GB free).

P2 (2026-11-18) — zero allocations per frame:

- **Heavy-UI memory jump closed** (`e9761db`): P1's harness enabled
  `FERROUS_TRACE_STARTUP=1` everywhere and `startup::span` recorded every
  per-frame span (1.8M records at teardown, 926.8 MB private). Spans now stop
  at the first-frame report (marks continue for close); the harness enables the
  startup trace only for idle. Heavy UI back to **557.3 MB private / 395.4 MB
  WS** (P0 556.2 / 403.0).
- **Per-thread alloc counters**: `thread_snapshot`/`ThreadScope`; `frame_trace`
  reports frame-thread allocations and `worker_*` separately. Heavy UI (120
  frames, alloc build): **3,154 frame-thread + 0 worker** allocs/frame (P0
  15,459 process-wide), 423 KB/frame.
- **Zero-allocation paths**: `RenderCollector::collect_with` (by reference, no
  child/command clones), `UiSystem::render_into` into the recycled batch,
  `draw_text_baseline` direct push, SVG scratch buffers, FxHasher
  (`rustc-hash`) in font/renderer/UI maps; Motioner mesh vertex/index scratch +
  per-topology indices, `Mesh::sample_into`/`write_indices`; LivePainter face
  cache; Editor `orphan_suggestions`/`code_panel` and PreviewApp `frame_facts`
  caches by generation/geometry.
- **Tests**: engine UI + Pure2D, Motioner editor + playback = **0 allocs/frame**
  after warm-up (`alloc-stats`, ThreadScope). Heavy UI still 3,154/frame
  (text/collection; P4).
- **Hover**: `redraw_on_pointer_move(false)` + `on_window_event`; across panels
  1144 -> **698 frames/30 s**, static area **5 frames/30 s total, 0 after
  startup**.
- **Watcher**: `notify` OS events replace the 80 ms sleep + 250 ms stat; 0
  periodic wakeups. **Harness**: unique output names + `--force`, startup trace
  only for idle, `--allow-pointer` restores the cursor.
- **Measured**: idle start/editor 1 frame / 1 submit, 0.42 / 0.16 % after the
  first frame; playback 55.3 fps / 20.28 % (P1 18.86; A/B P1 binary today
  23.11 % / 68.4 fps); heavy UI collection 20.14 -> **15.45 ms**, 14.1 fps
  (machine state; export 13.4 fps for both P1/P2 binaries today).
- **Gate**: Motioner fmt, clippy, **86 test groups**, doc, boundaries, encoding,
  machete, validate, refs, plus the zero-alloc tests; engine **29 groups**,
  wasm, clippy default/no-default (no new warnings in touched files), docs (no
  new warnings in touched files), touched-file rustfmt. **C: 13.50 GB free**.

P1 (2026-11-18) — idle = zero:

- **Engine** (`7022976`, `ee2b772`, `9d6218c`, `11a5b3c`, `74e85a2`,
  `9c3d6e3`): `redraw.rs` with `RedrawPolicy::{Continuous, OnDemand}` and a
  pure `Scheduler`; `AppContext::request_redraw`/`redraw_at(Instant)`/`waker`;
  worker `Waker` over `EventLoopProxy`; runner creates the loop first,
  `user_event` wakes it, `about_to_wait` draws or sleeps; `AppConfig::visible`
  (never-shown window renders from `about_to_wait` because it gets no
  `RedrawRequested`); `frame_trace` gains `presented`/`gpu_submits` and stops
  counting `RedrawRequested` as event handling. Docs `redraw.md` + nav.
- **Motioner**: `OnDemand` (with `--continuous` opt-out), no
  `request_redraw` left: playback 16 ms, audio open 50 ms, pulses 16 ms,
  tooltip due, overrides `write_at`, heartbeat 30 s/snapshot 2 min, proxy
  250 ms (one 3.2 s wake to clear 100 %), video 16 ms, frame limit.
  `motioner-watch` worker + `Waker`; control/assets workers wake the loop;
  heartbeat uses the cached `Project` (30 s spike 3.3 s -> ~0).
  `--exit-after-seconds`, `--hidden`; `--start --hidden` fixed. App-side
  `FileWatch`/`program_writes` removed.
- **Measured** (release 1600x900): start idle **1 frame total, 0 after**,
  0.031 % over a 45 s tail, 1 submit; editor idle **1 frame, 0 after**,
  0.062 %, 1 submit (hidden window; 60 s probe). 30 s harness + warm-up:
  start 0.37 % idle / 0.521 % tail, editor 0.264 % / 0.313 %. Playback
  55.4 fps / 18.86 % (P0 54.2 / 18.33); hover 1144/30 s / 9.79 % (P0
  601/11 s / 16.96); scrub 6.82 % (P0 20.32); heavy composition 11.49 % /
  55.3 fps (P0 13.15); export 28.68 % / 15.9 fps (P0 31.97). Heavy UI still
  99.4 % / 14.9 fps, collection 20.14 ms (event handling was a trace bug:
  22.26 -> 0.000); private 927 MB vs 556 (P3/P4).
- **Threads**: 45 start, 45 editor hidden, 52 editor visible (watcher +1;
  workers block on channels); 0 idle frame wakeups, autosave 1/30 s.
- **Gate**: Motioner fmt, clippy, 85 test groups, doc, boundaries, encoding,
  machete, validate, refs; engine 28 test groups, wasm, clippy default +
  no-default (no new warnings in touched files), docs (no new warnings in
  touched files), touched-file rustfmt. **C: 13.6 GB free** after cleaning.

P0 (2026-11-17) — instruments and baseline:

- **Instruments**: `ferrous_core::alloc_stats` (feature `alloc-stats`, counting
  global allocator + `snapshot`/`FrameScope`), `frame_trace` (`FERROUS_TRACE_FRAMES`,
  nine stages, median/p90/p99 + allocations, JSON on exit), per-pass GPU
  timestamps (`TIMESTAMP_QUERY`, Pure2D + GUI passes, async readback with lag),
  `ferrous_core::memory` + `Renderer::memory_report` (GPU/CPU per subsystem and
  largest items; 3D passes/offscreen targets and Motioner's decoded-image CPU
  copies/glyph tables are declared P0 gaps for P3), divan
  `benches/subsystems.rs`, and `scripts/perf_bench.py` (8 scenarios, psutil,
  `--check` scaffold). `motioner-bench` binary (feature `alloc-stats`) installs
  the counting allocator; `alloc_stats` is re-exported through `motioner_ui` to
  keep the dependency boundaries.
- **Bug found/fixed**: GPU timestamps were created from the env var alone and
  the headless `still`/`render` path hung waiting. `Renderer::enable_gpu_timings`
  is now called only by the runner; the headless harness never creates a query
  set. Headless readback tests pass again.
- **Baseline** (release, 1600x900, 1 measured + 1 warm-up, alloc build for
  allocations): start idle **11.85 %** CPU / 10.81 s / 601 frames / 334 MB WS /
  465 MB private / 45 threads / 18.6 MB VRAM / **114 allocs/frame**; editor idle
  17.20 % / 408 MB / 565 MB / 52 threads / 295 allocs; hover 16.96 %; scrub
  20.32 %; playback 1080p 18.33 % / 594 MB private / **2.83 MB per frame**;
  heavy UI (3000 texts) **99.86 %** / 36.91 s / 16.3 fps / **15 459 allocs/frame**
  (event 22.26 ms + command collection 18.78 ms per frame); showcase_pro 13.15 %
  / **748.7 MB private**; export 120 frames 31.97 % / 7.28 s / 16.5 fps.
  GPU passes are small (Pure2D 0.005-1.759 ms, GUI 0.152-0.306 ms).
- **Profiling**: WPR/ETW captured 56 201 samples / 641 282 stack events for
  heavy_ui and 18 338 / 221 641 for playback. `samply` cannot record on Windows
  (Linux/macOS only); no WPA/xperf/PDB, so stacks were not symbolized. The top-20
  ranking in the report is by measured stage/GPU/allocation/memory share, not
  function names (declared limitation).
- **Benchmarks**: ui_layout 8192 5.451 ms (14.18 MB one-shot); gui_batch
  16k quads/glyphs ~165 µs, 0 allocs; text layout 4.599 µs long / 0 allocs;
  MSDF ASCII 3.295 ms / 213 allocs / 1.80 MB; tessellation 0 allocs; pure2d
  heavy 2048 328 µs / 87 allocs; SVG upload 2.023 µs / 6 allocs.
- **Gate**: Motioner fmt, clippy, 85 test groups, doc, boundaries, encoding,
  machete, validate and refs green; engine tests 28 groups green, wasm green,
  touched-file rustfmt clean. P0 files clippy-clean; `ferrous_engine`/
  `ferrous_ui_render` keep the preexisting pedantic debt (27/13) documented in
  S5. **C: ≈2.85 GB free**; `cargo clean --release` twice. Next: P1 (idle = 0
  frames, ≤0.5 % of a core, thread audit).

## Prompt 17 status (startup performance)

Session `S-2026-11-15-001` (open). **S0-S6 done; prompt 17 complete.** No
commit in Motioner. S0 is committed and pinned, S1 as `5873863..a078f80`
(pinned), S2 as `a078f80..4582c42` (pinned), S3 as `4582c42..5687d18` (pinned),
S3b as `5687d18..a3ee934` (the rejected snapshot, still in history) and its
replacement as `a3ee934..2c23263`, S4 as `2c23263..8338d1c` (pushed; Motioner
pin `3b02208`), S5 as `8338d1c..493619f` (exit hook, **not pushed**, the
submodule is checked out there locally), and S6 as **uncommitted working-tree
changes in the dev clone on top of `493619f`** (show.rs docs + pure timeout
helper, first-frame mark before ShowWindow + `report_first_frame`,
`startup.md`). Reports: `PHASE-17-S0-REPORT.md` through
`PHASE-17-S6-REPORT.md` plus `PHASE-17-REPORT.md` (Spanish).

S6 (2026-11-17) — budgets, docs, reviewer findings:

- **Guard**: `scripts/startup_bench.py --check` (5 runs) fails on (i) the
  normal-path `engine: window visible` before `engine: first frame presented`
  (the timeout path has its own mark and is accepted) and (ii) any metric in
  `scripts/startup_budgets.json` over budget or missing. Budgets are 2x the S6
  median/p90 (5 ms floor) and 50 ms close-to-hidden; the critical path is
  `first frame - surface configured`, so the driver floor is excluded. The
  engine now marks the first frame right after `present()` and emits the report
  after the show (`report_first_frame`), so the order is provable. Pure
  `timeout_from_override` replaces the env-mutating test. Documented in
  `docs/startup.md` and D-0062.
- **Measured** (release, 1600x900, warm 20): start first frame **471.3 ms**,
  window visible **490.8** (p90 495.8), wall 666.7, path 33.8, renderer worker
  27.8, close hidden 1.8; preview first frame **624.4**, visible **650.4**
  (p90 678.7), wall 851.6, path 139.8 (document load 27.7, canvas 103.3, paint
76.7), close hidden 2.9. Cold (fonts+pipelines cleared, 5): start 489.8/
509.0, preview 664.2/687.6. AMD 760M (`WGPU_POWER_PREF=low`, 5): start
447.5/465.0, preview 604.3/633.5. Driver floor: surface configured 437.8
  (start) / 483.5 (preview); instance 207/245, device 44/48, swapchain 186/191.
- **Real screen** (`screen_check.py`, now refuses captures after the process
  exited and detects a covering window): start normal 3/3 (490-507 ms, first
  composed matches), start timeout 50 ms 3/3 (460-472, background then drawn),
  preview normal 3/3 (638-658 ms with `--allow-late-content`; a first batch had
  a 816/859 noisy outlier), preview timeout 3/3 (506-526, black then
  composition).
- **Console**: `motioner_app` split into `src/lib.rs` (`run_cli`) + GUI
  `src/main.rs` (subsystem 2) + `src/bin/motioner-cli.rs` (subsystem 3). Both
  are built by `cargo build --release -p motioner_app --bins`; verified
  `motioner-cli info` with `>` and exit codes in PowerShell and cmd, and the
  `motioner.com` copy alias. REF-0002 §11/README/docs/startup.md updated.
- **Findings**: (a) one GDI brush per colour was already fixed in S3b
  (`window_style.rs`); (b) keep `latin_western` (2.5 ms warm / 22 ms cold with
  the cache, not worth a per-app charset); (d) `show.rs` docs fixed and the
  timeout limitation documented; (e)/(f) screen_check + pure timeout test;
  (g) console companion; (h) `Untitled.motioner/` deleted and `/*.motioner/`
  ignored. Gate green (see the report); **C: 5.5 GB free** after the gate
  (purge `target/debug` before the next heavy build).

S5 (2026-11-16) — engine `8338d1c..493619f` (open/close paths):

- Assets: `Host::assets` now returns only the cheap facts (registry, size,
  usage, cached peaks) and starts a `motioner-assets` worker; ffprobe
  durations and the 6 video thumbnails arrive through the new
  `ProjectHost::poll_assets` and `PreviewApp::poll_assets` (installed into the
  same list, no layout shift). `motioner: refresh assets` 379 -> **1.23 ms**;
  `asset summaries` 0.22. Test `video_thumbnails_arrive_after_the_first_assets_call`;
  capture `out/reports/phase17/s5-assets.png`.
- Audio: `AudioOutput` is a handle to a `motioner-audio` worker that owns the
  non-`Send` `cpal::Stream`; mix (decode 1 ms + mix 4 ms) stays on the UI. New
  `audio_prewarm` (called from `run_preview` and `open_composition`) opens the
  device while the GPU starts. The transport starts playback after the first
  frame when the device is ready (or after 1.2 s), and `audio_align` starts the
  buffer at the current frame if the open was late. Device median 587 ms,
  19/20 before the first frame; outlier 1.23 s. Test
  `the_shipped_demo_mix_is_not_silent`.
- Close: new engine `FerrousApp::on_exit` (493619f) runs after the window is
  hidden and before the event loop exits; `PreviewApp::on_exit` forces
  `flush_overrides`, `end_session` (lock) and `save_state` (2.44 ms); the
  pipeline cache is saved after the loop and before the GPU drop.
  `close_project` also forces the override flush.
- **Measured** (release, 1600x900, 20 warm): `preview` first frame
  **1434.1 -> 693.5 ms (-51.6 %)**, wall 1648.8 -> 905.3; `preview` is now
  start + **155 ms** (`document load` 29.8, `canvas` 105.1); `update` ~819 ->
  2.92; `start` 538.2 first frame / 722.2 wall (S4 A/B 542.6/737.2); pre-window
  host 0.27 + recents 0.52 + examples 0.11 + app new 0.04 = **0.94 ms**. Close
  hidden start 1.66 / preview 2.11 ms; hidden->teardown 74.3 / 96.1; the
  window disappears well under the 50 ms target.
- Real screen (`screen_check.py`): start normal 3/3 (505-526), start timeout
  50 ms 3/3 (473-497), preview normal 3/3 with `--allow-late-content`
  (677-699; first frame is the editor chrome + video placeholder, reference
  has the video + thumbnails; never white/black), preview timeout 50 ms 3/3
  (567-589, black editor background then composition). The script gained
  `--allow-late-content` for asynchronous media.
- Gate green: Motioner (fmt, clippy, tests incl. the 34-motioner bin tests and
  goldens, rustdoc, boundaries, encoding, machete, validate, refs); engine
  `check --workspace --all-targets`, `test -p ferrous_engine` (56+1 ok),
  wasm check, fmt. C: 9.4 GB free after purging `target/debug`.
- Delivered by S6: budgets/regression guard, D-0062, engine `startup.md`,
  final report, reviewer findings, the pre-existing `Untitled.motioner/`
  directory, and the console/GUI docs (the exit hook is in 493619f).

S4 (2026-11-16) — engine `2c23263..8338d1c` (lazy 3D + step 0):

- Step 0(a): `motioner_app` is built with the Windows GUI subsystem
  (`#![cfg_attr(windows, windows_subsystem = "windows")]`), so double-click and
  `--start` open no console. `main` calls `console::attach_parent_console`
  first: raw `AttachConsole(ATTACH_PARENT_PROCESS)`, and `CONOUT$`/`CONIN$` are
  opened only when `GetStdHandle` is null/invalid; inherited console/pipe/file
  handles are kept. Verified with subprocess (help/info/unknown command: 0/0/1
  exit codes, pipes), PowerShell `Start-Process -Wait -RedirectStandardOutput`
  (153 lines) and `startup_bench.py`. `cmd` does not hand its `>` handles to a
  GUI-subsystem child and does not wait: documented in REF-0002 §11 with the
  supported alternatives.
- Step 0(b): removed the per-frame GuiRenderer prints + `static mut` counter and
  the RenderCollector print/atomic; adapter/pipeline/asset/font messages go
  through `log`; removed the debug-only per-image average colour and UV scans;
  Motioner `desktop: enter` gone. `log` added to `ferrous_assets` and
  `ferrous_ui_render`.
- S4: `Renderer::new_with_mode(Flat2D|Pure2D)` builds only 2D. `ensure_three_d`
  creates the world pass (instancing + shadow map + HDR + IBL + sky),
  material registry, SSAO/prepass, skinning, gizmos, post-process/AA, instance
  buffers and the 1M particles on the first 3D use or `set_mode(Full3D)`.
  Clear colour/sky/style/light/SSAO params are cached until then. The runner
  passes `AppMode` through `GpuInit`/`DeviceInit`; `Pure2dHarness` constructs
  in `Pure2D`. The four unused PBR pipelines are no longer created. Tests:
  `tests/lazy_3d.rs` (3). Example: `examples/renderer_startup_cost.rs`.
- **Measured** (release, 1600x900): engine example Vulkan `Renderer` Flat2D
  **51.1 ms** median vs Full3D **64.2** (first 3D use 16.4); DX12 7297/7273
  (2D pipelines dominate) and the group costs 572 ms at first use. DX12
  allocator: 31.26 MB (Flat2D) -> 134.74 MB (3D), **+103.48 MB (98.7 MiB)**.
  Motioner same-session A/B (20 warm, S3b pin vs S4): `--start` renderer
  worker **52.81 -> 30.43 ms (-42 %)**, first frame **591.2 -> 542.6 (-48.6,
  -8.2 %)**, wall 794.5 -> 737.2; final `8338d1c` 5 runs: 29.2, 509.4, 687.1.
  `preview`: renderer 51.44 -> 31.10, first frame 1466.9 -> 1434.1, wall
  1681.8 -> 1648.8; final 1362.2/1556.4. Driver phases also moved (instance
  233->222, surface 507->483), so the attributable S4 delta is the span.
- Real screen (`scripts/screen_check.py`, final binary): normal 3/3 (visible
  541-570, first composed matches reference, diff 0.99-1.04); forced timeout
  50 ms 3/3 (visible 519-530, first composed is `#0A0B0D`, reference drawn).
  The harness now raises the window topmost before capturing (another app in
  front produced false fails and a false pass), accepts a first frame that
  already beat the timeout, and no longer rejects the uniform background.
- Gate green: Motioner (fmt, clippy, tests incl. goldens, rustdoc, boundaries,
  encoding, machete, validate, refs); engine CI scope (fmt on touched files,
  clippy default + no-default, tests default + no-default + release
  pure2d_headless, workspace check, wasm check, docs without -D). Fixed the
  wasm cfg break and the pure2d_images clippy lint from Rust 1.96. C: 8.6 GB
  free after purging debug targets.
- Left for S5/S6: preview assets/audio (S5), budgets/decision/docs (S6),
  `screen_check.py` cover-window handling (started here), `cmd` redirection
  limitation. Flat2D still builds the internal `RenderTarget` and both
  `Renderer2d` (2D, not in the S4 list); a Flat2D->Full3D switch keeps the 3D
  group resident.

S3b (2026-11-16) — engine `a3ee934..2c23263` ("show the window when it is
ready"; the launch snapshot is gone):

- The snapshot (`a3ee934`) was **rejected on the real screen**: its child window
  never painted (the boxed state was passed as `CreateWindowExW` lpParam but
  never stored in `GWLP_USERDATA`, so `WM_PAINT` drew nothing and
  `WM_NCDESTROY` leaked it) and the client area stayed white until the handoff.
  The "0 differing pixels" check was invalid: it ran `capture_window.ps1` in a
  child `powershell.exe` whose own start-up delay put every capture after the
  first frame, and the DPI-unaware process captured a virtualised rect, so it
  compared the post-handoff screen with the file. The reviewer's
  `ImageGrab` capture from the same process at visibility exposed it.
- Removed: `snapshot.rs`, the snapshot API on `AppConfig`/`AppContext`, the
  child window, the file format, docs and tests; Motioner's start key, the
  capture request and the cache dir (the old `%LOCALAPPDATA%\Motioner\cache\
  launch` is deleted on run).
- New: window created hidden; `GraphicsState::init_device` (instance, adapter,
  device, swapchain) stays on the event-loop thread (`Surface`/DXGI swapchains
  are not usable from another thread: a worker-created surface failed
  `is_surface_supported` and the re-probe hung), `Renderer::new` runs on a
  `ferrous-renderer` worker, the runner renders the first frame from
  `about_to_wait` (a hidden window gets no `RedrawRequested` from winit),
  presents it hidden and only then `ShowWindow`; `DWMWA_TRANSITIONS_FORCEDISABLED`
  kills the Windows 11 open fade. `AppConfig::show_window_when_ready` (default
  on), `AppConfig::window_show_timeout` (800 ms) +
  `FERROUS_SHOW_TIMEOUT_MS`; the timeout shows the window with
  `background_color` filled into the client area *before* `ShowWindow`
  (`FillRect` + class brush) and the class brush is kept per colour (no leak).
  Input before the first frame is queued and replayed; close works.
  Motioner passes the start screen's pasteboard as the background.
- **Measured** (20 warm): window visible = first frame **514.0 / 526.6** (S3
  497.4 with the window shown early), wall 713.4, `Renderer::new` 46.8 on the
  worker, close 4.5. Forced timeout 50 ms: window at 449-455 with the
  background. DX12: window at 836 (timeout 800) with the background, responsive
  (`SendMessageTimeout`), first frame 7845; timeout 50: window at 448.
- **Real screen checks** (`scripts/screen_check.py`, ImageGrab from the same
  process): normal 3/3, forced timeout 2/2, DX12 50 ms 1/1, DX12 800 ms 1/1.
  The capture taken the instant `IsWindowVisible` turns true still shows the
  previous screen: DWM has not composed the window yet (one vsync); the first
  composed capture (+16 ms) already shows the drawn start screen and matches a
  capture 1 s later.
- Tests: gate states, timeout reported once, env override, queue order and
  overflow. Gate green, goldens byte-identical. C: 3.3 GB free.

S3 (2026-11-15) — engine `cf7ddb9` + `5687d18`:

- `AppConfig::backends` is an ordered list (Vulkan, DX12, GL on Windows; Metal
  on macOS; Vulkan, GL elsewhere), one instance per entry; `WGPU_BACKEND`
  replaces it. An adapter that lacks `TEXTURE_BINDING_ARRAY |
  SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING` (GUI images)
  is skipped with a warning: GL no longer panics in wgpu validation and, if
  requested alone, fails with a clear message (wgpu 23 GL has no texture
  binding arrays).
- `AppConfig::power_preference` (default `HighPerformance`) + `WGPU_POWER_PREF`.
  `EngineContext::request_adapter_with` and `from_adapter_with_cache` (seeds
  the wgpu pipeline cache); WebGL2 limits now only on wasm.
- `App::run` starts a GPU probe (instance + surface-less adapter, re-checked
  with `is_surface_supported`) and the UI font bake on workers while winit
  creates the window; both joined at upload.
- `AppConfig::pipeline_cache` / `App::with_pipeline_cache`:
  `%LOCALAPPDATA%\<app>\cache\pipelines\<key>.bin`, key = backend/vendor/device/
  driver/driver_info + format version, magic+version+key header, atomic write,
  saved after the first frame and at exit; loaded at device creation.
  Vulkan-only in wgpu 23 (DX12/Metal ignore it).
- Window created hidden; on Windows the class background brush is set to
  `background_color` and `WM_ERASEBKGND` forced (winit's class has no brush -
  the white flash); elsewhere shown after a clear-only frame presented as soon
  as the swapchain is configured.
- **Measured** (release, 20 warm / 5 cold): `--start` first present
  **497.4 / 508.0** warm (S2 854.8), 503.6 cold fonts, 526.2 cold pipelines;
  **window visible with the correct colour at 50.7 ms** (p90 54.1; before, the
  first painted frame was ~855). Driver floor 432 ms (instance 186.6 + device
  41.9 + swapchain 189.7); our critical path ~64 ms (renderer 46.9 with cache,
  67.6 without; fonts 5.2; setup+frame 11.9). `preview` first present 1553.6
  (S2 1874.2). iGPU (Radeon 760M): 485.3 ms, swapchain delta 143.8, renderer
  delta 71.2. DX12: `Renderer::new` 7284.5 ms, no cache -> fallback only.
- Tests: 7 new engine tests (backend order, cache key, cache file roundtrip and
  corruption). Gate green, goldens byte-identical. C: 7.3 GB free.

S2 (2026-11-15):

- Motioner fonts are lazy: `LivePainter::lazy` registers only the fallback
  (Roboto, shared with the engine UI atlas); `paint`/`paint_visible` call
  `ensure_faces`, which registers every face the plan's text resolves to
  *before* drawing (unknown family keeps the documented Roboto fallback). Both
  return `Result<PaintStats, RenderError>`.
- `font_jobs.rs`: a worker bakes the document's faces through the S1 cache
  (`document_faces` + `BakedAtlas::build_cached`) while the engine initialises
  the GPU; the frame thread only uploads (`Renderer::register_baked_font_2d`,
  new engine API `4582c42`). Started in `PreviewApp::new` (before `App::run`),
  restarted on reload/open; the start screen registers nothing (no bundled set,
  no measurer) and opening a project builds `host.fonts` + `CpuTextMetrics`.
- `FontSet::bundled()` borrows the embedded bytes (`Cow`): `project fonts`
  0.62 -> 0.03 ms; `bundled fonts` span gone.
- **Measured** (release, 20 warm / 5 cold with cache cleared): `--start` first
  present **854.8 / 863.8** warm (S1 867.6), **885.3 / 887.7** cold (S1 989.7).
  Font work on the start screen 11.0 ms warm = 0.2 Motioner + **10.2 engine UI
  atlas cache hit** (5.4 MB read+decode+upload; 37.1 cold). `setup` 0.83 (S1
  10.9). `preview` first present **1874.2 / 1898.2** warm (S1 1887.5), 1917.5
  cold; worker bakes only Inter 700/400 (2.97/2.67 warm, 34.6/34.3 cold,
  overlapping GPU init) and the first frame pays 0.87 ms of uploads.
- The ~1 s first frame of `preview examples/demo.motioner` is **not fonts**:
  `refresh assets` 323 ms (ffprobe + 6 thumbnails per video in `host.assets`),
  `audio start` 571 ms (565–1248; `cpal` device open 565, decode 0.94, mix 4.03),
  `canvas` 93 ms (video texture 26, first image upload 66; text is opacity 0 at
  frame 0). Diagnosed for S5 in the report with four concrete fixes.
- Tests: `crates/motioner_ui/tests/live_fonts.rs` (3 GPU tests: face registered
  before drawing, the face changes the pixels, unknown family falls back) and 3
  `font_jobs` unit tests. Gate green, goldens byte-identical. C: 5.4 GB free.

S1 (2026-11-15):

- `ferrous_font`: `generate_msdf` bins every edge into 8x8 pixel tiles and only
  tests the edges whose grown bounding box reaches the pixel's tile; winding
  only looks at edges whose y range straddles the row. Output is byte-identical
  to the un-culled reference (tests on squares and real glyphs at 16/64 px).
  `latin_western` 305 glyphs at 64 px: **582.0 -> 35.7 ms single-threaded
  (16.3x)**; `examples/msdf_bench.rs` reproduces it.
- `BakedAtlas` (pure data: pixels, metrics, indices, kern+GPOS, vertical metrics)
  with `upload`; `FontAtlas::new` = build + upload and keeps its behaviour.
  `encode`/`decode` is a versioned, deterministic, bounds-checked binary form; a
  corrupt/truncated/stale file is a typed error, never a panic.
- `AtlasCache` (`<FontKey>.atlas`, FNV-1a 128 of bytes + chars in atlas order +
  cell size + px range + format/generator versions; atomic write, validated
  read, LRU eviction, 64 MiB default). `FontCacheConfig` resolves
  `%LOCALAPPDATA%\<app>\cache\fonts`; `AppConfig::font_cache` /
  `App::with_font_cache`; `Font::try_load_bytes_cached`. GPOS gained its codec
  next to the private types; `FontParser::data()` feeds the key. Docs:
  `crates/ferrous_font/docs/atlas-cache.md` + nav.
- `Renderer::register_font_2d` dedupes by `FontKey` (same bytes+charset returns
  the same `FontId2d`, no bake/upload); `Renderer::set_font_cache` and
  `font_2d`. The engine registers its UI font through that path, so an app
  drawing Roboto reuses the UI atlas: **Roboto 108-178 ms -> 0.18 ms**.
- **Measured** (release, 20 warm runs; 5 cold runs with the cache cleared):
  `--start` first present **867.6 / 875.9 ms** warm (S0 1616.3 / 1651.7) and
  **989.7 / 1022.0** cold; wall 1072.2 warm. Font spans warm 3.3/3.2/3.2/0.18
  (S0 178/174/166/108), engine UI atlas 10.1 (S0 134.7), setup 10.9 (S0 642.6);
  cold font spans ~30 ms per face. `preview examples/demo.motioner` first
  present **1887.5 / 1902.7** warm (S0 2633.4); its first composition frame is
  still ~1018 ms (S2/S5). Cache: 4 files, 21.5 MB total.
- Motioner opts in with `FontCacheConfig::for_app("Motioner")` in `run_inner`;
  `scripts/startup_bench.py --clear-cache DIR` measures cold runs.
- Gate green against the clone with a temporary path override (removed); goldens
  byte-identical. C: 7.1 GB free.

S0 (2026-11-15) — committed as Motioner `d0ea87e`, engine `5873863`:

- Engine: `ferrous_core::startup` (marks, spans, `begin`/`begin_from_env`,
  `span`/`span_with`, `trace`, `first_frame_presented`, `teardown`; enabled
  with `FERROUS_TRACE_STARTUP=1` or `AppConfig::with_startup_trace`;
  `FERROUS_TRACE_STARTUP_JSON=1` adds one JSON line per report; 8 tests;
  disabled cost is one relaxed atomic load and computed labels are not
  formatted). Runner marks: entry, event loop, window created, instance,
  adapter, device, surface configured, renderer built, fonts ready, app setup
  done, first frame presented, close requested, window hidden, event loop
  exited, teardown done. `EngineContext` exposes `request_adapter` +
  `from_adapter` (`new_with_instance` composes them). Closing now hides the
  window before exiting the loop (the S5 mechanism, needed for the mark).
  Docs: `crates/ferrous_engine/docs/startup.md` + mkdocs nav.
- Motioner: `motioner_ui::startup` re-export; `motioner_render` depends on
  `ferrous_core` for the per-face marks. Spans: `motioner: host`,
  `bundled fonts`, `recents`, `examples`, `resolve input`, `project fonts`,
  `document load`, one per font face in `LivePainter::with_fonts`; mark
  `motioner: main`.
- `scripts/startup_bench.py` (20 warm runs, median/p90 per mark/span, close
  rows, wall time, `--cold`, `--scenario`, `--json-out`, `--markdown`);
  optional tool, not in the gate; README documents it.
- **Baseline** (release, 20 warm runs, RTX 4050, 1600x900):
  `--start --exit-after-frames 1` first present **1616.3 / 1651.7 ms**
  median/p90, wall 1818.0. Phase deltas: window 37.8, instance 224.5,
  adapter 261.2, device 30.4, surface 185.8, renderer 68.2, engine font
  134.7, setup 642.6, first frame 31.2. Fonts Inter 400/600/700
  178.4/174.1/166.4 ms, Roboto 400 108.0 (Roboto is baked twice). Pre-window
  2.0 ms. `preview examples/demo.motioner` first present
  **2633.4 / 2650.1**, document load 27.1, first composition frame +1034,
  pre-window 30.9. Close: requested->hidden 5.2, hidden->teardown 82.3;
  ~98 ms of untraced process exit.
- The prompt's reference table is reproduced phase by phase (<=15 % spread);
  the floors are the GPU driver plus `Renderer::new` up to `renderer built`
  (~810 ms from process start) and our font bake (~620 ms).
- Gate green against the dev clone with a temporary `../FerrousEngine` path
  override, removed afterwards; **the tree will not build until the reviewer
  pushes the engine commits and bumps the pin**. C: 9.3 GB free after
  purging `target/debug` and `target/release`.

## Prompt 16b status (finish prompt 16)

Session `S-2026-11-11-002` (open). **E0–E4 and E5a/E5b are done; E6–E7
pending. No commit.** Reports: `out/reports/PHASE-16B-E0-REPORT.md` through
`PHASE-16B-E4-REPORT.md`, plus `PHASE-16B-E5A-REPORT.md`,
`PHASE-16B-E5B-MODEL-REPORT.md` and `PHASE-16B-E5B-ENGINE-REPORT.md`.

E5b engine/render/goldens (2026-11-15):

- FerrousEngine dev clone `cab7035..6d1bc7a`: `draw_textured_mesh_2d` on
  `Renderer`, `RenderContext` and `Pure2dHarness`; `MeshVertex2d`
  (position/uv/alpha, premultiplied flag), `MeshBatcher2d`, grow-only GPU
  vertex/index buffers, batching by texture+blend, the same texture registry
  (in-place updates/mips), painter depth, layers, masks, blend modes.
  Rustdoc, `crates/ferrous_renderer/docs/textured-meshes.md` + mkdocs nav,
  `examples/textured_mesh_2d`, 8 headless tests.
- Motioner: `motioner_render` draws `ItemShape::Image { mesh: Some(i) }`
  through `Canvas2d::draw_mesh`, remapping the [0,1] mesh UVs into the node
  crop; the 2x2 default stays on the sprite path. Existing goldens
  byte-identical.
- Goldens: `mesh_lattice_f030`, `mesh_video_skin_f030`, `mesh_arm_ik_f030`
  (inside a `screen` layer), `mesh_blend_f030`, plus a cropped-mesh vs sprite
  equivalence test.
- Release (1280x720, RTX 4050): mesh 8x8 x1 0.080 ms / x50 0.116 ms
  (0.0023 ms/item), 32x32 x1 0.102 ms / x50 0.470 ms (0.0094 ms/item),
  sprite x1 0.088 ms / x50 0.088 ms (0.0018 ms/item).
- Built with a temporary `../FerrousEngine` path override, restored to
  `engine/ferrous` before stopping; the tree will not build until the
  reviewer pushes 6d1bc7a and bumps the pin.
- Engine caveat: the clone baseline is not fmt/clippy/rustdoc-clean under
  Rust 1.96 (unrelated files); the touched crates and files are clean and
  `pure2d_meshes` passes.

E5b model (2026-11-13):

- `Mesh` channel on image/video (D-0061), default 2x2; `mesh=(N, M)`,
  `key("mesh", ...)` and `RenderPlan::meshes`; default quad stays on the
  sprite path.
- `m.part("name", node)` attaches image/video as a rig media part; shared
  `apply_points` deformers; bake writes normalized mesh keys.
- Pending: FerrousEngine textured triangle mesh and the three E5b goldens.

E3 (2026-11-12):

- `GeometryChannel` (D-0058) keys path points with a fixed point count;
  closure, cap and join are shared. `CoreError::GeometryTopologyMismatch`
  refuses a resize, and `sample` interpolates positions/handles with the
  arriving key's ease. `ContentField::Geometry` is a normal content channel;
  `key("geometry", t, points)` and `m.geometry(part, t, points, ease=...)`
  key it.
- `motioner_rig::geometry` is the driver IR: `path_builder` with
  `move_to`/`line_to`/`cubic_to`/`arc_to`/`close`, plus `ellipse_arc`, `arc`,
  `rounded_rect` and `mirror`. `arc_to` is the SVG endpoint-to-centre arc
  and emits cubics; `sqrt` joined the scalar expression subset so the HTML
  lid formula is expressible. `Driver.geometries` + `PartOutput.geometry` +
  `PartBase.geometry` carry and validate it.
- The DSL bakes geometry drivers into ordinary `ContentField::Geometry`
  keys, with a hold key before a gap. `RigEdit::GeometryKey/GeometryUnkey`
  plan `m.geometry(...)` calls; the editor verifies the compiled channel
  directly with a 1e-6 source round-trip tolerance and one undo step. The
  corpus now has 16 cases (`rig_geometry_key`).
- Golden: `crates/motioner_rig/tests/geometry.rs` reproduces the HTML lid
  formula (`d = -r + 2*r*lt`, `h = sqrt(r*r-d*d)`, `a/b` and the arc flags)
  point-for-point at `lt=0.5`; `arc_to` is also exercised through the DSL.
- Microbench (`geometry_bench`): bake 121 driven frames **0.056 ms
  (0.5 us/frame)**, sampling 121 keys **14.7 us (0.12 us/frame)**.
- Gate green; C: free 20.1 GB after the release bench (debug purged once).

E2 (2026-11-12):

- `motioner_dsl::rig_edit` defines `RigEdit` and `plan_rig_edit`: property
  set/key/unkey (mirror both/left/right), pose apply/save/delete, clip
  key/remove/length/play, state/transition/input edits. It finds the
  assignment that binds the rig root in the open program and walks only that
  block; declarations inside a used `.motion` module are refused with a
  reason (D-0057).
- `RigManifest` schema is **v2**: each property now carries its authored
  `keys` (frame/value/ease). `build_rig_manifest` populates them; JSON and
  the tool schema include them.
- `Editor::apply_rig_edit` plans, compiles once, compares the manifest with
  `apply_to_manifest`, checks that every node outside the rig subtree is
  unchanged and installs the program with `PreviewSession::install_compiled`
  as one undo step (`Strategy::RigEdit`).
- Eight `rig` tools are implemented: `rig_list`, `rig_manifest`, `rig_set`,
  `rig_key`, `rig_key_input`, `rig_apply_pose`, `rig_play_clip`,
  `rig_save_pose`. `Host::rig_edit` + `AppHost` route them to the editor.
- Corpus: 15 fixtures under
  `crates/motioner_dsl/tests/write_through/rig_*`, driven by
  `crates/motioner_app/tests/rig_edit.rs`; the tool contract test is in
  `crates/motioner_app/src/toolhost.rs`.
- Latency (release): property write-through **10.70 / 11.48 ms**
  (median/p90, 30 runs, showcase_pro); rig write-through **36.04 / 37.98 ms**
  (20 runs, mascot demo, one compile).
- T-RIG-0014 is closed. Gate green; C: free 7.2 GB at the E2 checkpoint
  (purge `target/debug` before the next heavy build).

E1 (2026-11-11):

- Resolver: `PreviewSession` owns one `Rc<dyn AssetResolver>`; all three
  compile paths (reload, override edit, write-through apply) call
  `compile_source`, which uses
  `motioner_dsl::run_with_overrides_and_metrics_and_assets`. The host injects
  it (`Editor::open_with_assets`, `PreviewOptions.asset_resolver`,
  `ProjectHost::asset_resolver`); `query`, `edit`, `bake`, `corpus` and the
  tool host pass `ProjectAssets`. No `motioner_io` dependency in
  `motioner_ui`.
- Manifest: `motioner_core::rig_manifest` (D-0056) is stored on `Document`;
  the DSL builds it after the bake from the same `RigEntry` list. It covers
  parts/nodes, properties (kind/angle/default/range/step/values/group/mirror),
  poses, pose tracks, clips, plays, blink layers, aims and the machine
  (states, transitions, inputs, events).
- CLI: `motioner query <file> --rig NAME|all` and the positional alias
  `query rig <file> <instance>` print `{"version":1,"rigs":[...]}`.
  `motioner tools --schema` carries the `rig_manifest` result schema
  (`implemented: false`; E2 wires execution).
- Test: `crates/motioner_app/tests/rig_manifest.rs` opens
  `examples/rig/mascot_demo.motion` through the headless editor session with
  `ProjectAssets` and asserts the document and manifest equal a direct
  `run_with_assets`, plus the query JSON and the unknown-rig error.
- Gate green; C: free 10.2 GB at the E1 checkpoint.

E0 (2026-11-11):

- Encoding: `scripts/check_encoding.py` reads index blobs and the BOM of
  changed files, excludes `engine/`, and is wired into the gate, README and
  CI; tests in `scripts/tests/test_check_encoding.py`. Worktree CRLF is
  normalised by `eol=lf` on commit, so only a committed CR is a failure.
- Non-scaling stroke: `PaintSpec::non_scaling_stroke: Option<f64>` (the
  placement scale the width is authored at), SVG `vector-effect` (inherited;
  the mascot root opts in), and `motioner_render` strokes those paths in
  composition space. The mouth no longer collapses under the 0.06 squash.
- Measurements (release, reference machine): interpreter on
  `showcase_pro.motion` (20 warm-up + 200 runs) median **1.205 ms**, p90
  1.462 (no prelude); 1.208/1.526 with prelude. Prompt 12 baseline 1.2105,
  budget +7 % = 1.2952: within budget. 50 mascots with a pose + clip +
  look keys: build+bake **238.0 ms**, 2000 nodes, **48 600** transform keys,
  **+8.9 MiB** RSS, preview **1.5 fps** at 1920x1080 (not the 60 fps target;
  rendering scales with instance count; E7 owns the investigation).
- Write-through latency: **pending E2**.
- T-RIG-0011 acceptance text corrected: the `motioner rig run` CLI was not
  delivered in prompt 16; E7 delivers it.
- Disk: the gate drove C: to 2.4 GB; stale `target/debug` was purged and the
  session closed at **20.7 GB free**. Keep `-j 1..2` and clean before big
  release runs.

## Prompt 16 status (rigs, symbols, state machines)

Session `S-2026-11-11-001`. **No commit**: the reviewer commits. Report
`out/reports/PHASE-16-REPORT.md` (Spanish). Tasks T-RIG-0001..0016;
ADR D-0051..D-0055; **no new external dependency**.

Delivered:

- `use(path)` (D-0051): SVG symbol, `.motion` module namespace, raster /
  audio / video / font definitions; metadata; sandbox; cycles; injected
  `AssetResolver` (`run_with_assets`). `motioner_io::ProjectAssets` is the
  file-system resolver; the CLI render/still/info paths use it. Modules
  import top-level names into the shared global scope (v0, documented).
- SVG reader in `motioner_io/src/svg.rs` (zero deps, D-0054): groups,
  paths with arcs to cubics, shapes, transforms flattened, paint, opacity,
  gradients, defs/use, clipPath -> D-0033 mask, viewBox. Every id is a part.
- `motioner_core::symbol` (`Symbol`, `SymbolPart`, `SymbolShape`).
- `motioner_rig` crate (D-0055, core + eval only): typed properties,
  channels, mirrors, driver IR (`driver.rs`), poses/clips/blinks, machine
  (`machine.rs`), aims, `bake`/`evaluate` (`eval.rs`).
- DSL scene child module `crates/motioner_dsl/src/scene/rigs.rs`: symbol
  instantiation (anchors, clip masks), `rig()`, `prop`, `key`/`set`,
  `mirror`, `driver` compiler, `pose`, `clip`, `play`, `input`, `state`,
  `transition`, `fire`, `aim`, `blink`, and the **bake** in `into_document`.
- **Bake-on-build** is the model (D-0052): rig output is written as node
  keys per frame with hold-before-change keys. Opacity is always written
  on the first touched frame (otherwise the evaluator's hold-first-key
  rule shows a later value early). Pose tracks start from the value the
  previous tracks produced; clips do not override a property before its
  first key; `Pose.reset` clears omitted properties (the HTML `expand()`).
- Mascot: `examples/rig/mascot.motion` (21 props, 6 drivers, 17 presets,
  2 clips, mood machine, blink, aim) and `examples/rig/mascot_demo.motion`
  (15 s, 1080p60). Screenshots in `out/reports/phase16/`.
- `docs/rigs.md` (Rive/Lottie comparison), `docs/imports.md`, spec §7.9
  and §10, README pointer.

Measured (release): mascot.svg import 0.10 ms; demo build+bake 31 ms for
901 frames (0.035 ms/frame, 4081 transform keys); 121-frame 1080p60 render
1.7 s including process + GPU + encode.

## Left (tasks)

- T-RIG-0014: resolver + rig manifest in the editor session (preview,
  query, edit, corpus, bake cannot compile `use()` today; `motioner_ui`
  must not depend on `motioner_io`, so the host injects it). Also the
  write-through corpus for rig property/pose/clip/machine edits.
- T-RIG-0013: bones, skinning, mesh/lattice, blend shapes, IK, follow
  path, spring; **path-geometry drivers** need a keyable geometry channel.
- T-RIG-0009: editor rig panel, graph view, timeline rows.
- T-RIG-0016: demo goldens + corpus/visual comparison.
- T-RIG-0015: wasm player, binary rig format, Lottie import/export,
  auto-rigging.

Known fidelity gaps of the mascot vs the HTML `apply()`: lids slide instead
of re-deriving arcs, mouth scales an authored lens (no curvature), no
perspective/skew or movable corners, no back-facing turn.

## Write-through essentials (unchanged)

Strategy ladder: private literal -> loop guard -> call site (argument,
returned node, parameter promotion) -> local assignment -> fallback
override. Every edit is verified by re-running; undo stores source +
overrides. `motioner_dsl` owns planner/printer/lint (strings); writes are
in `motioner_ui`/`motioner_app`.

## Project essentials

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu), branch `feat/m1-foundations`. Program-first
MDF builds the composition; scope/decisions are REF-0001..0004 and
D-0010..D-0062. Code is written by opencode with DeepSeek V4.1 Flash;
Claude is architect/auditor; the user speaks Spanish.

- CLI: `render`, `still`, `info`, `bake`, `preview --control`, `ctl`,
  `new`, `import`, `backup`, `migrate`, `query`, `edit`, `fmt`, `lint`,
  `trace export|replay`, `corpus check`, `tools --schema`, `tool`.
- Editor: projects, assets, timeline, graph editor, effects/masks/
  gradients/blend modes, typography (Inter), images, video proxy/cache,
  audio mixer, autosave/backup/recovery, write-through gestures,
  overrides, code panel, traces.
- Std CLI surface: `theme`, `use_theme`, `token`, `beat`, `bar`, `scene`,
  `comp_size`, `row`, `column`, `grid`, `stack`; node methods `place`,
  `bounds`, `t`.

## Gotchas

- **Host memory**: GUIMaker/FerrousEngine may build in parallel; pagefile
  commit can run out (`E0786`, 1455). Use `-j 1..2`; free space ended at
  13.7 GB. Stop below 5 GB.
- `cargo` with `-j 2` sometimes reports "can't find crate" under memory
  pressure; retry with `-j 1`.
- PowerShell 5.1 drops unescaped `"` in native args; to pass JSON use
  `--args '{\"frame\":30}'`. `Set-Content` has no `-NoNewline`.
- **PowerShell 5.1 `Set-Content -Encoding utf8` writes a BOM and mangles
  UTF-8 read as ANSI.** Use `[System.IO.File]::WriteAllText` or Python for
  files with non-ASCII (module accents).
- MDF has no `is`, dicts, `**kwargs` or `while`. Use `== None`.
- `scene.t(offset)` takes one argument (scene-local time).
- Clippy pedantic: new numeric modules carry documented `#[allow]` for
  pixel/sample casts.
- Windows ffmpeg lacks `-pattern_type glob`; use `seq%03d.png`.
- GPU tests serialize through `motioner_testkit::gpu::lock()`.

## Engine

Pin `7e95367` on `feat/pure2d-text-and-readback`; dev clone through
`6a30e7d`, `e482b3c`, `cab7035`, `6d1bc7a`, `5873863`, `a078f80`, `4582c42`,
`5687d18`, `a3ee934`, `2c23263`, `47d0cbc`, `e530596`, `8338d1c` (S4, pushed),
`493619f` (S5), `c2196d0` (S6), `23278a2` (P0), `9c3d6e3` (P1: 7022976,
ee2b772, 9d6218c, 11a5b3c, 74e85a2, 9c3d6e3), `c577877` (P2: e9761db,
c577877), `5c2fd29` (P3: cd547de, a13c9d3, a46a82e, 5c2fd29), `bd41224`
(P4: e8ed036, 3415098, 8b0e273, 56855c6, 48ca25b, bd41224), `db776bb`
(P5: GPU counters, state hoisting, overdraw view, damage-copy target, layered
pass timing, present modes, layer scale, 2D GUI-depth policy), `7e623a9`
(P6: lazy 2D target, adaptive blur-first layer scale, input-to-present and UI
damage trace, batched texture remap) and **`840a8b8`** (P7: segment coalescing,
offscreen present path, wasm-safe SSAO). The submodule is
checked out at `840a8b8`; the parent pin is not committed. The reviewer pushes the engine and
bumps the pin. **No pin bump in this session.** Never edit `engine/ferrous`.
The `svgparser 0.8.1` future-incompat warning predates this work. Engine
pre-existing debt remains: crate-wide rustfmt diffs (`asset_bridge.rs`,
`plugin.rs`, `renderer.rs`), broken rustdoc links (`render_context.rs`,
`traits.rs`), and pedantic clippy warnings in `ferrous_engine`/
`ferrous_ui_render` (P7).

## Goldens

All previous goldens unchanged and green. Prompt 16 deliberately did not
touch `motioner_eval`, `motioner_render` or `motioner_core` rendering paths,
so goldens could not move. Mascot screenshots are report artefacts, not
goldens (T-RIG-0016). E5b added `mesh_lattice_f030`, `mesh_video_skin_f030`,
`mesh_arm_ik_f030` and `mesh_blend_f030` under
`crates/motioner_testkit/golden/`; the default 2x2 quad remains on the sprite
path, so the older goldens are byte-identical.

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.
Cargo needs `-j 1..2`; `CARGO_INCREMENTAL=0`.

## Last session

`S-2026-11-17-001` (prompt 18): P0 instruments/baseline; P1 on-demand redraw;
P2 zero allocations per frame; P3 bytes/layout/ownership (memory hints, inventory,
font LRU, proxies, pointer opt-out) and **P4 CPU algorithms** (text allocation path,
per-image proxies with live-frame bypass and re-decode, O(1) layout cache, ASCII
path, retained engine batches, layout early-out, tessellation cache, atlas occupancy).
Heavy UI 3,154 -> 154 allocs/frame and 4.49 -> 4.06 ms collection; scrub back to
1.78 ms; MemoryHints A/B keeps 434/399 MB. Gate green, goldens byte-identical,
C: 5.63 GB free after cleaning both targets. **P5 GPU efficiency** (engine
`db776bb`): per-frame GPU counters, GUI/2D state hoisting (hover buffer binds
73 -> 25), 2D GUI depth off (-5.76 MB; start 18.6 MB VRAM), layered pass timing,
opt-in layer scale (showcase 61.8 -> 37.1 MB at 0.5), overdraw view, damage-copy
target (18.45x at 1600x900 / 102x at 4K for a 4% damage, rejected for now),
present modes (Fifo kept), and Motioner reload hygiene. P4 regressions do not
reproduce (720 frames hover, 8.41% scrub). Gate green, goldens byte-identical,
C: 5.43 GB free after cleaning both targets. Next: P6 (Motioner hot paths).

`S-2026-11-15-001` (prompt 17, open): S0 startup trace and bench (committed and
pinned), S1 font atlases (pinned), S2 lazy fonts in Motioner plus the
first-frame diagnosis of `preview`, S3 GPU startup (Vulkan-first backends,
overlapped probes, pipeline cache, background-coloured window at ~51 ms), S3b
first as the launch snapshot (rejected: the child window never painted and the
pixel check looked after the handoff) and then as **show the window when the
first frame is ready**, S4: no console window for the desktop binary
(GUI subsystem + `AttachConsole`), engine stdout routed through `log`, and
**lazy 3D in Flat2D** (renderer worker 52.8 -> 30.4 ms, first frame 591 ->
543 ms, 103.5 MB of VRAM no longer allocated), and S5: **assets and audio off
the preview first frame** (assets worker + `poll_assets`; audio worker +
prewarm; playback starts at frame 0 when the device is ready) and **a visible
close in 1.7-2.1 ms** with an engine `on_exit` hook that flushes overrides, the
session lock and app state after the hide. `preview` first frame 1434 -> 693.5
ms, start 538 ms, pre-window 0.94 ms. Engine `8338d1c..493619f` unpushed (S4 is
pushed); no Motioner commit.

Then **S6**: the regression guard (`startup_bench.py --check` + budgets, with
the window-before-first-frame rule), D-0062 and the startup docs, a console
companion binary (`motioner-cli.exe`, same library; `.com` alias documented),
`screen_check.py` robustness, the pure timeout test, the deleted
`Untitled.motioner/` and the final report. Final numbers (warm 20): start
first frame **471 ms** / visible **491 ms**; preview **624 / 650 ms**; close
hidden 1.8 (start) and 2.9 (preview); driver floor 438-484 ms. Engine S6
changes are uncommitted in the dev clone; no Motioner commit.

