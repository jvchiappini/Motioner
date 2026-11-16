# opencode prompt 17 — Startup performance: Motioner and FerrousEngine open in milliseconds (startup trace, font atlas cache, GPU init policy, show when ready, lazy 3D, budgets)

You are the orchestrator of a long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`) and the FerrousEngine dev clone at
`C:\Users\jvchi\CARPETAS\FerrousEngine`. Run this prompt **after prompt 16b
is finished and committed**. Do not touch `C:\Users\jvchi\CARPETAS\GUIMaker`.

Goal: opening Motioner must feel instant and never look half-loaded: the
window appears only when its first real frame is ready (≈ 500 ms on the
reference machine), complete, with no white/black flash. Today the window takes ~1.3 s to
show its first frame and the process ~1.8 s to finish a one-frame smoke run.
Most of that time is ours (font atlas baking) or avoidable (backend
enumeration, eager 3D resources), and the remaining GPU driver cost can be
overlapped with CPU work. The window stays hidden until the first frame is
presented (phase S3b, "show when ready"), with a safety timeout for slow
cold starts.

Engine work goes to the dev clone as **generic** engine features (no
Motioner/MDF concepts): rustdoc, `crates/<crate>/docs` pages, `mkdocs.yml`
nav, engine tests, and an example where it helps. Never edit
`engine\ferrous` in the Motioner repo; the reviewer pushes the engine and
bumps the pin.

Do not commit, stage, stash, reset, checkout, rebase or push: the reviewer
commits. Quality bar: top-tier studio; zero duplication, rustdoc on public
items, typed errors, no panics on runtime paths, regression tests that fail
without the change, no warnings. Never write files with a BOM or CRLF.

**Work one phase at a time and STOP after each phase** with the gate green
and a short phase report (what changed, before/after numbers from the S0
bench, gate results, free space on C:). The user will reply "continúa con la
fase N".

Gate: `cargo fmt -- --check` (never `--all`), `cargo clippy --workspace
--all-targets -j 2 -- -D warnings`, `cargo test --workspace -j 2
--no-fail-fast`, `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps
-j 2`, `python scripts/check_boundaries.py`, `python
scripts/check_encoding.py`, `cargo machete crates`, `python
.control/scripts/pctl.py validate`, `pctl doc-check-refs`. Use
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`
(8 GB machine). Engine clone, when touched: its own gate. To test engine
changes from Motioner before the pin bump, use a temporary `[patch]` or
path override pointing at the dev clone and remove it before the phase
report (say so in the report).

Order:

```
S0 trace + bench ─► S1 font atlas cache (engine) ─► S2 lazy fonts (Motioner)
   ─► S3 GPU init policy + overlap + instant window (engine)
   ─► S3b show when ready (engine + Motioner; replaces the launch snapshot)
   ─► S4 lazy 3D in Flat2D (engine) ─► S5 Motioner open/close paths
   ─► S6 budgets, docs, report
```

S1 and S4 touch disjoint engine crates (`ferrous_font` vs
`ferrous_renderer`/`ferrous_engine`) and may run as two subagents; the
orchestrator alone edits shared files and integrates.

---

## Reference measurements (reviewer, 2026-11-15)

Machine: Windows 11, RTX 4050 Laptop (Vulkan, driver 32.0.15.9282) + AMD
Radeon iGPU, 8 GB RAM. Release build, `motioner --start
--exit-after-frames 1`, warm runs, times from process start. Obtained by
temporarily adding `eprintln!` markers to the engine (reverted).

| Phase | Time | Notes |
|---|---|---|
| `main` → window created | ~45 ms | winit |
| `wgpu::Instance::new(Backends::all())` | ~245 ms | Vulkan alone ~175 ms (NVIDIA ICD load) |
| `request_adapter` | ~250 ms | enumerates Vulkan + DX12 + GL; Vulkan alone ~7 ms |
| `request_device` | ~30 ms | |
| `surface.configure` | ~190-220 ms | NVIDIA swapchain on a hybrid laptop; AMD iGPU ~150 ms |
| `Renderer::new` (Flat2D app!) | ~65 ms | builds 8 PBR/instancing pipelines, world pass + 2048² shadow map, SSAO, prepass, skinning, gizmos, **1 000 000-particle system** |
| engine UI font atlas (Roboto, `latin_western`) | ~115 ms | MSDF baked on CPU every launch |
| Motioner `setup()` → `LivePainter::with_fonts` | **~620-680 ms** | bakes 4 atlases (Inter 400/600/700, Roboto 400); Roboto is **baked twice** (also the engine UI font) |
| first frame (update + draw + present) | ~30 ms | fine |
| process teardown after exit | ~160 ms | device/swapchain drop; makes closing feel slow |

Other facts:
- `WGPU_BACKEND=dx12`: `Renderer::new` takes **7.4 s** (shader compilation,
  no cache). DX12 must never be the silent default path while it costs that.
- `WGPU_BACKEND=gl`: panics (exit 101) after surface configure.
- `VK_LOADER_LAYERS_DISABLE=~implicit~` (Steam, Overwolf, OBS, Epic overlay
  layers are installed) saves only ~15 ms: not a lever.
- `EngineContext` creates a `PipelineCache` with `data: None`: nothing is
  persisted, so every run recompiles. A cold run on the AMD iGPU spent
  1.4 s in `Renderer::new`.
- `ferrous_font::FontAtlas::new` already uses rayon; ~0.5 ms per 64² glyph
  per core is slow for MSDF.
- The first run after every rebuild pays ~350 ms before `main` (Defender
  scanning the new exe). Measure warm runs; report cold separately.

## Targets (this machine, warm, release)

- **The window appears only with its first real frame** (start screen
  fully drawn), on a real screen capture: never white, black or partially
  drawn. Safety timeout: if the first frame is not presented by ~800 ms,
  show the window with the start-screen background colour.
- When the timeout fires (cold driver start): window, painted with the app background colour, **no white
  flash**: ≤ 100 ms from process start.
- First presented frame with the start screen fully drawn: ≤ 500 ms, of
  which our own CPU work on the critical path ≤ 50 ms; the rest must be the
  GPU driver floor (instance + swapchain), measured and named.
- Opening a project from the command line (`preview <project>`): first
  frame with the composition drawn ≤ 700 ms for `examples/demo.motioner`.
- Closing the window: it disappears ≤ 50 ms after the close request.
- Cold run (empty caches) not slower than today's warm run.
- No visual change: every existing golden byte-identical.

If a target turns out impossible because of the driver, say so with
numbers and what the floor is; do not fake it.

---

## Phase S0 — Startup trace and benchmark harness

1. **Engine (generic):** a startup trace in `ferrous_core` (or the crate
   that fits best): monotonic spans/marks from process start, zero cost
   when disabled, enabled with `FERROUS_TRACE_STARTUP=1` or
   `AppConfig::with_startup_trace(true)`. Engine marks at least: window
   created, instance, adapter, device, surface configured, renderer built,
   fonts ready, app setup done, first frame presented, close requested,
   window hidden, teardown done. Public API so apps add their own marks
   (`startup::mark("motioner: fonts")`). Output: one table on stderr at
   the first present, plus an optional JSON line for tools. Docs page +
   test.
2. **Motioner:** marks for pre-window work (`Host::new`, recents,
   examples), font registration per face, document load in `preview`.
3. `scripts/startup_bench.py`: runs the release binary N times (default
   20) for `--start --exit-after-frames 1` and `preview
   examples/demo.motioner --exit-after-frames 1`, parses the JSON trace,
   prints median/p90 per mark and the time to first present (not process
   exit). Wire it as an optional script, not in the gate. Record the
   baseline in `.control/CONTEXT.md` and compare with the table above.

## Phase S1 — Font atlases: cache, dedupe, faster bake (engine, `ferrous_font`)

1. Split the CPU bake from the GPU upload: a pure-data `BakedAtlas`
   (pixels, metrics, kerning, notdef...) built without a device, and an
   upload step. `FontAtlas::new` keeps its behaviour on top of both.
2. Persistent disk cache of `BakedAtlas`, keyed by a hash of (font bytes,
   charset, glyph size, px range, generator version). Versioned binary
   format, atomic write (temp + rename), a corrupt/truncated/stale file is
   ignored and rebuilt (never a panic), bounded size with LRU eviction.
   Cache directory configurable on the app config; default per-OS cache
   dir (`%LOCALAPPDATA%\<app>\cache` on Windows); can be disabled. Keep
   dependencies minimal and justify any new one.
3. Dedupe: registering the same bytes + charset twice returns the same
   atlas (shared, not baked or uploaded twice), for both the UI font and
   `register_font_2d`.
4. Faster cold bake: profile `msdf_gen`, then optimise (e.g. per-edge
   bounding-box culling / spatial binning for the distance search, avoid
   per-pixel allocations). Target ≥ 3× faster cold bake of `latin_western`,
   output identical or within a documented tolerance with a test that
   proves rendered text goldens are unchanged.
5. Optional, decide with numbers: lazy per-glyph baking (bake ASCII first,
   the rest on demand or on a worker) if it beats the cache on cold runs.
6. Tests: cache hit equals a cold bake byte-for-byte; corrupt file
   rebuilds; dedupe returns the same id; version bump invalidates.

## Phase S2 — Motioner: fonts only when needed

1. The start screen needs only the UI font. Composition faces (Inter
   400/600/700, Roboto, project faces) are registered when a document needs
   them: at document load on a worker (bake through the S1 cache), and
   synchronously only for a face a visible text node uses before the
   worker is done. No text may render with the wrong face, even for one
   frame (test it).
2. Roboto 400 reuses the engine UI atlas (S1 dedupe); no second bake.
3. `CpuTextMetrics` / `FontSet::bundled()`: check their cost (parsing,
   `to_vec()` copies of embedded bytes) and make them lazy or borrowed if
   they show in the trace.
4. Target: fonts on the start-screen critical path ≤ 5 ms warm.

## Phase S3 — GPU init policy, overlap and instant window (engine)

1. **Backend policy:** stop requesting `Backends::all()` by default.
   Per platform ordered list (Windows: Vulkan, then DX12; macOS: Metal;
   Linux: Vulkan; GL only as last resort), trying the next only when no
   adapter is found. Configurable on the app config and still honouring
   `WGPU_BACKEND`. Document the DX12 shader-compile cost and either fix it
   (pipeline cache / precompiled shaders) or keep it as fallback only. Fix
   or report the GL panic.
2. **Persist the pipeline cache:** load `PipelineCache` data at start and
   save `get_data()` after the first frames and at exit, keyed by adapter
   vendor/device/driver version, atomic write, invalid data ignored (wgpu
   `fallback: true`). Measure cold vs warm `Renderer::new` and first frame
   on both GPUs.
3. **Overlap:** start instance + adapter creation on a worker thread as
   soon as `App::run` begins (it does not need the window), while winit
   creates the window; start font baking (S1) at the same time on another
   worker; join at upload. Nothing that only needs the CPU waits for the
   GPU.
4. **Instant window:** the window must be visible ≤ 100 ms with the app
   background colour and no white flash. Investigate the winit/Win32
   options (window class background brush, `with_visible(false)` until
   the first clear present, presenting a clear frame as soon as the
   surface is configured, before the renderer and fonts are ready) and
   pick the one with the best measured time to a correctly coloured
   window. Same idea on other platforms where it applies.
5. Keep `HighPerformance` as the default power preference but make it
   configurable; report the iGPU numbers for the record.
6. Tests where possible (backend order logic, cache key, corrupt cache
   file); the rest measured with S0.

## Phase S3b — Show the window when it is ready (replaces the launch snapshot)

Decision (user, 2026-11-16): at ~500 ms to the first frame, the app must
not show anything until it is complete. The CPU-painted launch snapshot
committed in the engine as a3ee934 is **removed**: it failed on the real
screen (white client area: the boxed paint state was passed as
CreateWindowExW lpParam but never stored in GWLP_USERDATA, so WM_PAINT drew
nothing and WM_NCDESTROY leaked it) and it adds unsafe Win32 code, a
5.5 MB write per launch and stale-screen risk for little gain.

1. **Remove the snapshot** from the engine (child window, file format,
   capture/request API, key, docs, tests) and from Motioner (start-screen
   key, cache dir use). Delete the `%LOCALAPPDATA%\<app>\cache\launch`
   folder on first run of the new build (best effort). Keep what is still
   useful from S3b (queued input during startup only if it is needed for 3).
2. **Show when ready (engine, generic, default on):** the window is created
   hidden; GPU init, renderer, fonts, app setup and the first frame run
   while it is hidden; the window is shown right after the first frame is
   presented, so its first visible content is that frame. Verify on
   Windows that a swapchain on a hidden window presents correctly and that
   DWM shows the presented frame on `ShowWindow` (no stale, white or black
   frame); otherwise present the first frame again right after showing,
   and measure. Same idea on the other platforms.
3. **Safety timeout:** configurable (`AppConfig`, default ~800 ms from
   process start); if the first frame is not presented by then, show the
   window with the S3 background (brush + clear) so the user knows the app
   opened. Input that arrives before the first frame is delivered in order
   afterwards, and close works at any time.
4. **Background colour:** Motioner passes `with_background_color(black)`
   while the start screen paints the theme pasteboard colour; use the
   start-screen background colour for the brush and the clear so the
   timeout path does not switch colours either. Review the S3 GDI brush
   (one brush per colour, no leak).
5. **Real screen verification:** move the reviewer's
   `out/reports/phase17/reviewer_screen_check.py` to `scripts/` (or a
   `--screen-check` mode of `startup_bench.py`, outside the gate):
   launch, find the largest visible top-level window of the PID, capture
   the desktop (PIL ImageGrab, a real desktop capture, not a readback) at
   the moment the window first becomes visible and at +16/+50/+150 ms.
   Pass: the first visible capture already shows the drawn start screen
   (matches a capture taken 1 s later within a documented tolerance), never
   a uniform white/black client area. Also run it with the timeout forced
   low (e.g. 50 ms) to verify the background-colour path, and with
   `WGPU_BACKEND=dx12` (shader compile: the window must appear at the
   timeout and must not be marked "Not Responding"; keep pumping messages
   or document the limit with numbers).
6. Measure with S0: time to window visible (= first frame), timeout path,
   process time, before/after S3. Update docs (`startup.md`), CONTEXT and
   the S3b report (rewrite `out/reports/PHASE-17-S3B-REPORT.md`, saying
   what the earlier pixel check actually captured).

## Phase S4 — Lazy 3D resources in Flat2D (engine, `ferrous_renderer`)

0. First, two user-visible fixes (user request after trying the S3b build):
   (a) No console window for the desktop app. Double-clicking `motioner.exe`
   opens a cmd window next to the app because the binary uses the console
   subsystem (it is also the CLI). Build `motioner_app` with
   `#![windows_subsystem = "windows"]` (release and debug) and, at the top of
   `main`, call `AttachConsole(ATTACH_PARENT_PROCESS)` so that, when it is
   started from a terminal, `motioner render/still/info/...`, `--help` and
   errors still print to that terminal and exit codes still work (check
   stdout/stderr piping and redirection to a file as well: when the std handles are
   already valid, do not replace them). Double-click and `--start` show
   no console. Document the trade-off (a shell does not wait for a windows-
   subsystem exe unless it is started with `start /wait` or from scripts; our
   scripts use subprocess and are fine) in the Motioner docs. Verify by double-click,
   from PowerShell, from cmd, and via `scripts/startup_bench.py`. (b) Stdout
   noise from the engine: `ferrous_ui_render/src/gui_renderer.rs` prints
   `[GuiRenderer] Render/Base Scissor/First Segment` every 120 frames through
   a `static mut` counter, `Rebuilding pipelines` on every pipeline rebuild,
   and `ferrous_gpu/src/context.rs` prints `[WGPU] Selected Adapter` with
   `println!`. Remove the per-frame prints (and the `static mut`); route the others
   through `log::debug!`/`log::info!`. Grep the engine and Motioner for other
   `println!`/`eprintln!` on runtime paths (e.g. Motioner `desktop: enter`) and move them to `log`
   or the startup trace. CLI output that is the command's result stays.

1. `Renderer::new` in `RendererMode::Flat2D` builds only what 2D needs.
   PBR/instancing pipelines, world pass and shadow map, material registry
   GPU parts, SSAO + prepass, skinning, gizmos, particles (and its
   1 000 000-particle buffer) are created on first 3D use or mode switch.
   Report time and VRAM saved (allocated buffer/texture bytes before and
   after).
2. Switching to 3D at runtime works; existing 3D examples and engine
   goldens unchanged; a test that a Flat2D renderer allocates none of
   these resources and that the first 3D draw creates them.

## Phase S5 — Motioner open and close paths

1. Pre-window work ≤ 10 ms (`Host::new`, state, recents, examples scan:
   no file content reads that the first screen does not show).
2. `preview <project>`: trace document run, image/video decode, fonts,
   first composition paint; move what is not needed for the first frame
   off the critical path (decode on workers, show placeholders without
   layout shift, then swap in). Same for opening a project from the start
   screen.
3. Close: hide the window first, then tear down; skip or background
   expensive drops at process exit where it is safe (flush saves and the
   pipeline cache first). Target ≤ 50 ms to a hidden window.

## Phase S6 — Budgets, docs, report

1. Regression guard: a smoke test (or `scripts/startup_bench.py --check`)
   that fails when the window becomes visible before its first frame (outside
   the timeout path), or when our own critical-path marks exceed a generous
   budget
   (e.g. 2× the S6 measured value), independent of the driver floor.
   Document how the reviewer runs it.
2. Decision record (D-0062, startup budget and policy: backend order,
   caches, show-when-ready and its timeout, what may run before the first
   frame). Engine docs page
   `startup.md` (trace, caches, backend policy, show when ready, how
   apps keep startup fast) in `mkdocs.yml` nav. Motioner docs and CONTEXT numbers updated.
3. Reviewer findings to fix here: (a) after S3, `show_with_background`
   creates a new GDI brush on every window creation and replaces the class
   brush without deleting the previous one (one leaked brush per extra
   window); keep one brush per colour or delete the replaced one. (b) After
   S2, decide with numbers whether to trim the engine UI glyph set (its
   atlas is 10 ms warm, 37 ms cold). (c) [moved to S4 step 0; verify only] After the S3b review: double-
   clicking `motioner.exe` opens a black console window next to the app
   (console subsystem, because it is also the CLI). With no arguments (or
   `--start`) the desktop run must show no console: e.g. build as the
   windows subsystem and `AttachConsole(ATTACH_PARENT_PROCESS)` for the CLI
   commands, or `FreeConsole` early in the desktop path when the console was
   created for this process only (`GetConsoleProcessList` == 1). Keep stdout
   working when run from a terminal; verify by double-click and from a shell.
   (d) `runner/show.rs` module docs still say the instance/device/swapchain are
   built on a worker; since S3b only `Renderer::new` is, fix the docs and
   state that the timeout cannot fire while `init_device` blocks the loop
   (~430 ms warm; a cold driver can block longer, document it). (e)
   `scripts/screen_check.py` takes the reference capture after the process
   has exited when `--exit-after-frames` is small (one reviewer run compared
   against the terminal behind it and failed); take the reference while the
   window is still visible or refuse arguments that exit before it, and
   mask taskbar previews / ignore captures where another window covers the
   app. (f) `the_timeout_can_be_overridden_from_the_environment` mutates a
   process env var in a parallel test run; make `resolved_timeout` take the
   value as a parameter in the test or serialize it.
4. Final report **in Spanish** at `out/reports/PHASE-17-REPORT.md`: before
   and after table per mark (median/p90, warm and cold, NVIDIA and AMD),
   time to visible window (= first frame), timeout path, first frame, project open, close; VRAM saved;
   what is now the driver floor; engine commits to push; anything left in
   backlog with reasons.
