# opencode prompt 07 — Motioner: first editor window (live preview) + engine fix-ups

You are the orchestrator of a long, multi-phase job in two repositories:

- **Motioner** at `C:\Users\jvchi\CARPETAS\Motioner` (branch
  `feat/m1-foundations`): an AI-first motion-graphics editor in Rust. Today it
  is a CLI only (`motioner render|still|info`). This job gives it its **first
  window**: a live preview editor that opens an `.motion` (MDF) file, plays it,
  scrubs it, shows its scene tree and reloads it when the file changes.
- **FerrousEngine dev clone** at `C:\Users\jvchi\CARPETAS\FerrousEngine`
  (branch `feat/pure2d-text-and-readback`): two small fix-ups found in review
  (Part A). Motioner does **not** see this clone; it uses the pinned submodule
  `Motioner\engine\ferrous`. Part A and Part B are independent.

The quality bar is a top-tier studio: zero duplication, rustdoc on every
public item, typed errors, no panics on runtime paths, regression tests that
fail without the change, no warnings. Work through every phase in order and
run the phase gate before moving on. Do not stop early unless a stop
condition applies. The user wants to **see and use a window** at the end:
the last phase is running it on this machine and reporting what you saw.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the two folders above. Never edit
  `Motioner\engine\ferrous` (pinned submodule, decision D-0018); engine
  changes go only to the FerrousEngine clone.
- Do not commit, stage, stash, reset, checkout, rebase or push in either
  repository. The reviewer commits.
- Build with `CARGO_INCREMENTAL=0` **and `-j 2`** (`cargo build -j 2 …`,
  `cargo test -j 2 …`). This machine has 8 GB RAM; more parallel jobs fail
  with `os error 1455` ("paging file too small") while linking. That error
  is memory, not code: rerun with `-j 1` if it appears.
- Free space on C: is about 13 GB. If it drops below 8 GB, stop and report.
  Only one release build is allowed (the final smoke run, phase 9).
- Motioner: run `cargo fmt` (never `cargo fmt --all`: it would format the
  engine) and `cargo machete crates` (never at the root).
- FerrousEngine is not fmt-clean: run `rustfmt --edition 2021 --config
  skip_children=true` only on files you edit. 25 files are stored with CRLF
  in its index (`git ls-files --eol | grep crlf`): keep their CRLF endings.
- No new dependencies in either repository unless a phase allows one
  explicitly. No `thiserror` in library crates (hand-written `Display` +
  `Error`); `anyhow` only in `motioner_app`. No new `unsafe`.
- Determinism: rendered pixels depend only on the document and the integer
  frame index (rational fps). The preview is the **only** place where a
  wall clock is allowed, and only to choose *which* frame index to show
  during playback; it never changes what a frame looks like. Write that rule
  in the `motioner_ui` crate docs.
- Subagents may run phases in parallel only where this prompt says so, each
  owning only the files listed for it.

## 1. Verified facts (read the cited code before changing it)

Motioner:
- Workspace crates: `motioner_core` (document model), `motioner_dsl` (MDF
  parser/interpreter), `motioner_eval` (`Evaluator`, `RenderPlan`),
  `motioner_render` (engine adapter, `FrameRenderer`), `motioner_io` (PNG,
  ffmpeg MP4), `motioner_app` (CLI, `crates/motioner_app/src/main.rs`),
  `motioner_testkit` (golden `golden/hello_f120.png`). 131 tests green.
- Root `Cargo.toml` declares engine path deps
  (`engine/ferrous/crates/...`); `ferrous_renderer` is used with
  `default-features = false` (D-0019). Today only `motioner_render` may use
  `ferrous_*`/`wgpu`/`glam`.
- `scripts/check_boundaries.py` enforces that: `FORBIDDEN_PREFIXES`,
  `ALLOWED_PREFIXES = {"motioner_render": ("wgpu", "ferrous")}`,
  `FORBIDDEN_EDGES` per crate. `motioner_ui` already appears in the
  forbidden edges of other crates but does not exist yet.
- `motioner_render` (`crates/motioner_render/src/{lib,frame,shapes,text,space}.rs`):
  `FrameRenderer` owns a `Pure2dHarness` (headless, own device), the
  embedded Roboto font (`FontId2d`) and a per-node missing-glyph cache.
  Emission is written against the harness: `shapes::draw_item(..)` calls
  `harness.draw_shape(..)` (`shapes.rs:53-118`), `frame::draw_text` calls
  `harness.draw_text(..)` (`frame.rs:142-176`), `frame::render` drives
  `begin_frame` / items / `render_frame` / `read_pixels_into`
  (`frame.rs:56-120`). The coordinate contract lives in `space.rs`
  (`composition_flip`, `shape_model`, `model_mat4`, `z_for`, `rgba`) and
  `text.rs` (`local_flip`, `text_model`). The public API exposes no engine
  type; keep it that way for `FrameRenderer`.
- `examples/hello.motion` is a 1920×1080, 60 fps, 5 s composition with text,
  circles and rotating groups. Use it for every manual run.
- ADRs live in `.control/decisions/` (last is D-0019). Create new ones with
  `python .control/scripts/pctl.py decision-new "<title>"` and fill the
  file. D-0004 says the editor UI is built on "Ferrous UI and GUIMaker"
  and that `motioner_ui` depends on GUIMaker widgets. GUIMaker today is a
  standalone designer binary that does **not** build against the current
  engine (it imports `ferrous_app`, which no longer exists), so it cannot
  be a dependency yet.

FerrousEngine (pinned submodule, read-only from Motioner; same APIs exist in
the clone):
- `ferrous_engine` (`engine/ferrous/crates/ferrous_engine`) is the app
  runner: `App::new(state).with_title(..).with_size(..).with_mode(AppMode::Flat2D).run()`
  (`src/builder.rs`), the `FerrousApp` trait with `setup`, `update`,
  `configure_ui(&mut ferrous_gui::UiSystem<Self>)`, `draw_ui(&mut DrawContext)`,
  `on_resize`, `on_window_event` (`src/traits.rs:122-173`), `AppContext`
  with `input`, `time`, `window_size`, `render: RenderContext`
  (`src/context.rs`), and `RenderContext::{enable_pure_2d, renderer_mut,
  set_background_color, frame_2d_stats}` (`src/render_context.rs`).
  `AppMode::Flat2D` runs the renderer in the 2D fast path with no 3D sync.
  It depends on winit 0.30, `ferrous_gui`, `ferrous_ui_core`,
  `ferrous_ui_render` and the default (`gui`) features of
  `ferrous_renderer`.
- `ferrous_ui_core` is a retained-mode UI (`UiTree`, theme, layout via
  `ferrous_layout`/taffy) with widgets in `src/widgets/`: `button`,
  `slider`, `label`, `panel`, `split_pane`, `tabs`, `tree_view`,
  `virtual_list`, `scroll_view`, `toggle_switch`, `number_input`,
  `progress_bar`, `separator`, `viewport` (`ViewportWidget`: reserves a rect
  for engine rendering and records `resolved_rect`), `image`, `tooltip`,
  `toast`, `dock_layout`. `ferrous_gui::UiSystem` has builders such as
  `button(label)` (`ferrous_gui/src/system.rs:151`).
- `ferrous_editor` (`engine/ferrous/crates/ferrous_editor/src/app/*.rs`) is a
  working example of a `FerrousApp` with `draw_ui` (immediate text draws via
  `dc.gui.draw_text(font, ..)`) and `configure_ui`. Read it to learn the
  real API before writing UI code; do not guess signatures.
- Pure2D: `Renderer::draw_2d_shape(ShapeInstance)` and
  `Renderer::draw_text_2d(font, text, Mat4, size_px, align, color)` are what
  the harness methods call (`headless.rs`, `draw_shape`, `draw_text`). World
  space: origin at the window centre, Y up, 1 unit = 1 logical pixel.
  `begin_2d_view` multi-view segments cover shapes only (not text), and the
  multi-view path has a known bug (Part A, phase 1): **do not use 2D views
  in the preview**; draw with the single main camera.

---

## 2. Phases

### Part A — FerrousEngine fix-ups (subagent E, parallel with phases 2–3)

Owner: the FerrousEngine clone only:
`assets/shaders/shape_2d.wgsl` (camera uniform + triangle fb mapping),
the camera-uniform struct and its writers in
`crates/ferrous_2d/src/render/{types,renderer}.rs`, every other 2D shader
that declares the same camera uniform (`sprite_2d.wgsl`, `text_2d.wgsl`,
`grid_2d.wgsl`, `composite_2d.wgsl`), `crates/ferrous_renderer/src/headless.rs`
(ring state only), the layer code around the single runtime `expect` in
`crates/ferrous_renderer/src/renderer_core.rs`, and new tests under
`crates/ferrous_renderer/tests/`.

**Phase 1 (engine).**
1. Multi-view hard-edge bug: in `shape_2d.wgsl` `vs_main`, the triangle path
   maps NDC to framebuffer space with `camera.resolution` only
   (`fa = ((n.x*0.5+0.5)*res.x, (1-(n.y*0.5+0.5))*res.y)`), but in the
   multi-view path (`renderer_core.rs`, `rpass.set_viewport(x, y, w, h, ..)`
   around line 2584, per-view camera written by
   `ferrous_2d/src/render/renderer.rs` around line 466 with
   `resolution = view size`) `@builtin(position)` is in **full** framebuffer
   coordinates. Any view whose origin is not (0, 0) clips the hard edges of
   mixed triangles at the wrong place. Add the viewport origin (physical
   pixels) to the 2D camera uniform (keep the uniform 16-byte aligned; update
   the Rust struct, every writer and every shader declaration in one go — one
   WGSL struct definition shared by concatenation if the shaders already
   share code that way, otherwise keep them byte-identical and add a test
   that asserts the Rust struct size matches), and use it in the mapping.
   Regression test: two side-by-side views (the second at x = width/2) each
   drawing the same mixed triangle fan; the right view's pixels equal the
   left view's pixels shifted by width/2. It must fail before the fix (say so
   in the report).
2. Ring cleanup in `headless.rs`: `SlotState::Ready` and `SlotState::Failed`
   are assigned and immediately overwritten with `Free`
   (`take_readback_indexed`), and `Copying { submission }` duplicates
   `PendingReadback::submission`. Reduce the state to what is actually used
   (for example `busy: bool` per slot, or `Free | InFlight`), remove the
   duplicated submission and the "defensive" fallback, keep the public
   behaviour and docs truthful. All ring tests must still pass.
3. Replace the runtime
   `.expect("ensure_layer_resources must create the layer resources")` in
   `renderer_core.rs` with a code path that cannot panic (return early with
   a `log::error!` and skip the layer, or restructure so the resource is
   returned by `ensure_layer_resources`).
4. Engine gate (section 4B).

### Part B — Motioner preview window

**Phase 2 — decision and boundaries (orchestrator, no Rust yet).**
1. Create ADR D-0020 "First editor window on the engine-native UI stack"
   with `pctl decision-new`. Content: `motioner_ui` is a new library crate
   that may depend on `ferrous_engine`, `ferrous_gui`, `ferrous_ui_core`,
   `ferrous_2d`, `ferrous_renderer`, `wgpu`, `glam`, `winit` (only through
   the engine's re-exports if possible; list exactly what you depend on);
   it uses the engine's UI widgets directly because GUIMaker cannot build
   against the current engine; GUIMaker integration is deferred to
   T-ARCH-0007. State which parts of D-0004 this corrects (the GUIMaker
   dependency and "viewport rendered into a texture": the first window draws
   the composition directly in the window's Pure2D pass, see phase 4) and
   what stays (no egui, one device, one event loop).
2. `scripts/check_boundaries.py`: allow the engine prefixes for
   `motioner_ui` in `ALLOWED_PREFIXES`; add `motioner_ui` to
   `FORBIDDEN_EDGES` (it may depend on `motioner_core`, `motioner_dsl`,
   `motioner_eval`, `motioner_render`; not on `motioner_io`, `motioner_ai`,
   `motioner_app`, `motioner_testkit`); `motioner_app` may depend on
   `motioner_ui`. Update the script's docstring. Run it: it must pass.
3. Root `Cargo.toml`: add `ferrous_engine` (and any other engine UI crate
   you need) as workspace path deps with a comment that only
   `motioner_render` and `motioner_ui` may use them. Note that Cargo unifies
   features: `ferrous_renderer` will be built with `gui` when `motioner_ui`
   is in the build. `motioner_render` must still build and pass its tests
   alone (`cargo test -j 2 -p motioner_render`), which uses
   `default-features = false`.

**Phase 3 — one emission path for headless and live (subagent R, parallel
with Part A).**
Owner: `crates/motioner_render/**`.
1. Introduce a private trait in `motioner_render`, for example
   `trait Canvas2d { fn shape(&mut self, ShapeInstance); fn text(&mut self, FontId2d, &str, Mat4, f32, TextAlign, [f32; 4]); }`,
   implemented for `Pure2dHarness` and for `ferrous_renderer::Renderer`.
   `shapes::draw_item` and the text emission take `&mut impl Canvas2d`.
   There must be exactly one implementation of "RenderPlan item → engine
   draws"; the headless path and the live path both call it.
2. Add a root transform to the emission: today `space::composition_flip`
   maps composition pixels (Y down, origin top-left) to engine world
   (Y up, origin centre) for a target of exactly the composition size. Add
   a `ViewFit { scale: f64, offset: (f64, f64) }` (or an `Affine2`
   pre-multiplied into the flip) so the same plan can be drawn into a
   window region of any size. `FrameRenderer` uses the identity fit, and
   its output must stay byte-identical (the golden test and every existing
   test must pass unchanged).
3. Public live API, behind a cargo feature `live` of `motioner_render`
   (off by default, so `FrameRenderer` users and the boundary rule stay as
   they are): a `LivePainter` that owns the registered font id and the
   missing-glyph cache for one `ferrous_renderer::Renderer`, with
   `fn new(renderer: &mut Renderer) -> Result<Self, RenderError>` and
   `fn paint(&mut self, renderer: &mut Renderer, document, plan, fit: ViewFit) -> PaintStats`.
   This is the only public API of `motioner_render` that names an engine
   type; say so in its rustdoc and in the crate README. Only `motioner_ui`
   enables the feature.
4. Pure helper `ViewFit::contain(comp_w, comp_h, rect: [f64; 4] /* x, y, w, h in window px, top-left */, window_w, window_h) -> ViewFit`
   that letterboxes the composition into a rect of the window (aspect
   preserved, centred). Unit tests: exact fit, wide rect, tall rect, rect
   offset from the window centre, zero-size rect (returns a fit that draws
   nothing, no NaN).
5. Tests: the existing suite unchanged; a new test that renders frame 120 of
   `hello.motion` through `FrameRenderer` before and after the refactor
   (the golden) — no tolerance change; unit tests for the fit maths.

**Phase 4 — `motioner_ui` crate: state, without a window (orchestrator).**
Owner: new `crates/motioner_ui/**`, root `Cargo.toml` members.
Everything in this phase is pure Rust and unit-tested without a GPU.
1. `PreviewSession`: owns the source path, the last good `Document`, the
   `Evaluator`/plan for the current frame, and the last diagnostics. Loading
   uses the same MDF entry points as `motioner_app` (read
   `crates/motioner_app/src/main.rs` and reuse the library calls; if the CLI
   has helper logic you need, move it into the library crate that owns it
   instead of copying it). A failed reload keeps the last good document on
   screen and stores the diagnostics (message, line, column).
2. `Transport` state machine: `Stopped | Playing { anchor_frame, anchor_time } | Scrubbing`,
   current frame as an integer, composition `fps` as a rational and
   duration in frames. `fn frame_at(&self, now: Duration) -> i64` maps
   elapsed wall time to a frame with exact rational arithmetic (no float
   accumulation; integer nanoseconds × fps_num / (fps_den × 1e9)). Operations:
   play, pause, toggle, step ±1, go to start/end, set frame (clamped), loop
   on/off. Changing the document's fps or duration on reload keeps the
   current frame clamped. Unit tests cover every operation, the loop
   boundary, 23.976 (24000/1001) fps for 10 minutes with no drift, and a
   paused transport never changing frame.
3. `FileWatch`: polls the source file's modification time and length every
   250 ms (no new dependency) and reports "changed" once per change, with a
   short debounce so an editor's save-in-two-steps triggers one reload.
   Unit-test it with a temp file in the crate's test directory (or
   `std::env::temp_dir()` plus a unique name), cleaning up after itself.
4. Timeline maths: `frame_to_x` / `x_to_frame` for a scrubber rect
   (clamped, rounding to nearest frame), and the tick spacing for frame /
   second labels at a given width (at least 60 px between labels). Unit
   tests.
5. Scene outline model: a flat list of rows `(depth, NodeId, label, kind)`
   built from the `Document` tree (group → children), deterministic order.
   Unit tests on `hello.motion`.

**Phase 5 — the window (orchestrator).**
Owner: `crates/motioner_ui/**`, `crates/motioner_app/src/main.rs` (new
subcommand only).
1. `motioner_ui::run_preview(path: &Path, options: PreviewOptions) -> Result<(), UiError>`
   implements `FerrousApp` with `AppMode::Flat2D`, window title
   `Motioner — <file name>`, initial size 1600×900, dark theme.
2. Layout (engine widgets; read `ferrous_editor` and `ferrous_ui_core`
   widget code first):
   ```
   +-----------------------------------------------------------+
   | top bar: file name | WxH @ fps | frame / total | timecode   |
   +------------+----------------------------------------------+
   | Scene      |                                              |
   | outline    |            viewport (composition,            |
   | (tree or   |            letterboxed, checker or dark      |
   | list)      |            pasteboard around it)             |
   |            |                                              |
   +------------+----------------------------------------------+
   | transport: |<  <  [play/pause]  >  >|  loop  | timeline   |
   | scrubber with frame/second ticks and a playhead            |
   +-----------------------------------------------------------+
   | status bar: diagnostics (red, with line:col) or "ok", fps  |
   +-----------------------------------------------------------+
   ```
   Use a split pane (or fixed panels if split panes are not usable in this
   engine version; report which), `ViewportWidget` (or the measured central
   rect) to know where the composition goes, buttons for the transport, a
   slider or a custom drawn scrubber for the timeline, and a tree view or
   list for the outline. Clicking an outline row selects the node and draws
   a 1 px selection outline around its evaluated bounds in the viewport if
   the plan exposes bounds; if it does not, show the selection only in the
   outline and report what would be needed.
3. Viewport drawing: every frame, compute `ViewFit::contain` for the
   viewport rect, draw a pasteboard background and the composition
   background rect, then `LivePainter::paint` the plan of the current
   frame. Re-evaluate the plan only when the frame index or the document
   changes (cache the last `(document generation, frame)`); a paused window
   must not re-evaluate every frame. Items that overflow the composition are
   visible over the pasteboard (like After Effects); UI panels are drawn on
   top. Document why 2D views are not used (Part A bug, views clip shapes
   only).
4. Input: Space play/pause, Left/Right step one frame (Shift: 10 frames),
   Home/End start/end, L loop, R reload now, Esc quit. Mouse: click or drag
   on the timeline scrubs (transport enters `Scrubbing` while dragging),
   wheel over the timeline steps frames. Show these shortcuts in a tooltip
   or the status bar.
5. Hot reload: the `FileWatch` is polled in `update`; on change the session
   reloads; success clears diagnostics and keeps the current frame (clamped);
   failure shows the diagnostic in the status bar and keeps the last good
   frame.
6. CLI: `motioner preview <file.motion> [--frame N] [--paused] [--exit-after-frames N]`
   in `motioner_app` (update `HELP` and the argument parser tests).
   `--exit-after-frames` closes the window after N presented frames (for
   smoke runs and future CI); it must not change any other behaviour.
7. Errors: `UiError` (window/device creation, load failure at startup with
   the MDF diagnostic) with `Display` + `Error`; `motioner_app` maps it to
   exit code 1 (2 for MDF diagnostics, like the other commands).

**Phase 6 — polish pass on the window (orchestrator).**
1. Timecode `HH:MM:SS:FF` and frame counter in the top bar, using the
   rational fps (drop-frame timecode is out of scope; say so).
2. Playback that cannot keep up must drop frames, not slow down: the frame
   shown is always `frame_at(now)`. Show the measured UI fps and the number
   of dropped frames in the status bar.
3. Window resize and DPI: the viewport fit, panels and text stay correct
   when the window is resized and on a 125 %/150 % scale factor (use the
   engine's pixel-ratio API; report what you tested).
4. Empty states: file not found at startup → error message and exit 1;
   composition with zero items → the background only; reload to an MDF
   error before any good load → the window opens with the diagnostic
   shown.

**Phase 7 — tests for the window layer (orchestrator).**
1. Everything testable without a window is tested (phase 4 plus the input
   mapping: a pure `fn action_for(key, modifiers) -> Option<Action>` and
   `fn apply(action, &mut Transport, ..)`, unit-tested).
2. A headless integration test in `motioner_ui` (GPU, skips cleanly without
   an adapter, using the same skip pattern as `motioner_render`'s tests)
   that paints `hello.motion` frame 120 through `LivePainter` into an
   offscreen 1920×1080 `Pure2dHarness` renderer with the identity fit, reads
   it back and compares it to the golden `hello_f120.png` with the same
   tolerance the golden test uses. This proves live and export draw the same
   pixels.
3. A second case with `ViewFit::contain` into a 960×540 rect: the result
   equals the golden downscaled by 2 within a documented tolerance (or
   compare a few sampled pixels of large flat areas; explain the choice).

**Phase 8 — docs and review (orchestrator).**
1. `crates/motioner_ui/README.md`: what the window does, shortcuts, the
   wall-clock exception to determinism, architecture (session, transport,
   watcher, painter), known limitations (no editing yet, no audio, no 2D
   views, no GUIMaker).
2. Update the root README "Usage" section with `motioner preview`.
3. Strict review of the whole diff in both repositories: duplication
   (especially loading logic shared with the CLI, and emission shared
   between headless and live), rustdoc on all public items, typed errors,
   panics on runtime paths, allocations per frame in the paint path (reuse
   buffers), tests that would pass without the code. Fix findings.

**Phase 9 — run it (orchestrator).**
1. `cargo build -j 2 --release -p motioner_app` (the only release build).
2. Run `motioner preview examples/hello.motion --exit-after-frames 600`
   and confirm it opens, plays and closes cleanly with exit code 0.
3. Run it again without the flag for a manual check: play, pause, step,
   scrub, loop, resize, edit `examples/hello.motion` (change a colour, save,
   see it update; introduce a syntax error, save, see the diagnostic; undo
   it with `git diff` showing no change left in the example at the end).
   Take a screenshot if you can (Windows: `nircmd`/PowerShell
   `System.Windows.Forms` screen capture into `out/preview.png`; `out/` is
   ignored by git) and report its path.
4. Report CPU/GPU frame time while playing at 1600×900, and memory use.

---

## 3. Acceptance criteria

- `motioner preview examples/hello.motion` opens a window with top bar,
  scene outline, letterboxed live viewport, transport, timeline scrubber
  and status bar; plays at the composition fps (dropping frames, not
  slowing); scrubs; reloads on save; shows MDF diagnostics without
  crashing.
- One emission path: the headless `FrameRenderer` and the live
  `LivePainter` share all "plan → engine draws" code; the golden is
  unchanged and the live painter reproduces it.
- Transport, file watch, timeline maths, outline and input mapping are pure
  and unit-tested; rational time has no drift.
- ADR D-0020 written; boundaries script updated and passing;
  `motioner_render` still builds and tests alone without engine GUI
  features.
- Part A: multi-view hard edges correct with a failing-before test; ring
  state simplified; no runtime `expect` in the layer path.
- Zero warnings, rustdoc on all new public items, no new dependencies
  (except engine path deps inside the workspace), no new `unsafe`.

## 4. Gates

**4A — Motioner** (end of phases 2, 3, 4, 5, 7, 8; `CARGO_INCREMENTAL=0`, `-j 2`):
- `cargo fmt -- --check`
- `cargo clippy -j 2 --workspace --all-targets -- -D warnings`
- `cargo test -j 2 --workspace`
- `cargo test -j 2 -p motioner_render` (alone, without `live`)
- `RUSTDOCFLAGS="-D warnings" cargo doc -j 2 --workspace --no-deps`
- `python scripts/check_boundaries.py`
- `cargo machete crates`
- free space on C:

**4B — FerrousEngine** (end of phase 1 and again in phase 8; `-j 2`):
- `rustfmt --edition 2021 --config skip_children=true --check <files you edited>`
- `cargo clippy -j 2 -p ferrous_font -p ferrous_2d -p ferrous_renderer --all-targets --no-deps -- -D warnings`
- `cargo clippy -j 2 -p ferrous_renderer --no-default-features --all-targets --no-deps -- -D warnings`
- `cargo test -j 2 -p ferrous_font -p ferrous_2d -p ferrous_renderer`
- `cargo test -j 2 -p ferrous_renderer --no-default-features --tests`
- `cargo check -j 2 --workspace --all-targets`
- `cargo check -j 2 -p ferrous_web --target wasm32-unknown-unknown`

## 5. Stop conditions

Stop and report instead of improvising if: a gate fails twice after a fix
attempt; the engine's app runner or UI widgets cannot do something the
layout needs without changing the engine (report the exact gap with
file:line and propose the engine change — do not edit the submodule);
`motioner_render` cannot keep its golden byte-identical after the emission
refactor; the window cannot open on this machine; free space on C: drops
below 8 GB.

## 6. Final report

1. Phase summaries (which subagent did what).
2. Files created/changed per repository, one line each.
3. Public API additions and changes (exact signatures) in `motioner_render`,
   `motioner_ui`, `motioner_app` and the engine clone; breaking changes.
4. Engine UI features actually used, and every gap you hit (with file:line).
5. Tests added per crate, which skip without a GPU, and why each fails
   without its change (state explicitly that the multi-view test failed
   before the fix).
6. Gate tables (4A and 4B) for the final run.
7. Manual run (phase 9): what worked, what did not, screenshot path,
   frame times and memory.
8. Free space on C: at the end and open questions for the reviewer
   (especially for the GUIMaker plan, T-ARCH-0007).
