# opencode prompt 15 — GUIMaker: a UI library that is beautiful by default and easier than HTML + CSS

You are the orchestrator of a very long, multi-phase job in two
repositories:

- **FerrousEngine dev clone** at `C:\Users\jvchi\CARPETAS\FerrousEngine`.
  **GUIMaker lives here**, as a group of crates in the engine workspace
  (`crates/guimaker*`). It becomes **the UI library of the FerrousEngine
  ecosystem**, which is what D-0004 always meant by "GUIMaker".
  - The old separate repository (`github.com/jvchiappini/GUIMaker`, last
    commit `9dcbc28`, 2026-03-11, a small designer binary that no longer
    builds) is retired, and its local folder was deleted on 2026-11-21. To
    read the old designer for Phase 0 and Phase 11, clone it read-only into
    `%TEMP%\guimaker-old` and delete it after.
  - Engine render primitives that the library needs and the engine lacks
    are added to the engine crates as **generic** features (no GUIMaker or
    Motioner names in them), with rustdoc, crate docs in
    `crates/<crate>/docs` plus the `mkdocs.yml` nav, tests and ideally an
    example. The reviewer pushes and bumps pins.
- **Motioner** at `C:\Users\jvchi\CARPETAS\Motioner` (branch named by the
  reviewer): only in Phase 10, for the plumbing. The frontend itself is a
  separate Claude session (see Phase 10).

**Dependency direction inside the engine workspace.**
- The GUIMaker crates may depend on the engine crates: the GPU-free ones
  from the core, and the drawing ones only from `guimaker_ferrous`.
- No engine crate may depend on a GUIMaker crate.
- Add this rule to the engine's boundary check, so the gate fails when it
  breaks.
- A project that uses Ferrous without GUIMaker compiles none of it.

### Status and refresh (reviewer, 2026-11-21) — read this first

This prompt has **not been run**. The old GUIMaker repository stopped at
`9dcbc28` (2026-03-11) and is retired. **Repository decision
(2026-11-21):** GUIMaker is built inside the FerrousEngine workspace, so
there is one repository, one `target` dir and one gate for the engine and
its UI library.
Prompts 16, 17 and 18 ran in the meantime, and the rest of this prompt must
be read with these changes in mind:

- **Order.** Run this prompt after prompt 18 P7 is merged into `main`, and
  **before** prompt 19 (debug tools) and prompt 14 (agent panel). Prompt 19's
  devtools panels and the future Unreal-style editor are UI. Per the
  ownership split below they are built on GUIMaker, so GUIMaker must exist
  first. Prompt 11 is committed, so Phase 10 is no longer blocked.
- **Already done elsewhere: reuse it, do not redo it.**
  - **Phase 0B (startup freeze):** prompt 17 fixed the "Not responding"
    startup, and the first composed frame lands at about 0.5 s. Phase 0B
    shrinks to a re-check with the prompt 17 startup trace.
  - **Render flags, redraw scheduler and damage:** prompt 18 P1 and P5 did
    them (on-demand redraw, `redraw_at`, damage rects, `PersistentUiTarget`).
    GUIMaker emits damage through those APIs.
  - **Spatial index, incremental layout and UI widgets:** prompt 18 P4 put
    `spatial_index.rs`, incremental layout and retained batch ranges in the
    engine (`ferrous_ui_core`). Under the user's rule below (the engine is
    draw calls, GUIMaker has the controllers), Phase 0 moves the controller
    side to GUIMaker:
    - the spatial index, incremental layout and the `scroll_view`,
      `virtual_list` and `tree_view` logic move to GUIMaker, keeping their
      measured performance (prove it with the prompt 18 benches before and
      after);
    - the draw side (`GuiBatch`, retained batch ranges) stays in the
      engine;
    - one implementation is left;
    - Motioner's panels move onto GUIMaker, which replaces the
      `UiSystem`-versus-retained-ranges question from D-0063.
  - **Zero allocations per frame, byte budgets and the perf harness:**
    prompt 18 (D-0063). The resource budgets below are measured with
    `scripts/perf_bench.py` and the `alloc-stats` feature, not with a new
    harness.
- **Not done anywhere yet:**
  - gesture recognition in `ferrous_events`, which today is a single
    `lib.rs`;
  - canvas and scene picking in the engine (Phase 4B);
  - the whole library (tokens, view tree, components, text editing, lint);
  - the Motioner migration.
- **Stale numbers.**
  - `start.rs` is now 973 lines with 28 `Rect::new` calls.
  - The idle and memory figures must be re-measured against the prompt 18
    `p7-after.json`, not the older numbers in this file.
- **Execution.**
  - Use the prompt 18 execution model: lanes with disjoint files,
    edit-only subagents, one gate in a clean worktree by the main agent,
    and three-run medians.
  - Use the disk floor: check C: before every build and stop below 12 GB.
  - GUIMaker is its own lane in every phase.
- **Debug hooks.** Every GUIMaker view must be inspectable by the prompt 19
  widget reflector and frame debugger:
  - stable node ids and paths;
  - layout rects and resolved tokens;
  - the cache state and repaint reason;
  - the draw commands it produced, with attribution.

  Design those data sources in Phase 2 so that prompt 19 only adds the
  panels.

### Visual contract

`.control/design/ui-target-v1.md` is the visual target for this prompt: tokens
(phase 1), components (phase 6), motion (phase 7), the screenshot acceptance
checks (phase 5) and the Motioner screens (phase 10). Read it before phase 1.
Deviations need a one-line reason in the phase report.

### Why this prompt exists

Motioner's start screen (`crates/motioner_ui/src/start.rs`, 934 lines) was
written by a capable model and still looks bad. The cause is the tool, not the
model. Read `start.rs::draw_home` before anything else: every element is placed
with hand-computed rectangles (`Rect::new(layout.new_project.x, header_y +
p.m.px(22.0), layout.window.w, p.m.px(20.0))`), colours and radii are picked
per call (`p.m.px(6.0)`, `accent[3] = 0.06`), hover states are hand-written
`if/else`, and a separate `StartLayout` duplicates the geometry for hit
testing. With that API, every screen is a pile of magic numbers, nothing
aligns by construction, and good taste depends on the author doing arithmetic
correctly hundreds of times. D-0021 explains how the editor ended up there
(engine gaps in 2026-10); the engine has since fixed borders, shadows,
baselines, lines and texture updates (`e482b3c`).

The engine's own stack (`ferrous_ui_core` 11.7 k lines, `ferrous_layout` on
taffy, `ferrous_ui_render`) has flex layout but no visual styling in `Style`
(only margin/padding/size/alignment/gap), colours hard-coded inside widgets, a
JSON loader with absolute `x/y/w/h`, and widgets as `Box<dyn Widget<App>>`
mutating app state in callbacks. It is not a foundation a model can design
well on either.

### The split of responsibilities (decided by the user, non-negotiable)

- **FerrousEngine draws everything**: shapes, borders, shadows, gradients,
  clips, blur, glyph rasterisation, images, icons, the GPU pass and the
  offscreen readback for screenshots. Any drawing capability that is missing
  is added to the engine as a generic feature.
- **GUIMaker decides everything else**: structure, positions, sizes,
  responsive rules, formatting (tokens, themes, typography choices),
  interaction (pointer, keyboard, focus, text editing) and the diagnostics
  that explain the layout. GUIMaker contains **no shader, no rasteriser and no
  GPU code**; its output is a list of draw commands that a thin adapter hands
  to the engine.

**UI is not the canvas.** Clicking a button and clicking a shape in a scene
are different problems and live in different places:

| Concern | Owner | Why |
|---|---|---|
| Window, raw input (pointer, keys, wheel, IME, clipboard, file drop), DPI | FerrousEngine (`ferrous_events`, runner) | platform layer, shared by everything |
| **Gesture recognition**: click, double/triple click, long press, drag threshold, pointer coalescing, key repeat | GUIMaker (`guimaker::input`) | a UI controller; the `canvas` element hands the same recognised gestures to the app, so it is still implemented once |
| Render flags, redraw scheduler, damage regions (Phase 0B) | FerrousEngine | the engine owns frames |
| **Hit testing and its spatial index** (grid/BVH over the laid-out rects, incremental updates), hover detection, scroll controllers (wheel, trackpad, inertia, scrollbars, auto-scroll), virtual lists | GUIMaker | UI controllers; engine scene picking may reuse the index as a GPU-free GUIMaker crate only if that avoids a second implementation |
| **Scene / canvas picking**: objects with transforms and rotation, path fill containment, stroke distance, text bounds, clips and masks, z-order, ID-buffer picking, marquee and lasso queries, snapping queries, screen ↔ world transforms, pan/zoom-to-cursor camera helpers, 3D raycasts | FerrousEngine (2D scene / renderer crates) | it knows the geometry it draws |
| UI tree: layout, responsive rules, tokens, components | GUIMaker | |
| **UI routing**: which widget gets an event, hover/press state, focus and `Tab` order, shortcuts scopes, overlays and focus traps | GUIMaker | screen-space rects of its own tree |
| Text-editing engine (`Ctrl+A`, selection, undo…) | GUIMaker | a UI concern on top of engine key events and clipboard |
| `canvas` / `viewport` element | GUIMaker reserves the rect, routes events inside it, in local coordinates, to the app | the bridge between both worlds |
| What a pick *means* (select an MDF node, drag a gizmo handle, edit a keyframe) | the app (Motioner) | domain logic |

The GUIMaker core stays free of drawing crates, but it **may depend on the
engine's GPU-free crates** (`ferrous_events` for raw input, math) so nothing
is implemented twice.

**The user's rule (2026-11-21):** GUIMaker exists so that building an
interface is fast and efficient. **FerrousEngine is draw calls**: shapes,
text, images, passes, plus the window and raw input. **GUIMaker has the
controllers**: scroll, hit testing ("is the pointer on this element"),
hover, press, drag, gestures, focus, text editing and layout. The UI widget
and controller code that grew inside the engine moves to GUIMaker in Phase 0,
with one implementation left:
- `ferrous_ui_core` widgets such as `scroll_view`, `virtual_list` and
  `tree_view`;
- `spatial_index.rs`;
- incremental layout.

The engine keeps the draw-command side (`GuiBatch`, retained batch ranges,
`ferrous_ui_render`).

### The goal

A person (or a model) describes **structure and intent**, never coordinates,
and gets a screen that looks like Linear, Raycast, Figma or Vercel's Geist
without trying, that adapts to any window size, and whose text fields and
controls behave like native ones. Concretely:

1. **Beautiful by default.** A complete design system ships in the library:
   spacing, type, radius, elevation, colour roles, motion, density. Default
   values of every component are already the good ones.
2. **Wrong is hard.** Styling takes tokens (`pad(S4)`, `radius(Md)`,
   `elevation(2)`, `text(Body)`, `Tone::Muted`), not pixels or RGBA. A raw-pixel
   escape hatch exists, is explicit (`Px(13.0)`), and the design lint reports
   it.
3. **Layout is structural.** Stacks, rows, grids, centring, spacers,
   scroll areas, overlays. Content sizes itself (text is measured). No
   parallel "layout struct" for hit testing: the same tree draws and receives
   input.
4. **100 % responsive.** Every view works from a narrow 360 px panel to a 4K
   window and at any DPI scale, with no code per size beyond declarative
   breakpoint rules. Nothing overflows, overlaps or clips unless asked to.
5. **Interaction is free.** Hover, pressed, focus ring, disabled, selected and
   their animated transitions come from the component, not from the caller.
   Click, double/triple click, right click, drag, scroll, `Tab` focus,
   shortcuts and a complete text-editing engine (`Ctrl+A`, word jumps,
   selection, clipboard, undo) are library utilities any widget reuses.
6. **The library explains itself.** When a layout is wrong, GUIMaker says in
   plain words what overlaps what, which rule caused it and how to fix it, plus
   a text dump of the laid-out tree and a headless screenshot of any view at
   any size, scale and theme. A model iterates on UI the way it iterates on
   code: write, look, read the explanation, fix.
7. **Less code than HTML + CSS** for the same screen (measured in Phase 9).
8. **The lightest UI stack there is.** The user's goal is that the engine and
   GUIMaker are each perfect at their own job and that a UI built on them
   uses **less memory, CPU and GPU than any other UI library** (egui, iced,
   Slint, Dear ImGui, Qt, Electron/WebView2). Measured, not claimed (Phase
   9B). An idle window costs 0 % CPU and 0 % GPU.

### Resource budgets (every phase measures against them; release build, the
start-screen recipe at 1920×1080, scale 1.5, on this 8 GB machine)

| What | Budget |
|---|---|
| Idle (no input, no animation) | 0 frames rendered, ~0 % CPU, ~0 % GPU, no timers waking the thread more than once per caret blink |
| Idle with a focused text field | only the caret's rect repainted at the blink interval; ≤ 2 wake-ups/s |
| Pointer move | hit test < 5 µs with 10 000 interactive nodes; no work if the hovered element does not change |
| Hover / typing | redraw only the damaged region; CPU per frame (build + layout + routing + command list) < 0.3 ms; GPU per frame < 0.5 ms |
| Steady state | **zero heap allocations per frame** in the GUIMaker core (counted by a test allocator); buffers and arenas reused |
| Layout | unchanged subtrees are not re-laid out (memoised by a hash of style + content + available size); a 10 000-row virtual list scrolls with < 1 ms CPU per frame |
| Memory | GUIMaker core < 1 MB heap for the start screen; whole process RSS for a minimal GUIMaker app < 60 MB, GPU memory < 64 MB (glyph atlas sized to the glyphs actually used, no 3D resources for 2D apps) |
| Startup | first visible frame < 300 ms warm, never "Not responding" (Phase 0B) |
| Binary | release size of the minimal example reported; no dependency added without its size in the decision |

Phase 0B (the startup freeze) is engine-only and independent of the rest;
it may run first while prompt 11 is still in progress, as long as it does
not touch the files prompt 11 owns in the engine clone.

Quality bar: top-tier studio. Zero duplication, rustdoc on every public item
(English), typed errors, no panics on runtime paths, regression tests that
fail without the change, no warnings. **Stop after each phase** with the gate
green and a short report; do not commit. The user compacts the conversation
and sends "continue with Phase N".

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the two repositories above. Never edit
  `Motioner\engine\ferrous`. Do not commit, stage, stash, reset, checkout,
  rebase or push in any repository. The reviewer commits.
- `CARGO_INCREMENTAL=0`, `-j 2` (8 GB RAM). Never run two cargo builds at the
  same time. Stop if free space on C: drops below 5 GB (you may clean caches
  outside the repositories).
- **The GUIMaker crates** (new, so held to the strictest bar):
  - `cargo fmt -p guimaker -p guimaker_ferrous …` with `--check`;
  - `cargo clippy -p guimaker* --all-targets -- -W clippy::pedantic -D
    warnings`, and every `#[allow]` has a reason;
  - their tests;
  - `cargo machete`;
  - the dependency-direction check.
- **Motioner:** its usual gate.
- **The other engine crates:** `rustfmt --edition 2021 --config
  skip_children=true` only on files you edit, keep CRLF files' endings
  (`git ls-files --eol`), and run the tests of the crates you touch.
- **Decisions.** GUIMaker gets its own decision log:
  `FerrousEngine/crates/guimaker/docs/decisions/G-NNNN.md` (context, decision, consequences,
  alternatives rejected), same tone as Motioner's ADRs. Motioner decisions go
  through `pctl decision-new`. Every new dependency needs a decision first
  (what, why writing it is worse, licence, size, transitive deps). Expected:
  `taffy` (same version the engine already uses), a font family if needed.
  No egui, no iced, no web view: this is our own library.
- **Generic.** The library knows nothing about Motioner, MDF, timelines or
  projects. Motioner-specific widgets (timeline, graph editor, canvas gizmos)
  stay in `motioner_ui`.
- **Determinism.** Layout and paint depend only on the view, the theme, the
  viewport size, the scale factor and an injected clock (`Instant` is never
  read inside the library). Same inputs → byte-identical display list and
  screenshot. Animations advance from the clock the host passes in.
- Shell gotchas: Bash heredocs containing `'''` break here (write files with
  the editor tool); PowerShell 5.1 drops unescaped `"` in native arguments;
  `core.autocrlf=true`.
- **Look at every screenshot you produce** and fix the worst visible problem
  first. A green test with an ugly picture is not done.
- **Resources are a feature.** Every phase ends with its numbers against the
  budget table (frame CPU, allocations per frame, memory, GPU time, idle
  wake-ups) in the phase report. A regression of more than 10 % against the
  previous phase's numbers blocks the phase like a failing test. Prefer
  flat arrays, indices and arenas over `Box<dyn>` trees, `Rc` and per-node
  `String`s (interned or borrowed text); no `clone` of the view per frame.

GUIMaker gate: `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`cargo machete`.
Motioner gate (phases 10–11): the full gate of prompt 11 (fmt, clippy, test,
doc, `scripts/check_boundaries.py`, machete, `pctl validate`).

Stop conditions: C: below 5 GB; a gate you cannot make green after two honest
attempts; an accepted decision a phase would contradict; an engine feature
whose pin is not bumped yet (finish behind a typed `Unsupported` path or a
documented fallback and continue).

---

## Phase 0 — Audit and the founding decision (no library code yet)

1. Read, and summarise in the session notes with file:line references:
   Motioner `start.rs`, `theme.rs`, `paint.rs`, `text.rs`, `icons.rs`,
   `widgets.rs`, `layout.rs`; D-0004, D-0018, D-0020, D-0021;
   engine `ferrous_ui_core` (`primitives.rs`, `theme.rs`, `style_builder.rs`,
   `ui_tree.rs`, `widgets/`), `ferrous_layout`, `ferrous_ui_render`
   (`gui_batch.rs`, the WGSL it feeds), `ferrous_font`; the old GUIMaker `src/` and
   `docs/` from the read-only clone. List what each one does well (Motioner's tokens, WCAG tests, text
   baseline/kerning/ellipsis, vector icons are good seeds) and what blocks good
   design.
2. Take screenshots of today's Motioner start screen and New project dialog
   (`scripts/ui_shot.ps1 -Out out/ui/p15-00-before-*.png`). They are the
   "before" of the final report.
3. Write **G-0001 "GUIMaker library architecture"** deciding:
   - **Crate layout** inside the FerrousEngine workspace:
     - `crates/guimaker`: the core. It has no engine drawing crates; the
       GPU-free ones are allowed. It holds the view tree, style, theme,
       layout, input routing and controllers, state, display list, lint
       and inspect.
     - `crates/guimaker_ferrous`: a thin adapter. Draw commands go to the
       engine API, input events come in, and text metrics come from
       `ferrous_font`. It has no drawing logic of its own: if the adapter
       needs more than a mapping, the capability belongs in the engine.
     - `crates/guimaker_gallery`: the storybook binary.
     - `crates/guimaker_designer`: the old designer, revived in Phase 11.

     Docs go in `crates/guimaker/docs` with their own section in the
     engine's `mkdocs.yml`. No dead code from the old repository is copied.
   - **Programming model.** Recommended, justify or replace: a declarative
     tree rebuilt every frame from the app's state (cheap, no stale widgets,
     ideal for a model to write), laid out, then input is routed through
     *this frame's* tree (no one-frame lag), then painted. **Rebuilding is
     event-driven, not continuous**: the view is rebuilt only when input
     arrives, app state signals a change, or an animation is running; layout
     and draw commands of unchanged subtrees are reused (memoised by hash);
     the engine is asked to repaint only damaged regions. The tree lives in a
     reused arena (flat `Vec` + indices), so a rebuild allocates nothing in
     steady state. The decision must show this model meets the budget table,
     or pick a retained model that does. Persistent state
     (hover/press animation, focus, scroll offsets, text-edit cursors,
     open popovers) lives in a store keyed by a stable id path
     (explicit `.key(...)` for lists). Interaction returns **typed messages**
     (`Ui<Msg>` → `Vec<Msg>`), Elm style, so apps keep their own command
     layer (Motioner's `StartAction` / `Command` map 1:1). No `Rc<RefCell>`,
     no callbacks that capture app state.
   - **Engine dependency:** GUIMaker uses workspace path dependencies.
     Motioner gets GUIMaker through its existing `engine/ferrous`
     submodule, so it builds one engine copy by construction, with no
     `[patch]` and no second submodule.
   - **Relation to the engine's `ferrous_ui_core`**: GUIMaker does not wrap
     its `Widget` trait. Decide which engine parts it reuses (taffy version,
     `GuiBatch`, fonts, SVG icons) and record which engine UI pieces become
     candidates for deprecation (a backlog note for the engine, not done here).
   - Text measurement as a trait in the core (`TextSystem`) implemented by the
     backend, so the core is testable headless with a deterministic fake
     *metrics table built from a real font file*, not a mock of rendering.

## Phase 0B — Engine: the "Not responding" freeze at startup

The user reports that Motioner and other FerrousEngine apps often sit in
Windows' "Not responding" state for a while on launch and then run perfectly.
Windows ghosts a window whose thread does not pump messages for ~5 s. The
likely cause is visible in `FerrousEngine/crates/ferrous_engine/src/runner/
events.rs:72` (`resumed`): the window is created **visible**, then on the
same thread, without pumping messages, the runner does `GraphicsState::new
(...).block_on()` (adapter, device, surface, every pipeline and shader
module, HDRI if configured), `Font::load_bytes` → `FontAtlas::new`, which
runs `generate_msdf` on the CPU for the whole `latin_western()` set
(`ferrous_font/src/atlas.rs:134`), and then the app's `setup` (Motioner
loads fonts, the project, thumbnails…). Prove it before fixing it:

1. **Measure**: add `tracing` spans (or the engine's existing timing) around
   each startup step and log a startup timeline: process start, window
   created, adapter, device, surface configure, each pipeline group, font
   atlas (glyph count, ms), app `setup`, first frame presented. Run cold
   (after reboot or with the shader/driver cache cleared if possible) and
   warm, debug and release, for Motioner and one engine example. Record the
   numbers in the session notes. Check also the driver shader cache, a
   discrete-GPU selection (`PowerPreference`) that wakes a sleeping dGPU, and
   any `std::fs` scan or font enumeration at start.
2. **Fix in the engine, generically**:
   - create the window **hidden** (`with_visible(false)`) and show it on the
     first presented frame, or show it immediately with a cheap clear colour
     and keep pumping messages; never block the message thread for more than
     ~100 ms;
   - move heavy CPU work off the main thread: MSDF atlas generation in
     parallel (per-glyph work is independent; `rayon` only with a decision)
     or lazily — rasterise ASCII first and the rest of the set on demand or
     in a background thread, uploading as glyphs arrive; cache generated
     atlases on disk keyed by font hash + glyph set + size + generator
     version;
   - create pipelines lazily or in parallel, use a wgpu `PipelineCache`
     where the backend supports it, and skip 3D/HDRI pipelines for
     `Flat2D`/`Pure2D` apps;
   - give apps an async-friendly `setup` path (a loading state the engine
     keeps drawing while app work finishes on a worker), documented.
3. **Motioner side** (after prompt 11 is committed): move its own slow
   startup work (project load, thumbnails, recent list, font loading) behind
   the loading path so the start screen appears at once and fills in.
4. **Idle cost** (same phase, also engine): measure CPU %, GPU %, wake-ups per
   second and RSS of an idle Motioner window and an idle engine example. The
   runner must use `ControlFlow::Wait` when nothing is animating and the app
   has not requested a redraw (a generic "request redraw / request animation
   frame until T" API for apps), present only when something changed, prefer
   the low-power adapter for 2D/UI apps (configurable), and not allocate 3D,
   HDRI or post-processing resources in `Flat2D`/`Pure2D` modes.
5. **Render flags and the redraw scheduler** (generic engine API; GUIMaker
   and apps drive it). A still window sometimes must update a few things —
   a blinking caret, a spinner, a progress bar, a playing video or preview,
   a clock, a thumbnail that finished loading on a worker, a file watcher.
   Instead of "render every frame" or "never", every source declares what it
   needs:
   - per-element / per-region **render flags**: `Static` (default),
     `Animating { until }`, `Periodic { interval }` (caret blink 530 ms,
     clock 1 s), `Continuous { max_fps }` (video playback, live preview,
     capped and paused when the window is hidden or minimised), and
     `Dirty` (one repaint after a change);
   - a thread-safe **redraw handle** (`RedrawProxy`, cloneable, `Send`) that
     workers use to mark a region dirty and wake the loop — no polling;
   - the scheduler computes the next deadline as the minimum over all flags
     and sleeps with `ControlFlow::WaitUntil(deadline)`; at wake-up it
     repaints **only the union of regions whose flags fired** (a caret blink
     repaints a few pixels, not the window); frames are aligned to vsync
     when continuous;
   - occlusion: minimised or fully covered windows drop to zero frames;
     unfocused windows may lower `max_fps` (configurable);
   - every frame records **why** it was rendered (which flag, which element,
     which region); the debug overlay and the frame counters show it, so
     an unexpected redraw is a one-glance diagnosis. A test fails if an idle
     view renders any frame without a flag that explains it.
   GUIMaker components set their flags themselves (text field → `Periodic`
   while focused, spinner → `Continuous { 60 }` only while visible, hover
   transitions → `Animating { until }`); app code only sets flags for its
   own live content (Motioner: the playing preview).
6. **Acceptance**: time from launch to first visible frame and to "fully
   interactive", cold and warm, before and after, plus idle numbers, in the report; the window
   never shows "Not responding"; an engine test or example that fails if the
   message thread blocks longer than a budget during startup (measured with
   the startup timeline). Engine docs: a "Startup performance" page.

## Phase 1 — Design system: tokens and themes

`guimaker::theme`, the only place with literal design values.

- **Spacing** scale on a 4 px grid (`S0`=0, `S1`=4, `S2`=8, `S3`=12, `S4`=16,
  `S5`=20, `S6`=24, `S8`=32, `S10`=40, `S12`=48, `S16`=64, `S24`=96).
- **Type roles**, not sizes: `Display`, `Title`, `Heading`, `Subheading`,
  `Body`, `BodyStrong`, `Label`, `Caption`, `Overline`, `Mono`, `Code`, each
  with size, weight, line height, tracking. Two **densities**: `Compact`
  (editors: 11/12/13 px body range, like Motioner's D-0021 scale) and
  `Comfortable` (launchers, settings, marketing-like screens). Density is a
  theme field, so the same view works in both.
- **Real weights.** If the shipped UI font has one weight (D-0021 fakes medium
  with two passes), decide in G-0002 to ship a real family (Inter or Geist,
  OFL) with at least Regular/Medium/SemiBold and a mono (JetBrains Mono or
  Geist Mono, OFL). Delete the faux-bold trick where the real weight exists.
- **Radius** `None/Xs/Sm/Md/Lg/Xl/Full`; **elevation** 0–4, each = a pair of
  soft shadows (key + ambient) plus a surface tint in dark themes; **borders**
  hairline/regular; **motion** durations (`Instant`, `Fast` 90 ms, `Base`
  140 ms, `Slow` 240 ms) and easings (standard, emphasized, spring-ish
  overshoot for popovers).
- **Colour roles**: `bg`, `surface`, `surface_raised`, `surface_sunken`,
  `overlay`, `border`, `border_strong`, `text`, `text_muted`, `text_faint`,
  `text_on_accent`, `accent` (+ hover/pressed/subtle), `focus_ring`,
  `selection`, `success`, `warning`, `danger`, `info` (+ subtle variants).
  Generated from **seeds** (one neutral hue + chroma, one accent) through an
  OKLCH ramp written in the library (small, tested math, no dependency), for
  light and dark. An app picks an accent and gets a coherent palette; it can
  still override any role.
- **Guarantees as tests**: WCAG contrast for every text role on every surface
  it may sit on (body ≥ 7:1, muted ≥ 4.5:1, faint ≥ 3:1 for large/secondary
  only), focus ring ≥ 3:1 against neighbours, sRGB↔linear conversion done once
  (the D-0021 double-encoding bug must be impossible: one `Color` type,
  constructors say which space).
- Themes as data: `Theme` is serde-serialisable (RON or TOML; decide) so apps
  ship theme files and a designer can edit them. Built-ins: `dark`, `light`,
  and a high-contrast variant.

Tests: ramps are monotonic in lightness, contrast table, round-trip of theme
files, density switch changes metrics only.

## Phase 2 — View tree, styling API and layout

The heart of the library. Write the **public API sample first**
(`crates/guimaker/examples/README` or rustdoc on the crate root), get it to
read well, then implement.

- **Containers**: `column`, `row`, `stack` (z-overlay), `grid` (fixed columns
  or `auto_fill(min)`), `center`, `spacer`, `divider`, `scroll`
  (vertical/horizontal/both), `wrap` (flow), `split` (resizable two-pane),
  `aspect(ratio)`, `max_width(Width::Sm/Md/Lg/Xl/Prose)` content columns.
- **Modifiers** (chainable on every element, token-typed): `pad`, `pad_x`,
  `pad_y`, `gap`, `align`, `justify`, `grow`, `shrink`, `width`/`height`
  (`Fill`, `Hug`, `Fraction`, token sizes, `Px` escape hatch), `min/max`,
  `bg(Role)`, `border(Role)`, `radius`, `elevation`, `opacity`, `clip`,
  `tone`, `text(Role)`, `key`, `tooltip`, `disabled`, `hidden`.
- **Text**: measured, wrapping, `lines(n)` clamp with ellipsis, middle
  ellipsis for paths, baseline alignment across a row (`align(Baseline)`),
  rich spans (strong, muted, mono, accent, link), tabular numbers.
- **Layout** on taffy (version shared with the engine) or a hand-written
  flex/grid if G-0001 decided so; either way: pixel snapping to the physical
  grid at the end, so 1 px borders never blur; DPI scale from the host.
- **Input routing** through the laid-out tree (the full interaction toolkit
  is Phase 4; here only the routing): hit testing respects clip and z-order,
  pointer capture, the keyed state store.
- **Display list**: the core's output is a flat, engine-agnostic list of draw
  commands for FerrousEngine to draw
  (rounded rect with per-corner radii, border, shadow, linear/radial
  gradient, image with `fit` (`Cover/Contain/Fill`), text run, icon, clip
  push/pop with radius, layer opacity, backdrop blur request). The backend
  maps it; the core never names an engine type.
- **Inspect**: `ui.inspect()` → stable, indented text dump (kind, key, rect,
  resolved style tokens, text) used by tests and by models. Document it as
  the first thing to read when a layout looks wrong.

- **Layout explanations** (the base of Phase 8's lint): every resolved rect
  records *why* it got its size and position (which constraint won: fixed
  size, min/max, grow share, content size, parent clamp). `ui.explain(key)`
  prints that chain in plain English, e.g. "`sidebar` is 280 px because
  `width(Px(280))` wins over the parent's 240 px; it overflows `shell` by
  40 px and overlaps `content` (x 240–280). Fix: `width(Fraction(0.25))` with
  `min(S…)`, or collapse it below the `Md` breakpoint".

Tests: layout snapshots via `inspect` at 3 sizes × 2 scales; hit testing
under clips and overlays; text clamp/ellipsis; pixel snapping; `explain`
output pinned for a few deliberate mistakes.

## Phase 3 — 100 % responsive

- **Container-based breakpoints** (like CSS container queries, not only window
  size): `Xs < 480 ≤ Sm < 768 ≤ Md < 1024 ≤ Lg < 1440 ≤ Xl`, values tunable
  in the theme. Any modifier takes a responsive value:
  `.columns(resp![Xs: 1, Md: 2, Lg: 3])`, `.direction(resp![Xs: Column, Md:
  Row])`, `.hidden_below(Md)`, `.pad(resp![Xs: S4, Lg: S8])`.
- **Fluid values**: `clamp(min, preferred %, max)` for widths and type size
  (Display/Title scale smoothly between breakpoints), `auto_fill(min)` grids,
  `max_width` content columns that centre on wide screens.
- **Adaptive patterns as components**: sidebar that collapses to an icon rail
  and then to a drawer; toolbar whose overflowing items move into a "more"
  menu; split panes with min sizes that stack vertically when narrow; dialog
  that becomes full-screen sheet below `Sm`; tables that hide low-priority
  columns.
- **Text never breaks layout**: long words break or ellipsise, labels in
  rows shrink before their siblings by declared priority (`.priority(n)`).
- **DPI**: everything in logical units, snapped to physical pixels; tested at
  1×, 1.25×, 1.5×, 2×.
- **Responsive test sweep**: a helper renders a view at a list of widths
  (360, 480, 640, 768, 1024, 1280, 1440, 1920, 2560, 3840) × scales and runs
  the overlap/overflow checks at every one; every recipe passes the sweep.
  A gallery mode shows a story with a draggable width handle.

## Phase 4 — Interaction: engine input and gestures, UI routing, text editing

Follow the ownership table at the top. Motioner already has
`crates/motioner_ui/src/text_edit.rs` and pointer/drag code in
`app/pointer.rs`, `app/drag.rs`: read them, move gesture and UI logic to
GUIMaker, and leave Motioner importing it (no copy remains). The engine only
supplies raw events and the clock:

- **Engine (`ferrous_events`)**: raw events only (pointer, keys, wheel,
  IME, clipboard, file drop, modifier state), the platform double-click time
  and distance, and the injected clock.
- **GUIMaker (`guimaker::input`)**:
  - gesture recognition as a pure, tested state machine fed by those raw
    events: click, double and triple click, long press, drag start
    threshold, pointer capture, coalescing, key repeat;
  - the hit-test spatial index;
  - routing through the UI tree, hover, press and focus;
  - scroll controllers;
  - shortcuts, overlays, drag and drop between UI elements;
  - the text-editing engine.

- **Algorithms first** (this toolkit runs on every input event, so it must be
  the cheapest part of the frame; document the complexity of each in
  rustdoc and prove it with benches at 100, 1 000 and 10 000 interactive
  nodes):
  - hit testing through a **spatial index** (uniform grid or BVH over the
    laid-out interactive rects, z-order and clips baked in) rebuilt only
    when layout changes, incrementally for moved subtrees; a query is
    O(log n) or O(cell), never a walk of the whole tree;
  - hover is recomputed only when the pointer moves or the layout under it
    changes; a pointer move that stays inside the same hovered element does
    no work beyond one rect test (cache the last hit and its rect);
  - pointer moves are coalesced to one per frame; high-frequency drags use
    the latest position and keep sub-frame samples only when a widget asks
    for them (e.g. drawing strokes);
  - event dispatch is allocation-free: bubbling and capture paths are walked
    by parent indices; messages go into a reused buffer;
  - focus order, type-ahead and shortcut lookup use precomputed tables
    rebuilt only when the tree changes (a hash map from chord to handler, a
    sorted focus array);
  - text editing: caret and selection positions come from cached line and
    grapheme boundaries of the laid-out text, invalidated only when the text
    or its width changes; a keystroke in a 10 000-character field re-shapes
    only the affected line.
- **Pointer (GUIMaker side, consuming engine gestures)**: click, double and
  triple click, right click / context menu, middle click, long press, drags
  with capture outside the window, drag and drop between UI elements with
  typed payloads and drop-target highlight, hover
  intent (delay before tooltips), wheel and trackpad scroll with
  `Shift`-horizontal, cursor icon per element (text I-beam, resize, grab).
- **Focus**: `Tab`/`Shift+Tab` order from the tree (overridable with
  `.tab_index`), focus scopes and traps for dialogs, restore focus on close,
  arrow-key navigation inside lists, menus, grids, radio groups and tabs,
  `Enter`/`Space` activation, `Esc` closes the top overlay, type-ahead in
  lists and menus.
- **Shortcuts**: a registry (`Ctrl+S`, `Ctrl+Shift+P`…) with scopes (global,
  focused element, overlay), conflict detection reported by the lint, and
  display strings for menus and `kbd` (platform-correct).
- **Text-editing engine** (`TextEditor` state + pure functions, used by
  `text_field`, `textarea`, `number_field`, search boxes, the command palette
  and any app):
  - caret movement by char, word (`Ctrl+←/→`), line (`Home`/`End`), document
    (`Ctrl+Home/End`), up/down keeping the preferred column in multi-line;
  - selection with `Shift` + any movement, `Ctrl+A`, mouse drag, double-click
    word, triple-click line, `Shift+click` extend, auto-scroll while dragging
    past the edge;
  - `Ctrl+C/X/V` through a host clipboard trait (the Motioner and engine
    clipboards implement it), paste sanitising for single-line fields;
  - `Backspace`/`Delete` by char, `Ctrl+Backspace/Delete` by word;
  - undo/redo (`Ctrl+Z`, `Ctrl+Y`, `Ctrl+Shift+Z`) with typing coalesced into
    word-sized steps;
  - grapheme-correct editing (emoji, accents, combining marks; a
    `unicode-segmentation` dependency needs a decision), IME composition
    events from the host, caret blink from the injected clock, horizontal
    scroll of long single-line text keeping the caret visible;
  - validation hooks (numeric, max length, pattern) and `Enter` submit /
    `Esc` revert for fields.
- Exhaustive tests: a table of (text, caret, selection, key) → expected state
  for every shortcut above, plus pointer sequences for double/triple click and
  drag selection.

## Phase 4B — Engine: canvas and scene picking

Generic picking for anything the engine draws, so editors and games built on
FerrousEngine never write their own (Motioner's `app/canvas.rs`,
`gizmo.rs`, `bounds.rs` hit tests are the first consumers; read them first
and list what they compute by hand today).

- Picking queries on the 2D scene: point pick (topmost or all hits) with
  transforms, rotation, scale and skew; exact path-fill containment
  (non-zero / even-odd), stroke hit within a tolerance in screen pixels
  (independent of zoom), text bounds, image alpha threshold (optional),
  respecting clips, masks, visibility and z-order; marquee (intersect /
  contain) and lasso queries; nearest-point and nearest-edge queries for
  snapping.
- GPU ID-buffer picking as an alternative for very dense scenes (render ids
  to a small offscreen target on demand only, read back one pixel/region);
  document when each method wins, with benches.
- Camera helpers: screen ↔ world transforms, zoom to cursor, pan, fit to
  bounds, with the same math the renderer uses (one source of truth).
- 3D: ray from cursor, raycast against meshes/AABBs if the 3D side lacks it
  (only what is generic; keep scope tight).
- Uses the generic spatial index (rebuilt incrementally when objects move);
  budget: a point pick < 20 µs with 10 000 shapes, a marquee over 10 000
  shapes < 1 ms; zero allocations per query with a reused result buffer.
- Docs page "Picking" in the engine, an example (click-to-select, marquee,
  drag) and tests with rotated, nested and clipped shapes.
- Motioner (Phase 10) replaces its hand-written canvas hit tests with these
  queries; what a hit *means* (selection, gizmo handle, snapping rules)
  stays in Motioner.

## Phase 5 — FerrousEngine drawing and the screenshot loop

- `guimaker_ferrous`: draw commands → engine API (reuse the frame batch as
  the engine does since `e482b3c`), text through `ferrous_font` with kerning
  and real weights, SVG/vector icons, images and textures, clips. A mapping,
  nothing more.
- A host adapter for `FerrousApp` (feed input events and scale factor in,
  messages out, paint in `draw_ui`), ~one screen of glue for an app to adopt.
- **Headless screenshot**: render any view to PNG at a given size, scale and
  theme; the offscreen target and readback are engine features (prompt 05),
  GUIMaker only asks for them.
  A CLI in the gallery: `gallery shot <story> --size 1280x800 --scale 1.5
  --theme light --out out/x.png`.
- **Engine gaps → generic engine features** in the dev clone, only if the
  audit proved them missing: e.g. rounded clip masks, multiple soft shadows
  per rect, backdrop blur for overlays, gradient borders, per-corner radii in
  the shader. Each with docs and tests; the backend uses a documented fallback
  until the pin moves.
- Golden screenshots: tolerance-based comparison (per-channel delta and
  pixel-count budget), goldens stored in the repo, a single command to
  update them with a reason.

## Phase 6 — Components (each one finished, not sketched)

Every component: all states (rest, hover, pressed, focus-visible, disabled,
selected/checked, loading where it applies), keyboard support built on the
Phase 4 utilities, responsive behaviour, both densities, light and dark,
animated state transitions, a gallery story, golden screenshots, rustdoc with
a 3-line example.

- Actions: `button` (`Primary/Secondary/Ghost/Danger/Link`, sizes, leading/
  trailing icon, icon-only with mandatory tooltip, loading), `icon_button`,
  `toggle_button`, `button_group`/segmented control, `split_button`.
- Inputs: `text_field` (placeholder, prefix/suffix icon, clear, error and
  helper text, selection, clipboard via host trait), `number_field` (drag to
  scrub, step, units), `textarea`, `checkbox`, `radio_group`, `switch`,
  `slider` (+ range), `select`/combobox with search, `color_swatch` picker,
  `path_field` (text + browse button that emits a message), `form_row`
  (label column alignment done by the layout, not by the caller).
- Display: `text`, `heading`, `badge`, `tag`, `kbd`, `avatar`/initials,
  `thumbnail` (image with fit, placeholder shimmer while loading, aspect),
  `progress` (bar, ring, indeterminate), `spinner`, `skeleton`, `empty_state`
  (icon, title, body, actions), `callout`/banner (info/success/warning/danger).
- Containers: `card` (static and interactive: hover lift via elevation),
  `list` + `list_item` (leading media, title, subtitle, trailing meta,
  selected), `table` (header, sortable columns, virtualised rows), `tabs`,
  `accordion`, `section` (overline header + content), `sidebar` + `nav_item`,
  `toolbar`, `status_bar`, `app_shell` (title bar, sidebar, content,
  inspector slots).
- Overlays (a layer stack with focus trap and `Esc`): `tooltip` (delay,
  placement with flip), `popover`, `menu` + `context_menu` (submenus,
  shortcuts column, separators, checkable items), `dialog`/modal (title,
  body, footer actions, sizes, backdrop dim + blur), `command_palette`
  (fuzzy filter, keyboard-first), `toast` stack.
- Icons: one coherent vector set (start from Motioner's `icons.rs` 16-unit
  grid, moved here and made generic; Motioner then imports it — no copy
  remains). At least the 60 icons a desktop app needs (file, folder, plus,
  search, settings, chevrons, close, play/pause, undo/redo, trash, copy,
  image, film, music, font, clock, star, more, external, check, warning…).

## Phase 7 — Motion and polish

- Transitions on style properties (colour, elevation, opacity, transform)
  driven by the injected clock; enter/exit animations for overlays (fade +
  scale 0.98→1, slide for toasts); list item insertion fade.
- `prefers_reduced_motion` theme flag turns them into instant changes.
- Smooth scrolling with overlay scrollbars that appear on hover/scroll.
- Focus ring that animates in, only for keyboard focus (`focus-visible`
  semantics).
- A "polish pass" over the gallery at 1×, 1.5× and 2×: optical alignment of
  icons with text, consistent icon sizes per type role, hairlines crisp,
  shadows soft and never banded. Fix what the screenshots show.

## Phase 8 — Design lint and diagnostics (the guard rail for models)

`guimaker::lint(&laid_out_tree, &theme) -> Vec<Finding>` with severity, the
offending key paths, the rects, the **cause** (the `explain` chain from
Phase 2) and a fix hint written for a model to act on. Findings print as
readable sentences ("`recent_card[2]` overlaps `examples_row` by 18 px
vertically at width 768 because the grid has a fixed height(Px(300)) and 3
rows of content need 342 px; remove the fixed height or wrap it in
`scroll`"). A debug overlay in the gallery and in host apps (toggle with a
shortcut) outlines overlaps in red, overflow in orange, and shows the
explanation of the element under the cursor.

- overlapping siblings that are not in a `stack`/overlay, and children that
  escape their parent;
- text clipped or truncated without an explicit `lines(n)` / ellipsis;
- contrast below the role threshold on the actual background under it;
- `Px` escape hatches and spacing not on the token scale;
- near-miss alignment (edges 1–3 px apart that should match);
- inconsistent radii or elevations among siblings of the same kind;
- hit targets smaller than 24×24 px; icon-only button without tooltip;
- more than one `Primary` action in the same group; empty containers;
- content wider than its `max_width` column; horizontal overflow;
- breakage only at some widths (the lint runs across the Phase 3 sweep and
  names the first width where each problem appears);
- focus traps without an exit, unreachable focusables, duplicate shortcuts.

Plus `gallery lint <story> [--sweep]` and a test helper `assert_clean(view)`.
Every gallery story and recipe must lint clean across the sweep.

## Phase 9 — Recipes, docs and the HTML + CSS comparison

- **Recipes** (gallery stories and copyable code, each lint clean, each with
  goldens in dark and light): app launcher / start screen, New-item dialog
  with form, settings window (sidebar nav + forms), file browser grid,
  empty-state first run, editor app shell (toolbar, sidebar, inspector,
  status bar), command palette, toasts after an action.
- **Docs** (`crates/guimaker/docs/` in the engine, English, in the mkdocs nav): getting started (a
  window with a button in under 30 lines), concepts (tree per frame, keys,
  messages, tokens), every component, theming, the backend contract,
  screenshots and lint, and **`design-guide.md` written for models**: "before
  writing a screen, pick the closest recipe"; hierarchy rules (one primary
  action, type roles per level, at most two text tones per block); spacing
  rules (S2 inside controls, S3–S4 between related items, S6–S8 between
  groups, S12+ between sections); when to use cards vs lists; the
  write → screenshot → lint → fix loop. Update `README.md` and rewrite
  `docs/roadmap.md` to the new reality.
- **Comparison**: write the start-screen recipe once in GUIMaker and once in
  plain HTML + CSS (a static `out/compare/start.html` with the same tokens,
  hand-written, no framework) and report lines of view code, number of
  numeric literals, and side-by-side screenshots. Target: fewer lines than
  HTML + CSS together, **zero** numeric literals in the GUIMaker view.

## Phase 9B — The lightest UI library: optimisation and comparison

1. **Instrumentation** in GUIMaker (behind a feature, zero cost when off):
   per-frame counters (nodes built, nodes re-laid out, draw commands,
   commands reused, allocations, bytes), shown in the gallery's debug overlay
   and exported as JSON for benches.
2. **Benchmarks** (`criterion` or a hand-written harness; decision first):
   build + layout + route + command list for the start screen, a 10 000-row
   list, a 500-field form, a dense editor shell; idle wake-ups; memory.
   Stored results so regressions are visible across phases.
3. **Engine side, generic** (dev clone): damage-rect / scissored partial
   redraw, persistent GPU vertex/instance buffers grown geometrically and
   never shrunk per frame, one draw call per texture/clip batch, glyph atlas
   that grows on demand and evicts unused glyphs, staging buffers released,
   texture budget respected (prompt 10's), GPU timings via timestamp queries
   where supported. Documented in an engine "UI performance" page.
4. **Comparison**: build the same start screen in **egui** and **Slint**
   (and, if feasible in time, a WebView2/HTML page as the Electron-class
   reference) as standalone bench apps in `bench/guimaker_compare/` of the
   engine repo (excluded from the workspace; their dependencies never reach the library; a decision
   records that). Measure for each: idle CPU/GPU, CPU and GPU per frame while
   hovering and typing, RSS, GPU memory, startup to first frame, release
   binary size. Report a table. Where GUIMaker is not the lightest, profile
   (`cargo flamegraph`/ETW, heap counts) and fix it, or record precisely
   why and what it would take.
5. Optimise until every row of the budget table is met; the bench numbers go
   in the GUIMaker README.

## Phase 10 — Motioner on GUIMaker: the plumbing only (the frontend is not part of this prompt)

**The user assigned the Motioner frontend to a separate Claude session after
this prompt.** That covers:
- the start screen and the New project dialog;
- the menus, dialogs, tooltips and toasts;
- the redesign of the editor panels.

Do not redesign or rebuild any Motioner screen here. This phase only makes
the frontend session possible, with no duplicated code left:

1. **Motioner ADR** (`pctl decision-new`), **"Editor chrome on
   GUIMaker"**:
   - it amends D-0004, D-0018, D-0020 and D-0021;
   - Motioner depends on `guimaker` and `guimaker_ferrous` by path into
     `engine/ferrous/crates/`, like the other engine crates, so one engine
     copy is built;
   - `scripts/check_boundaries.py` allows `guimaker*` only in
     `motioner_ui`.
2. **Theme.** Motioner's theme becomes a GUIMaker `Theme` built from the
   D-0021 seeds: the same accent, compact density for the editor and
   comfortable density for the start screen. `theme.rs` keeps only what is
   Motioner-specific.
3. **Controllers, one implementation.**
   - Motioner's text fields use the GUIMaker text-editing engine, and
     `text_edit.rs` is deleted.
   - The pointer, drag and scroll helpers that GUIMaker now owns are
     deleted from Motioner.
   - The composition canvas is a GUIMaker `canvas` element: it receives
     GUIMaker gestures in local coordinates and picks with the engine
     (Phase 4B). Its clicks never go through UI widget hit testing.
   - Every existing screen keeps working and looking as it does today; the
     goldens and parity tests stay green.
4. **Handoff for the frontend session.** Write
   `docs/frontend-handoff.md` in Motioner, covering:
   - the GUIMaker recipes and components each Motioner screen should use
     (launcher, app shell, dialogs, inspector forms, lists, timeline rows);
   - what is still hand-placed and where;
   - the gaps the library has for those screens;
   - screenshots `out/ui/p15-10-now-*.png` of every screen at 1280×800 and
     1920×1080, at scale 1 and 1.5, as the "before" for that session.

## Phase 11 — Dogfood: the GUIMaker designer, and the backlog

- Revive the designer as `crates/guimaker_designer` on the library, as the
  best stress test. Read the old one in the read-only clone for ideas, but
  write it fresh:
  - a canvas with the app shell recipe;
  - a component palette;
  - an inspector built from forms;
  - a live preview of the designed view, rendered by the library itself;
  - codegen that emits **GUIMaker builder code**. The old generator emitted
    absolute positions; delete it.

  Keep the scope to what the library proves, and put the rest in the
  roadmap.
- Backlog (`crates/guimaker/docs/roadmap.md` in the engine, and Motioner
  tasks through
  `pctl task-new`):
  - the Motioner frontend (the Claude session, from
    `docs/frontend-handoff.md`);
  - the agent panel on GUIMaker (prompt 14);
  - the `ferrous_ui_core` pieces left after the Phase 0 move;
  - **port `ferrous_editor` to GUIMaker** (the retained `UiSystem` is draw-only
    since Phase 4; the future Unreal-style editor replaces it);
  - accessibility tree export;
  - RTL and IME text input.

## Phase 12 — Report

`pctl session-log` in Motioner and a session note in `crates/guimaker/docs`. Final report
**in Spanish**: what changed per phase; before/after screenshots of the start
screen; the HTML + CSS comparison numbers; the resource table against the
budget and against egui/Slint/WebView2, and Motioner's idle and startup
numbers before and after; decisions (G-NNNN, Motioner ADR)
and dependencies; engine changes waiting for a push and pin bump; gate
results in every repository; free space on C:; what is left.

## Review findings after Phase 0 (reviewer, 2026-11-21)

Phase 0 is accepted. The engine part is pushed as `681888b` on
`feat/guimaker`. Python boundary check plus its 17 tests are green, re-run
by the reviewer. These rules now apply:

- **Controller move timing: accepted.** The spatial index and incremental
  layout move in Phase 2, and scroll, the lists, the tree and event routing
  move in Phase 4, following G-0001's move table. Each move is measured
  with the prompt 18 benches before and after. Each one deletes the old
  implementation in `ferrous_ui_core`, so one implementation is left, and
  Motioner must still build.
- **GPU-free base usable by every GUIMaker crate: accepted** as G-0001
  states it. Only `guimaker_ferrous` may touch the drawing crates.
- **The `.gitignore` fix was a good catch.**
- **The `StartScreen::action_at` bug** (clicking a form field returns
  `Cancel`, at `start.rs:368-371`) is **not** a Phase 10 fix: Phase 10 does
  no frontend work. List it in `docs/frontend-handoff.md` under "known
  bugs", together with the transparent New project card, the labels that
  overlap the fields and the clipped Create button seen in
  `p15-00-before-newproject.png`. The Claude frontend session fixes them.
- **Engine fmt debt.** The figure 727 counts `Diff in` hunks, not files;
  report it that way. The debt itself stays out of scope, but every
  GUIMaker crate must be fully fmt-clean:
  `cargo fmt -p guimaker -p guimaker_ferrous -p guimaker_gallery -- --check`
  goes into the phase gate as soon as a crate exists. So do
  `cargo clippy -p <guimaker crates> --all-targets -- -D warnings -W clippy::pedantic`
  and `cargo doc -p <guimaker crates> --no-deps` with
  `RUSTDOCFLAGS=-D warnings`. Whole-workspace clippy and doc stay red only
  from the known debt (26 clippy, 22 rustdoc, the 4 `ferrous_ui_core`
  doctests); any new item in those lists fails the phase.
- **Phase 0B was not reported.** It is a re-check only: run Motioner's
  startup with the prompt 17 startup trace, three runs, and report
  time-to-first-composed-frame and whether "Not responding" ever appears.
  Run it at the start of the next phase.
- **Scope check on taffy.** `taffy 0.7.7` is accepted because it is
  already in the lock. Memoise the layout by hash as G-0001 says, and
  report Phase 2 layout time for the start-screen recipe against the
  0.3 ms fallback threshold.

## Review findings after Phase 0B and Phase 1 (reviewer, 2026-11-21)

Accepted. The engine part is pushed as `12319e7` on `feat/guimaker`.
The reviewer re-ran the GUIMaker gate, and these were green: fmt, clippy
pedantic, tests, rustdoc, machete and the boundary check. Startup is fine:
the median first presented frame is 480.8 ms, and "Not responding" never
appeared. Phase 2 starts with these fix lanes:

1. **Filled accent with white text.** Dark text on a light-blue button is
   legal but unusual: pro tools (Figma, Unreal, VS Code, Blender) use light
   text on a filled accent. Keep `accent` (`#5B8BFF` on dark) for focus,
   links, selection outlines and accent text.
   - Add `accent_fill`, `accent_fill_hover` and `accent_fill_pressed`,
     derived from the same seed with a lower OKLCH lightness, plus
     `text_on_accent_fill`, which is the light neutral.
   - The guarantee is ≥ 4.5:1 in all three states and in all three themes.
   - Filled (primary) buttons use the fill roles.
   - Record the choice in `docs/theme.md`.
2. **Literal guard.** The report says no literal values exist outside
   `guimaker::theme`. Make the gate prove it: add a check (a script or a
   test) that fails when a GUIMaker crate calls a literal `Color`
   constructor (`hex`, `srgb`, `oklch` and the others) or a raw px number
   for spacing or radius outside `theme`. Tests and the theme itself are
   exempt. Phase 8's lint is for apps; this check is for the library.
3. **Report tables.** The contrast tables in the Phase 1 report lost every
   column except `text`. Paste the full `contrast_table` output from now
   on, with every role on every surface.

Phase 2 itself must also cover:
- the controller move from G-0001 (spatial index and incremental layout
  out of `ferrous_ui_core`), with the prompt 18 benches before and after
  and one implementation left;
- the start-screen recipe's layout time against the 0.3 ms threshold;
- the prompt 19 debug hooks designed as data: stable ids and paths, rects,
  resolved tokens, cache state, the repaint reason, and draw commands with
  their attribution.

## Review findings after Phase 2 (reviewer, 2026-11-21)

Accepted. The engine is pushed as `c866f7c` (GUIMaker) and `59e1c3f` (the
controller move). Motioner pins `59e1c3f`.

The reviewer re-ran these, all green:
- the strict GUIMaker gate;
- the literal guard and the boundary check;
- `cargo check --workspace --all-targets` on the engine (the phase only
  checked the crates it touched);
- the tests of the touched crates;
- both wasm32 checks;
- the full Motioner gate in a clean worktree against the pushed pin.

Measured:
- layout on the start screen: 0.164 ms, against the 0.3 ms threshold;
- 0 allocations in steady state;
- GUIMaker layout at 8,192 nodes: 3.885 ms, against 6.328 ms for the
  engine's full pass;
- the spatial index query: 37.0 → 33.3 ns.

Phase 3 starts with these:

1. **Idle cost after the engine lost its layout early-out.**
   `ferrous_layout` now runs a full pass. Prove that no engine app runs it
   while nothing changes:
   - run `scripts/perf_bench.py --check` against `perf_budgets.json` for
     the idle start screen, the idle editor, hover and heavy UI;
   - add an engine test proving a clean `UiTree` frame skips layout;
   - if anything regressed, restore a single paint-or-layout dirty check in
     the engine. That check is part of drawing, not a controller.
2. **The linear hit test in `ferrous_events` is temporary.** Phase 4 must
   remove it: routing goes through GUIMaker, or, for the legacy `UiTree`
   path that stays until it is deprecated, a bounded walk with a measured
   cost in the report. Do not let it become the permanent state.
3. **`docs/frontend-handoff.md`.** The reviewer fixed its last paragraph:
   Phase 10 never changes the start screen geometry, and the Claude
   frontend session does that. Keep it that way in later edits.
4. **Motioner fmt.** In Motioner, run `cargo fmt -- --check`, never
   `--all`, and do not use a per-package list either. That is the gate
   command, and it does not reach `engine/ferrous`.

## Review findings after Phase 3 (reviewer, 2026-11-21)

Phase 3 is accepted:
- the sweep passes 40/40 for the recipe and all five patterns;
- the layout frame paths stay under 0.3 ms;
- the engine idle proof holds (`layout_skip.rs`, and the idle start and
  editor runs are inside their budgets).

### Incident: the engine dev clone was deleted (new hard rule)

In the gate worktree, `engine/ferrous` was a Windows junction pointing at
the dev clone. `git worktree remove` followed it and deleted the dev
clone. No commit was lost. Local files that were never pushed were lost,
such as ignored notes and tools, and the `target` dir.

From now on:
- **Never create junctions, symlinks or directory links** to the dev
  clone or to any working repository.
- To gate Motioner against an engine that is not committed yet, init the
  submodule normally in the worktree. Then copy the change into it:
  `git -C <dev clone> diff HEAD --binary > %TEMP%\engine.patch`, apply it
  inside the worktree's `engine/ferrous` with `git apply`, and copy the
  untracked files. Or run `cargo check`/`test` against the dev clone from
  its own directory.
- Before any `git worktree remove` or `rm -rf`, list the target and check
  that it holds no link (`dir /AL`). If it does, stop.

### Fix lanes for Phase 4

1. **Bench sample size.** `heavy_ui.input_to_present` failed the budget on
   a **single** sample (11.986 against 8.494 ms). One sample is no
   measurement.
   - `perf_bench.py` must take at least 30 input-to-present samples per
     run and report p50 and p99 from them.
   - Phase 4 changes input routing, so this metric is its main proof.
   - `heavy_ui.frame_ms_p99` (11.306 against 11.231, +0.7 %) is in the
     noise. Re-check it in the same run.
2. **Remove the linear hit test.** It is in `ferrous_events`, marked
   `TODO(prompt 15, phase 4)`, and Phase 4 removes it, as required after
   Phase 2.
3. **Cold frame.** The cold frame (0.26–0.48 ms) pays a second responsive
   pass because containers exist only after the first pass. That is fine
   for now. Record it in `docs/responsive.md`, and revisit it in Phase 9B
   if the start screen's first frame budget needs it.

## Review findings after Phase 4 (reviewer, 2026-11-21)

Phase 4 is accepted.

| What | Result |
|---|---|
| Routing cost per event | 28 ns, flat from 100 to 10,000 nodes |
| Allocations per event in steady state | 0 |
| Tree flattening (10,000 rows) | 1,780 → 17.2 µs |
| Input-to-present, hover | 0.88 / 1.68 ms (p50 / p99) |
| Input-to-present, scrub | 2.73 / 4.07 ms (p50 / p99) |
| Implementations of the scroll, virtual list and tree controllers | one, in GUIMaker |

The heavy UI budget for input-to-present was re-derived from 356 samples:
15.5 / 18.3 ms (p50 / p99) at about 50 fps, which is one frame of cadence.
That is accepted, because the old 8.494 ms came from a single sample.

The fix lanes for Phase 4B follow.

1. **The engine's retained UI no longer gets input.** The runner stopped
   dispatching to `ferrous_gui::UiSystem`, so `ferrous_editor` and every
   engine app or web app built on `UiSystem` widgets draw but cannot be
   clicked. That is the user's split (the engine is draw calls), but it
   must not be silent.
   - List every affected crate and example in the report.
   - State in the `ferrous_gui` and `ferrous_engine` READMEs and docs that
     the retained UI is draw-only and that input and controllers go
     through GUIMaker.
   - Add the port of `ferrous_editor` to GUIMaker to the Phase 11 backlog,
     because the future Unreal-style editor replaces it.
   - Retained-UI input is not restored in the engine.
2. **Three runs for every input-to-present number.** `editor_hover` and
   `timeline_scrub` were one run each. Report the median of three, the
   same as `heavy_ui`.
3. **A budget change gets its own line in the report.** Name the old
   value, the new value, and the reason. P4 did this. Keep doing it, and
   never relax a budget to make a check pass without that line.

## Review findings after Phase 4B (reviewer, 2026-11-22)

The Phase 4B code is accepted. The engine is pushed as `e07d3fc` and
Motioner pins it. Accepted results:

- **One spatial grid:** `ferrous_core::spatial` is shared by GUIMaker
  routing and by engine picking, and no bench got slower. Hover routing went
  from 28.1 to 18.0 ns and Tab from 270 to 76 ns.
- **Picking:** it meets its budgets and has docs, an example and tests.
- **GPU id-buffer:** this is a conscious, documented deviation. It moves to
  Phase 5 and 9B, once the engine has a 2D scene target.
- **GUIMaker clippy pedantic** runs with `--no-deps`, so the lint does not
  land on `ferrous_core`'s old debt. Accepted.

### Blocking: CPU regression in hover and scrub

`perf_bench.py --check` fails on CPU:

| Scenario | Metric | Budget | Measured |
|---|---|---|---|
| `editor_hover` | `cpu_percent` | 6.813 | 9.399 |
| `editor_hover` | `tail` | 4.174 | 6.799 |
| `timeline_scrub` | `cpu_percent` | 7.998 | 8.892 |
| `timeline_scrub` | `tail` | 3.685 | 4.892 |

Hover was 5.509 % in Phase 3, so this is a regression, not noise. "The
machine was loaded" is not a finding. The report also says the new binary
draws more frames in the same time: 1,119 hover frames against 735 in
Phase 4. Under on-demand redraw, frames are only drawn when something
changed, so extra frames are the bug to find.

Fix it before any Phase 5 work:

1. **Bisect** with the same harness and the same machine state. Build
   Motioner release at the pins `55d2b2a` (Phase 3), `5c851f6` (Phase 4)
   and `e07d3fc` (Phase 4B), and run `editor_hover` and `timeline_scrub`
   three times each. Report CPU, tail, the frames presented and the input
   events sent.
2. **Check the harness first.** If the Phase 4 change to `perf_bench.py`
   (the 30-sample minimum, the probes) sends more input events, the
   budgets are no longer comparable. In that case:
   - make the CPU scenarios send exactly the input they sent before;
   - put the input-to-present probes in their own runs.
3. **If the engine causes it,** for example the runner change that stopped
   dispatching to `UiSystem` and now redraws on every raw event, fix the
   engine so that a pointer move that changes nothing presents nothing.
   Prove it with a test.
4. **Close** with `perf_bench.py --check` green on all scenarios, three-run
   medians, and no relaxed budget.


## Review findings after the Phase 4B fix (reviewer, 2026-11-22)

**Accepted.** The bisect is good work. The `55d2b2a` (Phase 3) pin already
fails the hover and scrub budgets on this host, and the frame counts follow
how many posted moves Windows coalesced, so the engine did not regress. The
redraw gate in `runner/events.rs` is the same at every pin. These parts of the
harness change are right and stay:
- `SendMessageW` for the moves;
- `input_events_sent` in every run;
- the cursor parked and then restored;
- the scrub seeks over one TCP request and reply, instead of starting a
  `motioner ctl` process for each seek;
- the input-to-present probe moved into its own `heavy_ui_probe` run.

**Not accepted. It is not committed and must be reworked.**

1. **The workload was tuned until it passed.** `HOVER_MAX_EVENTS = 580` and
   `SCRUB_MAX_REQUESTS = 220` cap the input, and the rest of the measured
   window is left idle. The bisect logs (`cap900-*`, `check-green*`) show the
   caps went down until the numbers fit. CPU averaged over a half-idle
   window is a lighter workload than the one the budgets were taken from, so
   this relaxes the budget without saying so.

   Do this instead:
   - Keep the input continuous for the whole measured window, at a fixed
     cadence (16 ms for hover, 60 ms for scrub), using `SendMessageW` or TCP
     so every event is processed once.
   - The workload definition changed, so the hover and scrub budgets must be
     derived again from three-run medians on a quiet host. Use explicit
     budget-change lines that give the old value, the new value and the
     reason ("benchmark definition changed: deterministic delivery").
   - Add a host-independent metric, `cpu_ms_per_presented_frame`, together
     with a budget for it. It is the number that actually catches an engine
     regression.
2. **The engine test proves nothing.**
   `a_pointer_move_the_app_does_not_forward_presents_no_frame` never sends
   a pointer move. It is the idle test under another name. Either feed a real
   `CursorMoved` through the same decision path `Runner::window_event` uses
   (extract it if you have to) and assert zero frames, or delete the test.
3. **Encoding again.** The edit saved `scripts/perf_bench.py` with about
   93,000 mojibake characters, in every comment banner and in the heavy_ui
   fixture string. The fixture text feeds the benchmark, so this would have
   changed what heavy_ui measures. The reviewer repaired the working tree,
   restoring the banners and keeping the fixture line byte-identical to
   `HEAD`. The session file also had CRLF line endings, which were normalised
   to LF. `check_encoding.py` passed both times because it does not look for
   mojibake. Two fixes:
   - Add mojibake detection to `scripts/check_encoding.py` (the sequences
     `Ã`, `Â` followed by a non-letter, and `â€`), with a test.
   - `HEAD` already has one mojibake `â€”` in the heavy_ui fixture. Fix it to
     `—` as part of the re-derivation in item 1 and note it there.
4. **Host.** AutoCAD (`acad`) held about one core for the whole run. The
   re-derivation and the final `--check` run only when the user says the host
   is quiet: AutoCAD closed, no builds running. Check this with a 10 s idle
   CPU sample before the runs, and report it.


## Review after the Phase 4B fix, second pass (reviewer, 2026-11-22)

**Accepted and pushed** as engine `dd382bd` and Motioner `c091fca`, `edf7343`,
`996d552` and `19bb2e5`. The reviewer ran the gate again: the engine files
changed, the workspace, wasm32, and the full Motioner gate in a clean worktree
are all green. What is in:
- **Continuous, deterministic input.** The hover and scrub budgets were
  derived again, each change has an explicit budget-change line, and the max
  is the median × 1.15, the same as before.
- **`cpu_ms_per_presented_frame`** on every scenario.
- **A real `CursorMoved` test** with a positive control. `event_requests_frame`
  is shared with `Runner::window_event`.
- **The mojibake guard.** It found and repaired real bugs: the window title,
  the menu labels (`Open project…`) and several doc comments.
- **A stale `Cargo.lock`.** It still listed `ferrous_events` as a dependency,
  which the `e07d3fc` pin no longer has. That entry was dropped.

**One budget is still red: `heavy_ui_list_1000.cpu_percent`.** It measures
10.453 against a max of 9.673, and the reruns stayed between 10.1 and 10.8.
It was not relaxed. It is resolved by a check, not by a choice:
1. On the same quiet host, build Motioner release at the `55d2b2a` pin
   (Phase 3) and at the current pin. Run `heavy_ui_list_1000` three times on
   each and report the median CPU and `cpu_ms_per_presented_frame`.
2. **If `55d2b2a` is also above 9.673,** the host baseline moved since
   prompt 18 P7 and the code did not regress. Derive the budget again with
   the line `heavy_ui_list_1000.cpu_percent: 9.673 -> X (host baseline moved
   since prompt 18 P7; 55d2b2a measures Y)`.
3. **If `55d2b2a` passes,** it is a regression. Bisect it and fix it before
   any Phase 5 code lands.

**Nit.** `cpu_ms_per_presented_frame` on the idle scenarios (`start_idle`,
`editor_idle`, `editor_hover_static`) divides CPU by a handful of frames, so
it is noise and not a guard. Leave those three budgets out and report the
value only. Apply it as a budget only when the run presents at least 30
frames.


## Review findings after Phase 5 (reviewer, 2026-11-22)

**Step 0 is accepted and pushed.** The `55d2b2a` pin measures above the old
budget too, so the budget line applies. The per-frame budget is now dropped
on the idle scenarios.

**Phase 5 is not accepted, and nothing of it is committed.** The reviewer ran
the gates and looked at the screenshots:

1. **The strict gate fails.** The report ran clippy without `-W clippy::pedantic`.
   - `guimaker_ferrous` has 13 lib and 26 test errors: precision-losing casts,
     strict float comparisons, missing doc backticks, and a function with 191
     lines.
   - `guimaker_gallery` has 22 lib and 23 test errors, mostly casts.
   - `rustfmt --check` fails on the changed `ferrous_ui_render` files.
   - The changed lines in `ferrous_ui_render/src/gui_batch.rs` add new
     clippy warnings: an unused `mut` at line 1280, functions with 8 to 10
     arguments, a `map_or`, a negated float comparison, and a complex type.

   Fix every one of them. A cast needs a checked conversion or a scoped
   `#[allow]` that states the invariant. Group the arguments into structs.
2. **The goldens freeze visible bugs** (see `start-light` and
   `components-light`):
   - **Padding is ignored on text leaves.** In a padded text element, such
     as every button and chip, the run is drawn at the border-box origin,
     top-left: "Primary", "Create", "Browse…", "Toggle". The run must sit in
     the content box, vertically centred on its line box. Fix this in
     GUIMaker layout or in the display list, wherever the run rect comes
     from. Add a display-list test that checks the run rect equals the
     content box, for padded text with each alignment.
   - **Labels on accent fills use the default text colour.** Black on blue
     fails the theme's own 4.5:1 rule. A filled accent must pair with
     `text_on_accent_fill`, through the theme pairing, not a manual tone.
     Add a test over the display list: every text run must meet the theme
     contrast against the fill painted behind it.
   - **The chips' labels overflow their pills.**

   Regenerate the goldens only after these are fixed, and log the reason in
   `UPDATE_LOG.md`. The gallery and the start story must use the theme's
   pairings. Do not hand-pick tones.
3. **Fake engine features.** "Rounded clip" falls back to a rectangular
   scissor, "backdrop blur" falls back to a tint, and unsupported icons fall
   back to a rect. An engine API that does not do what its name says is not
   a feature. Implement each one for real, or remove its API until it is
   real:
   - **Rounded clip:** an SDF clip mask (clip rect plus per-corner radii) in
     `gui.wgsl` that applies to quads, text and images, with a pixel test at
     the corner.
   - **Backdrop blur:** `SceneTarget2d` now exists, so sample it, using a
     two-pass downsample and blur at a bounded cost. Add a pixel test and a
     GPU timing figure.
   - **Icons:** render real vector icons through an engine path fill or a
     fixed `ferrous_svg`. A rect is not an icon.
4. **The perf `--check` is not valid.** It ran `--from-json
   out/reports/phase18/p15-p4b-fix-2.json`, which was measured before
   Phase 5. Phase 5 changes `gui.wgsl`, `gui_batch` and `gui_renderer`, and
   Motioner's UI draws through them. Build Motioner release in a clean
   worktree with the Phase 5 engine changes applied to the submodule
   (`git diff HEAD --binary` plus `git apply`, no links). Run every scenario
   three times on a quiet host, then run `--check` on that fresh JSON.
5. **Small items:**
   - The engine `out/` folder is untracked. Write the shots to an ignored
     path, or add `out/` to the engine `.gitignore`.
   - The start story shows "11:42". Confirm the time comes from the
     injected clock, so a golden run at another hour stays identical, and
     add a test for it.

The GPU picking is accepted as designed. CPU stays the default for point and
small marquee picks; GPU is used for bulk id-buffer and region picks, and
the crossover is documented.


## Review after Phase 5, second pass, and the deferred perf rule (reviewer, 2026-11-22)

**Phase 5 is accepted.** It is pushed as engine `4b94774` (render features
and picking) and `76d6cc8` (the GUIMaker crates). The reviewer ran the
strict gate with pedantic on `guimaker`, `guimaker_ferrous`,
`guimaker_gallery` and `ferrous_svg`, the engine tests, the workspace
check, wasm32 and the Motioner clean-worktree gate.

The goldens were inspected: the labels are centred, the text on accent
fills is white, and the chips fit their labels. The clip, blur and icon
effects are real and covered by pixel tests.

**Small leftover for Phase 6, lane 0:** on wasm32, `const CLIP` in
`ferrous_ui_render/src/pipelines/{quad,svg,text}.rs` is unused, which is a
new warning. Check whether the web shader path applies the rounded clip at
all. If it does not, wire it in so that web matches native, and prove it
with a test or a shader-source assertion.

**Deferred perf rule (user decision, 2026-11-22).** The user is working on
this machine and will not close apps between phases. From now on:
- No phase runs the full `perf_bench.py` suite or a fresh `--check`.
- Each phase still runs the host-independent guards:
  - the GUIMaker criterion benches and alloc tests;
  - the engine micro benches touched by the phase;
  - the GPU timings from the pixel tests.

  Report them against the previous phase.
- Each phase records the release-ready engine pin in the report, so a later
  bisect has every phase boundary.
- One **perf sweep** runs before Phase 9B, or earlier when the user says the
  host is free:
  - the idle sample: total under 8 %, no core above 25 %, sampled with
    psutil only;
  - every scenario three times, with `--check` on the fresh JSON;
  - any budget miss is bisected across the recorded phase pins.

Phase 5's fresh check is the first item of that sweep.


## Review findings after Phase 6 (reviewer, 2026-11-22)

**Not accepted. Nothing is committed.** Keep the working tree and fix it in
place. Lane 0 (the web rounded clip) is good. The breadth of the component
set is good. But the shots show real bugs, the report calls several fakes
"documented approximations", and Phase 6 is titled "each one finished, not
sketched". The reviewer opened `app-shell`, `dialog`, `text-field`, `table`,
`menu`, `icons` and `button`.

**Rendering bugs (engine or backend):**
1. **`dialog`:** the elevation shadow shows visible concentric bands, and
   the dialog body shows grey smears. The backdrop blur seems to sample the
   dialog's own shadow, or the target it is drawing into. Shadows must be
   smooth: use dithering or a proper Gaussian, and sample only the layers
   below the dialog. Add a pixel test: no step between neighbouring pixels
   bigger than 2/255 across a shadow ramp, and a uniform interior.

**Component bugs:**
2. **`menu` and `context_menu`:**
   - The open submenu is laid out inline. It pushes the parent rows down,
     which leaves a gap under "Export". A submenu is an overlay anchored to
     its row.
   - The highlighted row ("Snapping") does not span the menu width.
   - The shortcut column is not a column. Shortcuts follow the label;
     align them right in one column.
   - The submenu affordance is a large play triangle; use a chevron sized
     to the text.
3. **`table`:**
   - The rows are separate cards with gaps between them, not a table.
   - The columns are sized per row, not shared.
   - The first column uses a different type size.
   - The sort indicator is a large solid triangle.

   Use shared column widths (fixed, fraction or fit), one row surface with
   hairline separators or zebra striping, the same type role in every cell,
   and a small chevron for the sort indicator.
4. **`text_field`:** the caret is a `|` character, and the focused field
   shows none. Draw a real caret rect at the `TextEditor` position, taken
   from the shaped glyph positions, and real selection rects. Blinking can
   wait for Phase 7.
5. **`app_shell`:**
   - The toolbar has a second bordered box inside the title bar.
   - The inspector `form_row` breaks "Inspector / Name" over two lines next
     to a squashed field.
6. **Icons:**
   - "Back" draws → and "Forward" draws ←.
   - "Remove" appears twice, with two different glyphs.
   - The icons render pure black, ignoring the text tone. An icon must take
     its colour from the tone of its context, the way text does.
   - Icon sizes are not tied to the type role: the X in the fields and the
     dialog is about twice the text size, while the settings icon is tiny.
     Set one icon size per type role.
   - Several glyphs are crude or broken at 1.5×: More, Warning, Pin,
     Hidden, Snapping, Mask. Redraw them on the 16-unit grid at a
     consistent stroke weight.
7. **`button`:** the sizes change only the padding. Small, Medium and Large
   must also step the type role and the icon size.

**Fakes listed as approximations. Make them real:**
- `progress_ring`: a real arc, built from an engine path stroke (round cap,
  angle from the value).
- Per-side borders: add them as a generic engine feature (border widths per
  side in `GuiQuad` and the shader), then use them in `callout`.
- `avatar_group` overlap: support a negative gap in layout.
- `spinner` may stay static until Phase 7, but the story must say so.

**Process:** The main agent looked at the shots and still wrote these
goldens. Add a written visual QA step: for every story, list the checks
(alignment, a shared baseline, icon colour and size, column alignment,
overlay anchoring, smooth shadows) with pass or fail. Fix every fail before
writing goldens. Then add the mechanical guards to the gallery tests:
- no text or icon may use pure black or pure white; colours come from tones;
- the icon size must match the type role of the sibling text;
- the cells of a table column must share one x and one width;
- a submenu must not change its parent's layout.


## Scope split: backend for opencode, visual design for Claude (user decision, 2026-11-22)

The user decided that opencode writes the backend and the long code: the
engine, the layout mechanics, the component behaviour and the APIs. Claude
does the visual design later, in the frontend session: the look of every
component, the icon artwork, spacing and proportions, and the final goldens.

That changes the Phase 6 findings, and Phases 7 to 11:

**Opencode fixes these (mechanics and rendering):**
- the shadow banding and the backdrop blur sampling its own shadow or target
  (the engine), with pixel tests;
- per-side border widths and a round-cap arc stroke, as generic engine
  features with pixel tests;
- submenus as anchored overlays that never change the parent's layout;
  shortcut column alignment as a layout mechanism;
- table shared column widths (fixed, fraction, fit) as a layout mechanism;
- a real caret rect and selection rects from the shaped glyph positions;
- a negative gap in layout;
- icons take their colour from their context tone, and their size comes
  from a theme token per type role;
- the swapped Back and Forward icons and the duplicate Remove (data bugs);
- the mechanical guards in the gallery tests (no pure black or white, icon
  size from the role token, one x and width per table column, a submenu
  never changes its parent, the shadow ramp step at or under 2/255).

**Claude does these later (visual):**
- the icon redraws, the menu, table, toolbar, app_shell and form_row look;
- the button size steps;
- the visual QA checklist;
- the final goldens.

Opencode's goldens are regression snapshots only. Regenerate them after the
mechanics change, and do not polish their look. Phase 7's "polish pass" is
reduced the same way: opencode builds the transition, reduced-motion,
scrollbar and focus-ring machinery, and Claude tunes how they look.
