# opencode prompt 17 — Startup performance: Motioner and FerrousEngine open in milliseconds (startup trace, font atlas cache, GPU init policy, lazy 3D, budgets)

You are the orchestrator of a long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`) and the FerrousEngine dev clone at
`C:\Users\jvchi\CARPETAS\FerrousEngine`. Run this prompt **after prompt 16b
is finished and committed**. Do not touch `C:\Users\jvchi\CARPETAS\GUIMaker`.

Goal: opening Motioner must feel instant. Today the window takes ~1.3 s to
show its first frame and the process ~1.8 s to finish a one-frame smoke run.
Most of that time is ours (font atlas baking) or avoidable (backend
enumeration, eager 3D resources), and the remaining GPU driver cost can be
overlapped with CPU work and hidden behind an immediately visible window.

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

- Window visible, painted with the app background colour, **no white
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

## Phase S4 — Lazy 3D resources in Flat2D (engine, `ferrous_renderer`)

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
   that fails when our own critical-path marks exceed a generous budget
   (e.g. 2× the S6 measured value), independent of the driver floor.
   Document how the reviewer runs it.
2. Decision record (D-0062, startup budget and policy: backend order,
   caches, what may run before the first frame). Engine docs page
   `startup.md` (trace, caches, backend policy, how apps keep startup
   fast) in `mkdocs.yml` nav. Motioner docs and CONTEXT numbers updated.
3. Final report **in Spanish** at `out/reports/PHASE-17-REPORT.md`: before
   and after table per mark (median/p90, warm and cold, NVIDIA and AMD),
   time to visible window, first frame, project open, close; VRAM saved;
   what is now the driver floor; engine commits to push; anything left in
   backlog with reasons.
