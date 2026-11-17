---
actualizado: 2026-11-16
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

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
- **Measured** (release, 1600x900, warm 20): start first frame **468.2 ms**,
  window visible **489.9** (p90 500.3), wall 687.4, path 34.5, renderer worker
  27.7, close hidden 4.8; preview first frame **609.3**, visible **630.9**
  (p90 652.3), wall 825.2, path 128.8 (document load 28.7, canvas 90.1, paint
  65.0), close hidden 2.2. Cold (fonts+pipelines cleared, 5): start 507.3/
  532.8, preview 655.8/683.5. AMD 760M (`WGPU_POWER_PREF=low`, 5): start
  444.6/459.7, preview 579.0/597.8. Driver floor: surface configured 433.7
  (start) / 482.7 (preview); instance 203/250, device 45/43, swapchain 185/190.
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
  ignored. Gate green (see the report); C: GB free recorded there.

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
`5687d18`, `a3ee934`, `2c23263`, `47d0cbc`, `e530596`, `8338d1c` (S4, pushed)
and **`493619f`** (S5 exit hook; the submodule is checked out there locally).
S6 leaves **uncommitted** engine changes in the dev clone on top of `493619f`
(`runner/show.rs`, `runner/frame.rs`, `ferrous_core/startup.rs`, `docs/
startup.md`); the reviewer commits/pushes them and bumps the pin. The S6
measurements and guard were run with a temporary `../FerrousEngine` path
override (removed afterwards). **No pin bump in this session.** Never edit
`engine/ferrous`. The `svgparser 0.8.1` future-incompat warning comes from the
engine and predates this work.

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
first frame **468 ms** / visible **490 ms**; preview **609 / 631 ms**; close
hidden 4.8 (start) and 2.2 (preview); driver floor 434-483 ms. Engine S6
changes are uncommitted in the dev clone; no Motioner commit.

