# opencode prompt 18 — FerrousEngine to the byte: measured, zero-waste, idle-free (instrumentation, idle = zero, zero allocations per frame, byte-level data, own algorithms, GPU efficiency, guards)

You are the orchestrator of a long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`) and the FerrousEngine dev clone at
`C:\Users\jvchi\CARPETAS\FerrousEngine` (branch
`feat/pure2d-text-and-readback`). Run this prompt **after prompt 17 S6 is
finished and committed**. Do not touch `C:\Users\jvchi\CARPETAS\GUIMaker`.
Task: `T-PERF-0005` (absorbs `T-PERF-0001`, budgets and benchmark plan).

## Goal

The user's standing rule: FerrousEngine must be **as optimized as
possible**: every byte has an owner and a reason, nothing is computed twice,
nothing is drawn when nothing changed, and every claim is a number. Motioner
follows the same rule; the engine is held to an even higher bar because
every app inherits it.

Today nothing outside startup is measured. The engine has one benchmark
(`ferrous_renderer/benches/pure2d_headless.rs`), no allocation accounting,
no per-stage frame timing, no GPU timing and no memory inventory.
Reference measurement by the reviewer (release, RTX 4050 laptop, start
screen, 1600×900, 2026-11-17, nothing happening on screen):

| Metric (start screen, idle 10 s) | Value |
|---|---|
| CPU time consumed | **797 ms per 10 s (~8 % of a core) while idle** |
| Working set | 336 MB |
| Private bytes | 457 MB |
| Threads | 45 |

A static start screen should cost ~0 CPU and ~0 GPU while idle, and far
less memory. Fix the cause, not the symptom: the engine itself must make
waste hard.

## Rules

Engine work goes to the dev clone as **generic** engine features (no
Motioner/MDF concepts): rustdoc, `crates/<crate>/docs` pages, `mkdocs.yml`
nav, engine tests and benches. Never edit `engine\ferrous` in the Motioner
repo; the reviewer pushes the engine and bumps the pin. Commit to the engine
clone on top of the current pin, no push; no commits in Motioner.

Do not commit, stage, stash, reset, checkout, rebase or push in Motioner:
the reviewer commits. Quality bar: top-tier studio. That means zero
duplication, rustdoc on public items, typed errors, no panics on runtime
paths, regression tests that fail without the change, and no warnings.
Never write files with a BOM or CRLF.

**Measure before and after every optimisation**, with the same scenario, in
release, and record the numbers in the phase report. An optimisation
without a number is not accepted. An optimisation that makes code
unreadable must win by a clear margin (state it). Keep a
`out/reports/phase18/ledger.md` table: change, scenario, before, after,
commit.

**Work one phase at a time and STOP after each phase.** Stop with the gate
green and a short phase report in Spanish at
`out/reports/PHASE-18-Pn-REPORT.md`. It must cover:
- what changed;
- the before and after numbers from the P0 harness;
- the engine commit range;
- the gate results;
- the free space on C:.

The user will reply "continúa con la fase N".

Gate: `cargo fmt -- --check` (never `--all`), `cargo clippy --workspace
--all-targets -j 2 -- -D warnings`, `cargo test --workspace -j 2
--no-fail-fast`, `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps
-j 2`, `python scripts/check_boundaries.py`, `python
scripts/check_encoding.py`, `cargo machete crates`, `python
.control/scripts/pctl.py validate`, `pctl doc-check-refs`. Use
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`
(8 GB machine, little free disk: run `cargo clean --release` between
phases if C: drops under 6 GB). Engine clone: its own gate (clippy default
and no-default, tests, wasm check, docs). To build Motioner against the
clone before the pin bump, point the local submodule at your engine commit;
no `[patch]` left behind.

Order:

```
P0 instruments + baseline ─► P1 idle = zero ─► P2 zero allocations per frame
   ─► P3 bytes: data layout and memory ownership ─► P4 CPU algorithms
   ─► P5 GPU efficiency ─► P6 Motioner hot paths ─► P7 guards, docs, report
```

P3 and P4 touch mostly disjoint code (formats/ownership vs
layout/text/tessellation) and may run as two subagents. The orchestrator
alone edits shared files and integrates.

## Phase P0 — Instruments and baseline (engine + scripts)

Nothing is optimised in P0. Build the instruments the rest of the prompt
depends on, all zero-cost when disabled.

1. **Allocation accounting.** Add a counting global allocator behind a cargo
   feature, e.g. `ferrous_core` feature `alloc-stats`. It counts allocations,
   reallocations, frees and bytes, globally and per frame scope. Expose it
   through an API such as `ferrous_core::alloc_stats::{snapshot, scope}`.
   Use it from engine tests and benches and from Motioner's bench binary;
   it is never on in normal release builds. Document the cost.
2. **Frame trace.** Extend the startup trace (`ferrous_core::startup`) into a
   per-frame stage timer, `FERROUS_TRACE_FRAMES=1`. Stages:
   - event handling;
   - app update;
   - UI layout;
   - render-command collection;
   - batch build;
   - prepare/upload;
   - encode;
   - submit;
   - present / wait.

   Per stage, record the median, p90 and p99, plus allocations and bytes
   when `alloc-stats` is on. Output is a JSON summary on exit. The
   disabled path costs one branch per stage.
3. **GPU timing.** Add timestamp queries per pass when the adapter supports
   `TIMESTAMP_QUERY` (and `TIMESTAMP_QUERY_INSIDE_PASSES` where needed),
   resolved with a lag of a few frames so the CPU never stalls. Report the
   GPU ms per pass in the same JSON. When the adapter lacks the feature,
   degrade with a clear "unsupported" field.
4. **Memory inventory.** Every GPU buffer, texture and atlas the engine
   creates is registered with a label, a byte size and its owning
   subsystem. `Renderer::memory_report()` returns the totals per subsystem
   and the largest items. The report also covers CPU-side copies the
   engine keeps: atlas pixels, decoded images, glyph tables and caches,
   with their byte sizes. The process working set and private bytes are
   sampled in the bench.
5. **Benches.** Extend the divan benches per subsystem (headless, no window):
   - UI layout of a large tree;
   - render-command collection;
   - `GuiBatch` build for N quads and N glyphs;
   - text layout and shaping for short and long strings;
   - MSDF glyph rasterisation;
   - path tessellation (lyon) for simple, complex and stroked paths;
   - Pure2D record and encode for a heavy scene;
   - SVG mesh upload.

   Each bench also reports allocations per iteration.
6. **Scenario harness** `scripts/perf_bench.py` (Motioner, release). Each
   scenario runs N times with a warm-up. For each scenario, report CPU% of
   one core, GPU ms/frame, frames drawn and the stage breakdown.
   Also report allocations per frame (a bench build with `alloc-stats`),
   working set, private bytes, VRAM from the inventory, and thread count.

   Scenarios:
   - start screen idle 10 s;
   - editor with the demo project idle 10 s;
   - editor hovering and moving the mouse;
   - timeline scrubbing;
   - 1080p playback of the demo for 10 s, with dropped frames;
   - a heavy UI: many panels and long text lists;
   - a heavy composition: many shapes, text, gradients, masks and effects;
   - an export of 120 frames (frames/s).

   Add `--check budgets.json` for P7.
7. **Baseline report.** Run every scenario and bench, plus a profile of the
   two most expensive scenarios. Use any sampling profiler available on
   Windows (e.g. `samply`, ETW/WPR or superluminal if installed); say which
   one. Rank the top 20 costs by measured share. Write
   `out/reports/PHASE-18-P0-REPORT.md` with the baseline table and that
   ranking; later phases re-run the same harness.

## Review findings after P0 (apply in the phase named)

- **P1 and P4, profiles by function.** The P0 ETW captures could not be
  symbolised (no PDB, no WPA), so the top-20 is by stage, not by function.
  Before optimising in P1 and again in P4, get function-level profiles:
  - build the bench binary with `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`
    so rustc emits a PDB (and `CARGO_PROFILE_RELEASE_STRIP=none`);
  - sample with a tool that resolves PDBs on Windows, e.g. `blondie` (ETW,
    writes folded stacks or a flamegraph; needs an elevated shell, so ask the
    user to run that one command) or `cargo flamegraph`, which uses blondie
    on Windows.

  Rank the top 20 by function with their shares, and keep the SVGs in
  `out/reports/phase18/`. Mind the disk: C: had 265 MB free after the
  reviewer's gate, so `cargo clean` before and after each profile build.
- **P2, per-thread allocation counts.** `alloc_stats` uses process-wide
  counters, so a `FrameScope` also counts the audio, decode, font and rayon
  workers. Add thread-local counters (or main-thread vs others) so the "0
  allocations per frame" tests measure the frame thread. Report worker
  allocations separately, with their own budget.
- **P1 and P4, heavy UI is CPU bound at ~100 % of a core.** Event handling
  takes 22.3 ms/frame and command collection 18.8 ms/frame, against 1.8 ms
  for the GPU pass. Start P1 by explaining why event handling costs 22 ms;
  a hover must not walk the whole tree. P4 items 1–2 (incremental layout,
  retained commands) target the 18.8 ms; record both in the ledger.
- **P3, memory inventory gaps.** The 3-D passes, intermediate layers and
  Motioner's CPU copies of images and glyphs are not in the inventory yet;
  close those gaps first in P3.
- **P7, engine lints.** `ferrous_engine` and `ferrous_ui_render` carry 27 and
  13 pre-existing pedantic clippy warnings, and `cargo doc` has warnings.
  Leave the engine with 0 clippy warnings at the engine's configured level and
  rustdoc `-D warnings` clean.

## Review findings after P1 (apply in the phase named)

- **P2 first: explain the heavy-UI memory jump.** Private bytes in heavy_ui
  went from 556 MB (P0) to 927 MB (P1), and the working set from 403 to 636
  MB, with no explanation. Before any P2 work, bisect it across the six P1
  engine commits and the Motioner changes (or prove it is the harness: the
  `--continuous` path, trace buffers, GPU timing query sets, the waker
  queue). Fix it, or account for every MB in the report. A change that adds
  370 MB without a reason is not accepted under this prompt.
- **P2/P6: per-frame editor logic.** The P1 function profile of heavy UI is
  topped by editor logic, not drawing:
  - `editor::rebind::infer_shift`;
  - `PreviewApp::frame_facts`;
  - `Editor::orphan_suggestions`;
  - `rebind::Provenance::parse`;
  - `CodePanel::build`;
  - `resolved_faces`/`ensure_faces`/`font_for`;
  - `layout_block`.

  These are derived from the document and must be computed once per document
  revision (or per selection change), cached, and reused by later frames.
  Their inputs are the revision, the selection and the panel size; key the
  cache on those. `siphash` shows up in the generic top: apply the fast-hasher
  rule here. `project_root` and `poll_proxy_status` run every playback frame;
  hoist them out of the frame.
- **P2: hover redraws.** The 1144 frames in 30 s of hover were measured
  before the pointer change and not repeated. Re-measure it with
  `--allow-pointer`. Implement the P1 item 2 rule: a pointer move that
  changes no hover target and no visible state does not request a frame.
  Report the frames per second of mouse movement over static UI; the target
  is 0 frames over empty or unchanged areas.
- **P6: playback pacing.** Playback is paced by a fixed 16 ms deadline and
  reaches 55.4 fps on 60 fps content. Pace by the composition frame rate
  against the presentation clock (vsync or `Fifo`), so the result is 0
  dropped frames at 60 fps when the frame cost allows. Report the dropped
  frames and the frame-time p99.
- **P2: file watcher.** `motioner-watch` sleeps 80 ms and stats every 250 ms
  (12.5 wakeups/s while idle). Use OS change notifications instead
  (ReadDirectoryChangesW through the `notify` crate, already in Cargo.lock
  via `ferrous_assets`), with a debounce, so an idle app has 0 periodic
  wakeups apart from the 30 s autosave.
- **Harness:** `p0-baseline.json` was overwritten by a default output path.
  Give each phase its own output name and refuse to overwrite a baseline
  file without `--force`.

## Review findings after P2 (apply in the phase named)

- **Fixed in review: `frame_facts` cache key.** The cache was keyed on the
  session generation and the view geometry, but `layer_flags` also reads
  the solo and shy sets. Those are view-only and do not bump the
  generation, so toggling solo/shy left the layer icons stale. The review
  added `Editor::layer_flags_revision` to the key. **Rule for every cache
  from now on:** the key covers every input the cached value reads, and a
  test changes each input once and asserts that the cache refreshes.
  Audit the P2 caches (`orphan_cache`, `code_panel_cache`, `facts_cache`,
  LivePainter faces, mesh index cache) against this rule in P3.
- **P3 first: the idle regression.** P1 measured start_idle at 0.03 % and
  editor_idle at 0.06 %. P2 reports 0.42 % (tail 0.52 %) and 0.16 % (tail
  0.21 %): 10× and 3× worse while the frame count stays at 1. Find what
  wakes or spins: the `notify` thread, the audio worker, the font bake
  worker, or the change in warm-up. Bring idle back to ≤ 0.05 % of a core,
  or explain each remaining wakeup with its source and its period.
- **P3/P4: static hover CPU.** Static hover costs 2.83 % of a core for 5
  frames in 30 s, so the per-`CursorMoved` path (`hover_at`, `tooltip`,
  the start-screen layout) is not free. Profile it and make a pointer move
  over a static area cost under 0.5 % of a core.
- **P3 baseline: export.** P0 export ran at 16.5 fps and P2 runs at 13.4 fps.
  The report attributes the difference to machine state. Prove it: run the
  P0 binary, the P1 binary and the current binary back to back in the same
  session. If the current binary is slower, bisect it.

## Execution model from P3 on (parallel subagents)

Phases P3 to P7 run with subagents to cut wall time. The main agent
orchestrates; subagents only edit.

1. **Main agent, before any edit:**
   1. measure the phase baseline with the harness (one named output file);
   2. read the code the phase touches and split it into **lanes**, each
      with an explicit list of the files it owns. No file belongs to two
      lanes. The engine clone and Motioner are always separate lanes;
   3. write the lane plan in the ledger: the lane, its files, its goal and
      the number it must move.
2. **Subagents, all lanes in parallel:**
   - edit only the files their lane owns. A lane that needs a change
     outside its files writes the request in its hand-off note and does not
     make the change;
   - run **no** cargo build, check, clippy, test or doc, because one shared
     target dir and the tight disk on C: cannot take parallel builds.
     `rustfmt <file>` on their own files is allowed;
   - re-read their diff before handing off: signatures and call sites
     match, every new cache key covers every input, and no `unwrap` on a
     frame path;
   - hand off a short note: the files changed, the intent of each change,
     the risks, the cross-lane requests and the tests to add or update.
3. **Main agent, after all lanes:**
   1. apply the cross-lane requests;
   2. run the full gate once and fix the failures (`cargo check` first,
      then clippy, tests, doc, the scripts);
   3. in parallel with the gate, run one read-only review subagent over the
      whole diff with the hand-off notes;
   4. measure the phase numbers with the harness and compare them with the
      baseline from step 1;
   5. write the report, then `cargo clean` and check the free space on C:.

A lane whose change fails the gate and cannot be fixed quickly is reverted
and reported. It does not block the other lanes.

## Phase P1 — Idle = zero (engine + Motioner)

When nothing changes, the app must not render, must not tick the GPU, and
must use ~0 CPU.

1. Make the engine's redraw model **event-driven by default for apps that
   opt in**. Today the runner redraws at `target_fps` unless
   `idle_timeout`/`idle_fps` are set, and Motioner sets neither. Add an
   invalidation API and a runner policy that redraws only when one of
   these happens:
   - input arrives;
   - the app calls `ctx.request_redraw()`;
   - a scheduled wake fires: `ctx.redraw_at(Instant)` for animations,
     caret blink, hover transitions and toasts;
   - an async job completes (font, image or video decode, worker results);
   - the window resizes, or its scale or theme changes.

   While any animation is active the frame rate is the display's vsync;
   otherwise the loop sleeps (`ControlFlow::Wait`/`WaitUntil`). Keep the
   current continuous mode for games and demos (`RedrawPolicy::Continuous`
   vs `RedrawPolicy::OnDemand`), and document both.
2. Use the existing `UiTree` damage tracking (`damage_regions`,
   `cached_cmds`) so that a UI state change marks exactly the nodes it
   affects, and an unchanged UI produces no new frame. Hover changes that
   alter nothing visible must not redraw.
3. In Motioner, switch to `OnDemand`. Playback, scrubbing, drag and video
   decode drive redraws; everything else wakes on events. Find and fix
   every place that relies on continuous redraw: polling of workers, audio
   align, autosave timers and file watchers. Each becomes an event, a wake
   deadline or a proxy event (`EventLoopProxy`) from the worker.
4. **Threads.** Audit the 45 threads: rayon pools, font workers, audio, the
   watcher, wgpu and drivers. Size the pools to the work. Idle workers
   block on channels, never spin or poll. Report the thread count and the
   idle wakeups per second (e.g. from ETW, or by counting the frames drawn
   plus worker loop iterations).
5. **Targets:**
   - start screen and editor idle: ≤ 0.5 % of a core, with 0 frames
     presented during 10 s of idle and 0 GPU submits;
   - hover across the UI: frames are drawn only when something visibly
     changes;
   - playback and scrub frame rates are unchanged or better;
   - a caret-blink test shows the exact number of frames scheduled.

   Add tests that drive the runner headlessly: an idle app presents 0
   frames in N simulated seconds, and a `redraw_at` produces exactly one
   frame at the deadline.

## Phase P2 — Zero allocations per frame (engine + Motioner)

Rule: in steady state, **a frame allocates nothing on the heap**. Buffers
are reused with `clear()`, grown geometrically, and shrunk only by an
explicit trim policy, e.g. after N frames at under 25 % use.

1. Known offenders, each with a regression test:
   - `GuiBatch` is rebuilt with `GuiBatch::new()`
     (`ferrous_gui/src/system.rs`). Keep one batch per surface and clear it
     each frame.
   - `text_utils::draw_text_baseline` allocates a `Vec` of quads per call.
     Write directly into the batch, or into a reused scratch buffer.
   - In `GuiRenderer::prepare`, SVG commands rebuild
     `all_vertices`/`all_indices` every frame. Keep scratch buffers, and
     upload only the meshes whose content or transform changed. A position
     offset belongs in the instance data or a uniform, not in a CPU re-bake
     of every vertex.
   - In `GuiRenderer::prepare`, the image bind group builds a `views` and
     `samplers` `Vec` on change. That is acceptable only on change; keep it
     out of the steady state.
   - Motioner `draw_image` mesh path (`motioner_render/src/shapes.rs`,
     `lib.rs`): reuse the vertex and index scratch buffers, and cache
     `MeshSample::indices()` per topology. This finding was already listed
     in prompt 16b E7. If E7 has not run yet, fix it here and mark it done
     there.
   - `Mesh::sample` clones `uvs` (and the points, for unkeyed meshes) on
     every call (`motioner_core/src/mesh.rs`). Return borrowed UVs, and
     sample into a caller-provided buffer.
   - Strings: no `format!`, `to_string()` or `String` building on frame
     paths (labels, ids, cache keys). Use interned ids, `&'static str` or
     pre-formatted text cached by value.
2. Sweep every per-frame path in `ferrous_engine`, `ferrous_renderer`,
   `ferrous_ui_core`, `ferrous_ui_render`, `ferrous_gui`, `ferrous_font`,
   and in Motioner `motioner_ui`, `motioner_render` and `motioner_eval`.
   Look for `Vec::new` and `collect`, `clone` of heap data, `HashMap`
   building, `Box`/`Arc` creation, and `String` work. Fix them all, or
   justify each with a number in the report.
3. Hash maps on hot paths use a fast non-cryptographic hasher (`rustc-hash`
   or `ahash`, one choice for the whole workspace, recorded in the
   decision), with keys that are integers or interned ids, not strings.
4. **Tests that fail without the change:** with `alloc-stats`, after 10
   warm-up frames, these scenarios allocate 0 times per frame:
   - the engine headless UI frame;
   - the Pure2D frame;
   - the Motioner headless editor frame;
   - the Motioner playback frame.

   If any allocation remains, the test lists it by call site; use a
   backtrace in the counting allocator when a debug flag is on. Any
   allowed exception is listed by name with its reason.

## Phase P3 — Bytes: data layout and memory ownership (engine)

1. **Vertex and instance formats.** For every pipeline, list the vertex and
   instance struct with its byte size and the count per frame in the heavy
   scenarios. Compress where precision allows:
   - colours as `Unorm8x4` (or `Unorm10_10_10_2`), not 4×f32;
   - UVs as `Unorm16x2` or f16;
   - local positions as f16/i16 with a per-draw origin where the range
     allows;
   - flags packed into bit fields;
   - `u16` indices when a draw has under 65 536 vertices.

   Prove pixel identity: the engine goldens and Motioner goldens stay
   byte-identical, or document a sub-LSB difference that the user accepts.
   Report the bytes per frame uploaded before and after.
2. **Uploads.** Use a staging ring or `wgpu::util::StagingBelt` (measure it
   against `queue.write_buffer`), and keep large per-frame uploads in one
   buffer with sub-allocation. Upload only the dirty ranges.
3. **Textures.**
   - Atlases use the smallest format that holds the data: R8 for coverage
     and SDF, RGBA8 only where MSDF needs it (measure MTSDF vs MSDF).
   - Mipmaps only where they are sampled minified.
   - No duplicate textures for the same asset (dedup by content hash).
   - Video frames and images are sized to their displayed size in the
     preview (proxies), at full size only for export.
4. **CPU copies.** After a GPU upload, drop the CPU pixels of atlases and
   images unless they are needed: readback, re-pack or export. State who
   needs them. Glyph and layout caches have byte budgets with LRU
   eviction, and they appear in the inventory.
5. **Ownership.** Every cache in the engine has a byte budget, an eviction
   policy and an inventory entry; there are no unbounded growth paths. Add a
   test that runs the heavy scenario for N frames and asserts that the
   inventory is flat after warm-up, so no leak can hide.
6. **Targets:** set them from the P0 inventory and state them before
   optimising. The expectation is that private bytes for the idle start
   screen at least halve (457 MB today), and VRAM for the editor drops
   measurably. Explain every remaining item over 5 MB.

## Phase P4 — CPU algorithms (engine; own algorithms where they win)

For each item: benchmark the current implementation, design the
replacement, benchmark again, and keep it only if it wins clearly. Our own
algorithm is welcome when it beats the library on our workloads; when it
does not, keep the library and say so.

1. **Incremental UI layout.** Only dirty subtrees are re-laid out, with
   layout results cached per node and constraint. Use the
   `ferrous_ui_core::spatial_index` for hit testing and visibility culling:
   off-screen nodes in scroll views are neither collected nor batched.
2. **Retained render commands.** `cached_cmds` per node is reused when the
   node is clean, and batch segments for clean subtrees are reused as
   ranges instead of being rebuilt.
3. **Text.**
   - Shaping and layout are cached by (font, size, features, text hash),
     with a byte budget.
   - Glyph runs are reused across frames.
   - Measure-only paths (the layout engine asking for a text width) never
     build quads.
   - Line breaking is incremental for edits in text fields.
   - Measure our own ASCII/Latin fast path against the general shaper.
4. **Tessellation.**
   - Tessellate paths in local space once per geometry change, keyed by a
     geometry hash, and apply the transform on the GPU.
   - For strokes and fills that only change by transform, never
     re-tessellate.
   - Consider analytic GPU rendering (SDF rounded rects, circles, lines and
     arcs drawn in the fragment shader without tessellation) for the common
     UI primitives. Measure it against lyon.
5. **Atlas packing.** Measure the occupancy and repack cost of the glyph and
   image atlases. Try a better packer (skyline or guillotine with
   best-fit) if occupancy is under ~70 %.
6. **Data-oriented hot loops.** Where a loop runs over thousands of items
   (batch build, culling, transform), use a struct-of-arrays layout,
   branch-free inner loops and `chunks_exact`, and let auto-vectorisation
   work; check it with the benches. Use `rayon` only where a bench shows a
   win on the 6-core laptop without hurting frame pacing.

## Phase P5 — GPU efficiency (engine)

1. **Draw calls and state changes.** Report draw calls, pipeline switches,
   bind group switches and buffer binds per frame for each scenario. Sort
   and merge them: one pipeline per primitive class, a texture array or
   bindless-style slots (already partly there with `MAX_TEXTURE_SLOTS`),
   and instancing everywhere it applies. Targets per scenario go in the
   report.
2. **Overdraw.** Measure it with a debug overdraw view (an engine feature,
   toggled at runtime). Draw opaque UI quads front to back with early-z
   where it pays off, skip fully covered nodes, and avoid full-screen
   passes when the effect covers a small rect.
3. **Partial redraw.** Use P1's damage rects to scissor-limit the encode to
   the changed region. Measure "persistent offscreen target + copy of the
   damaged rect + present" against a full redraw at 1600×900 and 4K. Keep
   it only if it wins, and document the result.
4. **Pass budget.** Report the GPU ms per pass for the heavy composition.
   Optimise the top three: shader cost, MSAA only where needed, and
   resolution of the intermediate targets.
5. **Present.** Measure input-to-photon latency and frame pacing for
   Fifo, FifoRelaxed and Mailbox, and choose per platform with a documented
   reason.

## Phase P6 — Motioner hot paths

1. Evaluation per frame (`motioner_eval`): measure the evaluation of the
   heavy composition. Cache the results of static subtrees between frames.
   An unchanged node is not re-evaluated; property tracks with no key in
   the range are constant.
2. `motioner_render` (`tools.rs`, `paths.rs`, `fonts.rs`, `frame.rs` have
   the most allocations and clones): apply the P2 rules and the P4 caches.
3. Timeline and panels: virtualise long lists, so only the visible rows
   are laid out and painted. Thumbnails are uploaded once and drawn from
   the atlas.
4. Playback: report dropped frames at 1080p60 and 4K30 (proxy), CPU and GPU
   per frame, and decode-ahead hit rate. Export: frames per second for the
   120-frame export, with readback using a ring of mapped buffers so the
   GPU never waits on the CPU.

## Phase P7 — Guards, docs, report

1. **Budgets as tests.**
   - `scripts/perf_bench.py --check budgets.json` fails on regressions
     beyond the noise margin measured in P0.
   - The zero-alloc tests from P2, the idle tests from P1 and the flat
     inventory test from P3 run in the normal gate.
   - Benches have recorded baselines, and the reviewer runs them with a
     documented command.
2. **Decision D-0063, "Performance policy":**
   - on-demand redraw;
   - zero allocations per frame;
   - byte budgets for caches;
   - compact formats;
   - the hasher choice;
   - when our own algorithms replace libraries;
   - how to measure.

   Close `T-PERF-0001` as superseded by it.
3. **Docs.**
   - An engine page `performance.md` in the mkdocs nav: the instruments,
     the rules for contributors (with examples of right and wrong frame
     code), how to read the frame trace and the memory report, and the
     redraw policies.
   - Motioner docs and CONTEXT numbers.
4. **Final report** in Spanish, `out/reports/PHASE-18-REPORT.md`:
   - a before and after table per scenario: CPU%, frames while idle, GPU
     ms, allocations per frame, private bytes, VRAM, threads, draw calls,
     dropped frames and export fps;
   - the ledger of every optimisation with its numbers;
   - what was tried and rejected, with the numbers;
   - the engine commits to push;
   - the backlog, with reasons.
