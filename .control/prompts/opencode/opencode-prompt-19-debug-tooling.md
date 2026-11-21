# opencode prompt 19 — Real debug tools, zero cost when off (frame debugger with draw-call stepping, profiler timeline, visual views, inspectors, capture and replay, Motioner debug tools, headless debug CLI, guards), built as editor panels

You are the orchestrator of a long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` and the FerrousEngine dev
clone at `C:\Users\jvchi\CARPETAS\FerrousEngine`. Run this prompt **after
prompt 18 P7 is reviewed and merged into `main`** in both repositories. Work
on the branches the reviewer names in the start message (default:
`feat/debug-tooling` off `main` in both repositories). Do not touch
`C:\Users\jvchi\CARPETAS\GUIMaker`. Task: `T-DEBUG-0001`. Create it with
`pctl` in P0.

## Goal

The user's standing rule has two halves:
- FerrousEngine and Motioner are **as efficient as possible**;
- both have **real debug tools**: interactive instruments a developer
  drives, the way RenderDoc, PIX, Unreal Insights and the Unreal widget
  reflector work. A log or a print is not a debug tool.

The user's example sets the bar: in the renderer you can step through a
frame **draw call by draw call** and see each one in parts. That means its
state, its inputs, what it wrote, and who issued it.

This prompt also lays the base for the next big product: **an editor in the
spirit of Unreal**, built on FerrousEngine. Every tool here is built as a
reusable **panel**. For now it is hosted in an overlay; later the editor
docks the same panels with no rewrite.

Today the debug surface is a set of environment variables that write JSON
on exit:
- `FERROUS_TRACE_FRAMES`, `FERROUS_TRACE_STARTUP`, `FERROUS_OVERDRAW`;
- `FERROUS_LAYER_SCALE`, `FERROUS_MEMORY_HINTS`, `FERROUS_PRESENT_MODE`;
- `MOTIONER_TRACES`.

Next to them sit the memory inventory, the GPU pass timestamps, the draw
and bind counters, the damage and input-to-present traces, and
`scripts/perf_bench.py`. There is useful data underneath, but no tool on
top of it.

## Rules

Everything from prompt 18 still applies:
- **Engine work:** generic engine features with no Motioner/MDF concepts.
  Add rustdoc, `crates/<crate>/docs` pages, the `mkdocs.yml` nav, engine
  tests and examples. Commit in the engine clone, no push.
- **Motioner:** never edit `engine\ferrous` in Motioner, and make no
  commits there; the reviewer commits, pushes the engine and bumps the pin.
- **Quality bar:** top-tier studio. Zero duplication, typed errors, no
  panics on runtime paths, regression tests that fail without the change,
  and no warnings.
- **Files:** no BOM and no CRLF.

Debug-specific rules:
1. **Tools, not logs.**
   - Every feature is something you open, point at, step through, pin or
     compare.
   - Data is recorded only when a tool needs it, only while that tool is
     armed, and scoped to frames, never as an always-on stream.
   - Do not add `log::debug!`/`println!` output as a "debug feature".
   - Existing JSON outputs stay, as the data behind the tools.
2. **Panels for the future editor.** The tools live in a new engine crate,
   `ferrous_devtools`. Each tool is a panel type with three parts: a data
   source, a view, and the actions it exposes.
   - Panels only draw through the engine UI (Ferrous draws everything; no
     second toolkit).
   - A **host** places them. P0 builds the overlay host (dockable inside the
     app window), and the future editor (`ferrous_editor`) is a second
     host.
   - No panel knows which host it is in.
3. **One registry.**
   - Every tool and view is a named toggle in `ferrous_core::debug`. It can
     be set from four places, in this order of precedence: environment >
     `ferrous-debug.toml` > hotkey/menu > default.
   - The existing variables become aliases.
   - Nothing reads `std::env` for debugging outside the registry.
4. **Zero cost when off.**
   - A disabled tool costs one branch per pass or per frame, never per
     draw or per widget.
   - It costs no allocation, no GPU resource and no thread.
   - Everything sits behind a cargo feature, `debug-tools`: on in dev and
     in Motioner's release build, and compiled out in a shipping build.
   - Proven by the prompt 18 budgets with the feature on and all tools off,
     and with the feature compiled out.
5. **A tool must not change what it measures.** Tool UI is drawn in its
   own debug pass after the app pass, and its draws, allocations and time
   are counted apart from the app's.
   - The frame debugger captures a frame and then inspects the recording,
     so the app keeps running untouched.
   - An open panel on an idle screen does not force frames.
6. **Every tool earns its place.** Each phase ends by using its tools on
   Motioner, the demo project and the perf fixtures. It reports what they
   revealed, fixes the top three issues, and adds the numbers to the
   ledger.
7. **Agents are users too.** What a panel shows can also be read as JSON
   through the CLI (P7) and through Motioner's live control, so the agent
   harness can debug without pixels.

**Work one phase at a time and STOP after each phase.** Stop with the gate
green and a short report in Spanish at `out/reports/PHASE-19-Pn-REPORT.md`.
It must cover:
- what changed, with screenshots of each tool under `out/reports/phase19/`;
- the zero-cost numbers;
- what the tools revealed and what was fixed;
- the engine commit range;
- the gate results;
- the lowest free space on C: during the phase.

Keep `out/reports/phase19/ledger.md` (tool or fix, scenario, before, after,
commit). The user will reply "continúa con la fase N".

Gate: the same as prompt 18 (fmt without `--all`, clippy `-D warnings`,
tests, doc `-D warnings`, boundaries, encoding, machete, `pctl validate`,
`pctl doc-check-refs`). Run it in a clean worktree, and paste the last line
of each command in the report. Engine gate: clippy default and
no-default, tests, wasm check, docs, plus a build without `debug-tools`.
Use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0` and `-j 2`. Never run two cargo processes at
once. **Disk floor:** check C: before every build and stop below 12 GB to
free space first.

Order:

```
P0 devtools crate + registry + host ─► P1 frame debugger ─► P2 profiler
   ─► P3 visual views ─► P4 inspectors ─► P5 capture and replay
   ─► P6 Motioner debug tools ─► P7 headless debug CLI ─► P8 guards, docs, report
```

## Execution model (parallel subagents)

Same as prompt 18 from P3 on.
1. **Main agent, before any edit:** measure the baseline, split the phase
   into lanes with disjoint file lists (the engine clone and Motioner are
   always separate lanes), and write the lane plan in the ledger.
2. **Subagents:** edit only, with no cargo (`rustfmt <file>` is allowed),
   and re-read their own diff before handing off. Each hand-off note lists
   the files, the intent, the risks, cross-lane requests and the tests to
   add.
3. **Main agent, after all lanes:**
   1. integrate the cross-lane requests;
   2. run one gate in a clean worktree, with a read-only review subagent
      over the diff in parallel;
   3. measure;
   4. write the report;
   5. run `cargo clean` and report the free space.

## Phase P0 — Devtools crate, registry and host (engine)

1. **Inventory.** List every existing debug and trace facility in both
   repositories: what it shows, its data, and its cost when off. Record
   which of them become the data source of which tool. Put the table in the
   P0 report.
2. **`ferrous_core::debug` registry.**
   - Named toggles: bool, enum and number, each with a description, a
     category and a default.
   - Live values (counters, gauges, timers with history rings) that
     subsystems publish once and tools read.
   - A `set`/`get`/`list` API.
   - Loading from `ferrous-debug.toml`, and the environment aliases.
3. **`ferrous_devtools` crate.**
   - The `Panel` trait (data source, view, actions, persisted state).
   - The **overlay host**: panels dock left, right or bottom, float or
     stay pinned inside the app window, with a layout saved per app.
   - The **debug menu**, where apps register their own panels and actions.
   - The hotkey layer, configurable and off in shipping builds. Defaults:
     - F12: toggle the host;
     - Ctrl+Shift+D: the menu;
     - Ctrl+Shift+C: capture a frame;
     - Ctrl+Shift+R: start/stop input recording.
   - The debug render pass that every tool draws in.
4. **The `debug-tools` feature,** and a test that a disabled tool allocates
   nothing (`alloc-stats`).
5. **Proof:** the prompt 18 budgets with the feature on and all tools off,
   and with it compiled out. Both must be within noise of `p7-after.json`.

## Phase P1 — Frame debugger (engine) — the centrepiece

Capture one frame and take it apart, draw call by draw call, inside the
running app. Ctrl+Shift+C arms a capture of the next frame. The app keeps
running and the panel inspects the recording.

1. **Recording.** While a capture is armed, the engine's pass and draw
   recording layer (the wrapper the 2D, GUI and 3D renderers already go
   through) stores every operation:
   - pass begin and end, with attachments, load/store ops and clear values;
   - pipeline set, with the shader module, entry points, vertex layout,
     blend, depth/stencil and primitive state;
   - bind groups, with every binding: the buffer and its range, or the
     texture view or sampler;
   - vertex and index buffers with their ranges;
   - scissor, viewport, push or uniform data, and the draw parameters
     (vertex, index and instance counts and offsets).

   Every operation keeps its **attribution**: the engine subsystem, the UI
   widget id or the scene node id that issued it, and a short label. When
   no capture is armed, the cost is one branch per pass.
2. **Event browser.** A tree of passes, each with its draws, plus search and
   filters by subsystem, pipeline or texture. Selecting a draw shows its
   complete state from step 1 in readable sections. Uniform buffers are
   decoded by name and type where the shader reflection (naga) gives the
   layout, with a raw hex view as fallback.
3. **Step through the frame.** A slider and the keys `[` and `]` move over
   the draws. The engine replays the captured frame up to the selected
   draw into an offscreen copy of the target, and shows:
   - the target after that draw;
   - the pixels this draw wrote, highlighted (a diff against the previous
     step);
   - its geometry as a wireframe over the result;
   - the bounding box of its vertices.

   "Isolate" renders only the selected draw.
4. **Pixel history.** Click a pixel of the target. The panel lists every
   draw that touched it, with the colour before and after each one, and
   whether the draw was discarded, clipped or covered.
5. **Resource viewers.**
   - **Textures:** any texture or view bound in the capture, with zoom, mip
     level, channel masks, value range, and the value under the cursor.
   - **Buffers:** decoded through the draw's vertex layout as a table of
     vertices, with a mesh preview.
6. **Shaders.** The WGSL source of the selected pipeline, with the entry
   points and bindings. Edit the shader and re-run the captured frame with
   the edit (**shader hot reload** in the capture). Errors are shown inline.
7. **Per-draw GPU time.** In capture replay only, put timestamp queries
   around each draw where the adapter supports them, and show the time next
   to each draw and per pass.
8. **Captures as files.** "Save capture" writes the recording and its
   referenced resource contents to a folder that P5 and P7 can open later
   without the app.

Use it on Motioner's editor, showcase_pro and heavy_ui. Report the draws
that should merge, redundant state changes, oversized targets and wasted
pixels, and fix the top three.

## Phase P2 — Profiler (engine)

An interactive timeline in the spirit of Unreal Insights or Tracy, built on
the frame trace, the GPU timestamps and `alloc-stats`.

- **Frame bar chart:** the last N frames, scrollable. Click a frame to open
  it.
- **Opened frame:** CPU stages as nested spans per thread, the GPU passes on
  their own track, present wait, and the frame's allocations. Hover for
  exact times; drag to zoom.
- **"Why did this frame draw":** the redraw reason as a marker on each frame
  (input, hover change, an animation tick, `redraw_at` with its source
  name, resize, an asset arriving). Click a marker to jump to the code
  source name.
- **Recording:** recording is armed by the panel and is ring-bounded.
  Nothing is recorded while the panel is closed.
- **Compact HUD:** one line with fps, p99, allocations per frame, VRAM and
  draws, pinned in a corner, as a panel of its own.
- **Export:** to a Perfetto/Chrome trace, so it opens in `ui.perfetto.dev`.

Use it on the idle, hover, scrub and playback scenarios. Report every
redraw that should not exist and every stage over budget, and fix the top
three.

## Phase P3 — Visual debug views (engine)

Each view is a toggle in the registry and the menu, drawn in the debug
pass:

- overdraw (exists: move it into the registry);
- damage rects, flashed for N frames;
- UI layout bounds: margin, padding and content, with flex/grid lines;
- clip and scissor rects;
- batch segments: each draw call in its own colour, labelled with its
  pipeline and texture slot, and click-through to the draw in the frame
  debugger;
- the atlas viewer: texture and glyph atlas pages, fill, eviction age, and
  hover for an entry's key;
- the render-target and layer viewer: thumbnails of every intermediate
  target, with its size, format, scale and bytes;
- a tessellation wireframe for paths and SVG meshes, with vertex and
  triangle counts per node;
- layer scale: tint layers that render below 1.0 and show the scale;
- for 3D, so the 3D vision has tools from day one: depth, normals,
  wireframe, light-linking masks and the per-object shading mode.

Use them on the heavy fixtures, and report and fix the top three findings.

## Phase P4 — Inspectors (engine)

- **Widget reflector:** hover to pick, click to pin. Shows:
  - node id, type and path;
  - layout rect and style;
  - the retained-cache state (hit or miss, and which key part changed);
  - the last repaint reason;
  - the draws it produced, linked to the frame debugger.

  It includes a tree view with search. Editing a style value live changes
  the running UI until it is reset.
- **Cache inspector:** every engine cache (layout, text, tessellation,
  glyph, image, batch ranges) with entries, bytes, budget, hit rate and
  evictions per second. Clicking an entry shows its key and the owner.
- **GPU resource inspector:** every buffer and texture from the inventory,
  with its label, size, usage, creation frame and last-used frame. It opens
  in the texture or buffer viewer, and flags resources unused for more than
  N seconds.
- **Input and focus inspector:** the widget under the pointer, the focus
  chain, the hover chain and the active capture, live. It is not an event
  log: it shows the current state and the last change.

Use them, and fix the top three findings.

## Phase P5 — Capture and replay (engine)

- **Offline frame debugger:** open a saved capture (P1) in the engine
  example `devtools_viewer`, with no app needed.
- **Input recording and deterministic replay** (Ctrl+Shift+R). Record the
  window events with timestamps, and replay them at the same timing or as
  fast as possible. A replay that diverges stops at the first frame that
  differs and opens both in the frame debugger. `perf_bench.py` can use
  recordings as scenarios.
- **RenderDoc and PIX hook:** when one of them is attached, capture
  triggers it too, so the external tool sees the same frame.
- **GPU validation** toggle.
- **Device-loss and panic capture:** keep the last 300 frames of profiler
  data and the registry state, and write them as a capture that the
  profiler can open.

## Phase P6 — Motioner debug tools (Motioner, with engine requests)

Motioner registers its own panels in the host and the menu. Each one is a
tool, not a log.

- **Evaluation inspector:**
  - per root: cache hit or miss, and the reason for each miss (revision,
    keyed range, an input not in the key);
  - plan stats: items, layers, culled items and effect layers;
  - evaluation time per root.

  Click a root to select it on the canvas.
- **DSL compile profiler:** a timeline of parse, plan, MDF compile and the
  steps inside it, with the 20 most expensive nodes. Each node links to the
  code panel. Use it to confirm the prompt 18 compile-scaling fix.
- **Panel cache view:** per-panel retained-range hit or miss with the key
  that changed, and per-panel damage drawn on the window.
- **Media inspector:**
  - image proxies: size, target size, hysteresis state, failed keys;
  - the video decode-ahead queue: depth, hit rate, late frames;
  - audio sync drift;
  - the export ring: slot state and readback waits.
- **Canvas views:**
  - item AABBs, with culled items ghosted;
  - transforms, pivots and origins;
  - mask and matte coverage;
  - effect-layer bounds and blur margins;
  - mesh wireframe;
  - rig bones;
  - the live state-machine state, with its transitions and inputs.

  Clicking an item on the canvas opens its draws in the frame debugger.
- **"Why is this value this?"** Select a property in the inspector to see
  the value at the current frame. Show the contributing source: the DSL
  line from provenance, the keyframe or track, the expression, the override
  or the rig. Each of these opens the code panel at the line.
- **Agent access:** each panel's data is also queryable through
  `motioner_tools` live control as JSON (`debug.get`, `debug.set`,
  `debug.snapshot`).

Use them on the demo project, showcase_pro and the perf fixtures. Report
and fix the top three findings.

## Phase P7 — Headless debug CLI (Motioner + engine)

`motioner-cli debug <subcommand>` works on a project without a window and
writes JSON by default, or a table with `--pretty`:
- `plan --frame N`: the evaluated plan;
- `eval-cache --frames A..B`: hit/miss per root;
- `compile-profile`: DSL stage times and the top nodes;
- `memory`: the memory report after rendering frame N headless;
- `frame --frame N --out folder`: a frame-debugger capture of a headless
  render;
- `draws --capture folder`: the draw list with state and attribution;
- `trace --frames A..B --out file.json`: a Perfetto trace;
- `why <node>.<property> --frame N`: the value source chain.

Add the engine example `devtools_viewer` (from P5), and the example
`debug_tools`, which turns every engine tool on over a demo scene. Every
subcommand gets a test on a small fixture.

## Phase P8 — Guards, docs, report

1. **Guards in the normal gate.**
   - The prompt 18 budgets pass with `debug-tools` on and every tool off.
   - The engine build without `debug-tools` compiles and passes its tests.
   - The test that a disabled tool allocates nothing.
   - The idle test with the host open: no frames forced while values are
     static.
   - The frame-debugger replay of a captured frame matches the live frame
     pixel for pixel at its last draw.
   - Overhead budgets with tools on, each measured and recorded: compact
     HUD ≤ 0.2 ms/frame CPU, each visual view ≤ 0.5 ms/frame GPU at
     1600×900, and capture of one frame ≤ 50 ms.
2. **Decision D-0064, "Debug tooling policy":**
   - tools, not logs;
   - panels and hosts, and the path to the editor;
   - the registry and its precedence;
   - zero cost when off, and how it is proven;
   - the debug pass;
   - "a tool must not change what it measures";
   - agent parity through JSON;
   - what ships in a shipping build.
3. **Docs.**
   - An engine page, `debugging.md`, in the mkdocs nav: every tool, its
     hotkey and its cost, with a walkthrough of the frame debugger
     (stepping, pixel history, shader edit) and the profiler, and how an
     app adds panels.
   - A Motioner page, `docs/debugging.md`.
   - CONTEXT numbers.
4. **Final report** in Spanish, `out/reports/PHASE-19-REPORT.md`:
   - every tool with a screenshot and its cost on and off;
   - the findings each tool revealed and the fixes, with numbers;
   - the engine commits to push;
   - the backlog, including what the Unreal-style editor needs from
     these panels (docking, a project browser, a scene outliner, a
     details panel).
