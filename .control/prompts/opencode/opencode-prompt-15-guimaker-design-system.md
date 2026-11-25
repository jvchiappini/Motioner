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

## Phase 8B: popups outside the window, font fallback, live DPI change

Added by the user on 2026-11-22. This is backend only: the reviewer does the
look later. It follows the ownership split. The engine owns windows, the OS
and the drawing. GUIMaker decides where an overlay goes and which window it
lives in. Each of the three items is generic (no Motioner names) and gets
regression tests.

1. **Overlays that leave the main window** (menus, submenus, context menus,
   tooltips, combo/select dropdowns, popovers).
   - **Engine** (generic, documented in the engine docs):
     - a popup window API on desktop: borderless, no taskbar entry, owned by
       its parent and never stealing activation from it. It can be shown at
       a screen position, resized, hidden and destroyed;
     - per-platform transparency for the rounded corners and shadow
       (Windows first; macOS and X11/Wayland behind the same API, with a
       documented fallback where unsupported);
     - a monitor query: the work area (without the taskbar) and the scale
       factor of the monitor under a screen point.

     The popup shares the parent's GPU device and atlases. It never gets a
     second device, a second glyph atlas or a second icon cache. Input on
     the popup is reported in the popup's own coordinates, tagged with the
     window id.
   - **GUIMaker:**
     - **Placement.** A placement solver, `place(anchor_rect, preferred_side,
       content_size, work_area) -> Placement`, flips to the opposite side,
       then shifts along the edge, then clamps with an internal scroll when
       the content cannot fit. It is a pure function with unit tests for
       every edge, for the corners, for a submenu near the right edge
       (opens to the left) and for a dropdown taller than the screen
       (scrolls).
     - **Window choice.** An overlay that fits inside the main window stays
       in it, so there is no popup window and no extra cost. Only an
       overlay that does not fit is hosted in a popup window. The overlay
       tree, focus, keyboard navigation and dismissal stay one system across
       windows:
       - click outside closes the menu chain;
       - Escape closes one level;
       - focus returns to the opener.
     - **Web (`ferrous_web`) and single-window hosts.** Popups are
       unavailable there, so the solver clamps to the canvas. This is a
       documented capability flag, not a separate code path in the
       components.
   - **Tests:**
     - solver unit tests;
     - a headless test that a menu anchored 20 px from the right edge of the
       window chooses the popup host and keeps the parent layout unchanged;
     - an idle test: an open popup that is not changing renders 0 frames,
       like the main window.
   - **Perf.** Opening and closing a popup is measured: the time to the
     first popup frame and the extra RSS. Hidden popups are reused from a
     small pool rather than recreated.

2. **Font fallback** (emoji, CJK, Arabic/Hebrew glyph coverage, symbols).
   - **Engine** (`ferrous_font` / the text system, generic):
     - a fallback chain per font family;
     - per-glyph resolution during shaping: a run is split where the primary
       font lacks the glyph and continues in the first font that has it;
     - the system fonts as the last resort, through an OS font lookup
       (DirectWrite on Windows, CoreText on macOS, fontconfig on Linux) or
       a font-database crate (decision record first, with size and licence);
     - colour emoji (COLR/CBDT or bitmap) drawn into an RGBA atlas page next
       to the coverage atlas.

     Fallback fonts are loaded lazily on first use and never at startup, so
     cold-start must not regress. The atlas eviction from Phase 9B applies to
     them.
   - **GUIMaker.** The theme can name the fallback chain per type role. Text
     measurement, the caret and selection rects, and ellipsis all use the
     resolved runs.
   - **Tests:**
     - mixed strings ("Intro 🎬 動画 تجربة ✓") measure and draw without tofu
       (no `.notdef` glyph in the shaped output);
     - caret positions step over a multi-codepoint emoji as one grapheme;
     - a startup test proves no fallback font is opened when the text
       doesn't need it.
   - **Out of scope.** Bidirectional layout and RTL caret logic stay in the
     backlog. This item only guarantees that the glyphs exist.

3. **Live DPI and scale change.**
   - **Engine:**
     - forward `ScaleFactorChanged` and per-monitor DPI (Windows
       per-monitor v2 awareness in the manifest/runner) as one event with
       the new scale and the suggested window size;
     - re-rasterize the glyph atlas and the icon cache for the new scale
       (the old scale's entries are evicted, never kept);
     - resize the surface without a black frame.
   - **GUIMaker.** On a scale change:
     - re-run layout in logical units;
     - snap hairlines and borders to the new device pixels;
     - re-resolve icon sizes;
     - keep scroll offsets, focus, text selection and open overlays (an
       overlay in a popup window moves with the monitor change of its
       parent, or closes if its anchor leaves the screen).

     A window spanning two monitors uses the scale the OS reports for it.
   - **Tests:**
     - a headless test renders a story at 1.0, then switches to 1.5 and 2.0
       on the same `GuiHost`; it asserts crisp hairlines (the Phase 6 guard),
       the icon size equal to the role token times the scale, and unchanged
       scroll offset, focus and selection;
     - the atlas byte count after the switch is not larger than after a
       fresh start at that scale.
   - **Manual check, written in the report:** drag the gallery between two
     monitors with different scales if the host has them; otherwise say so.

**Acceptance:**
- every test above is green;
- the strict pedantic gate covers every new or changed crate, including the
  engine files;
- `cargo check --workspace --all-targets` and the wasm32 checks show no new
  warnings;
- the host-independent perf guards (divan, alloc, GPU timings) show no
  regression against the previous phase;
- the goldens are regenerated only where the output really changed, as
  regression snapshots.

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

## Review after the Phase 6 fix (reviewer, 2026-11-22)

The Phase 6 mechanics are accepted, and the engine part is pushed once the
reviewer's gate is green. These are now correct:
- the anchored submenu, with 0 px change to the parent;
- the shortcut column;
- the shared table tracks;
- the caret and selection rects;
- the per-side borders, the round-cap arc and the negative gap;
- icon tone inheritance and the icon size from the role token;
- the shadow ordering and dither.

Fix these first in the next run, before Phase 7:

1. **The backdrop blur samples nothing.** `shot.rs:158` clears the target
   and hands it over as the backdrop source, so the blur reads a black
   clear. The area behind the dialog is flat black in
   `out/shots/p6/dialog-*.png`. The guard passes, but the effect is not
   verified, which makes it the same kind of fake as in Phase 5.
   - The host must first render the layers below the overlay into the
     scene target, then set that target as the backdrop source, then draw
     the overlay.
   - The dialog story must put real content behind the scrim, for example
     a card grid or the start-screen recipe.
   - Add a pixel test: the region behind the dialog must not be flat. It
     must match a CPU Gaussian of the content below within the golden
     tolerance, and the dialog's own pixels never appear in the source.
   - The same applies to every host that uses `BackdropBlurSpec`, not only
     the gallery.
2. **Seams between table cells.** The selected row and the header show 1 to
   2 px vertical seams at every column boundary (`table-dark-*.png`, x ≈ 190
   and 336 at 1.5×). The row background is painted per cell.
   - Paint the row and header background as one rect behind the row's
     cells. The cells carry no background of their own.
   - Add a guard: the selected row's fill is uniform across the full row
     width, apart from the 2 px antialias at the rounded ends.
3. **`cargo machete crates/ferrous_svg` exits 1.** `glam` and `usvg` are
   unused, and have been since 4b94774. The reviewer's Phase 5 gate missed
   the exit code. Remove them, or use them, and add `ferrous_svg` to the
   phase gate's machete list.
4. **Divan against Phase 5** shows several rows slower:
   - click +26 %;
   - focus +19 %;
   - hover frame +23 %;
   - relayout +15 %;
   - layout 8192 +17 %.

   The host is busy, so this may be noise, but medians from two separate
   sessions cannot tell noise from a regression. Run an **interleaved A/B**:
   build the bench at the Phase 5 pin (76d6cc8) and at the current tree,
   then alternate the runs 3 times each in the same session and compare
   the medians. A row that stays more than 10 % slower in all 3 pairs is a
   regression: find it and fix it. Otherwise record it as noise, with the
   numbers.
5. **Line endings.** `.control/sessions/S-2026-11-21-001.md` was written
   with CRLF again. Write it with Python, using `newline="
"`.

These are visual, not mechanical, and stay with Claude: the icon artwork
(Clear and Delete share a glyph, the Keyframed glyph), the size of the sort
arrow in the table header, and the menu's check glyph.

## Review after the Phase 6 follow-ups and Phase 7 (reviewer, 2026-11-22)

**Accepted:**
- one background rect per table row (no seams, checked in the shot);
- `ferrous_svg` machete;
- the interleaved A/B method;
- the line endings;
- the Phase 7 machinery: transitions on the injected clock, `EnterExit`,
  `prefers_reduced_motion`, smooth scroll with an overlay thumb, the
  focus-visible ring and the caret blink, with idle at 0 frames and one wake
  per blink phase.

The reviewer commits the engine work as it stands, after the gate, with
these open items:

1. **Work went into the wrong engine tree again.** All engine edits were
   made in Motioner's submodule `engine/ferrous`, not in the dev clone
   `C:\Users\jvchi\CARPETAS\FerrousEngine`. The reviewer copied the 28 real
   files across by hand.

   The submodule also showed about 80 files as modified by line endings
   alone (CRLF churn on every file that was touched).

   Rules from now on:
   - edit engine code only in the dev clone;
   - the submodule is read-only, and moves only through the reviewer's pin
     bump;
   - lane worktrees are made from the dev clone, under
     `C:\Users\jvchi\CARPETAS\FerrousEngine\out\worktrees\`.
2. **Leftover worktrees.** Six worktrees are still registered on the
   submodule: `out/worktrees/lane-a`, `lane-b-cur`, `lane-b-p5`, `lane-c`,
   `lane-d` and `motioner-gate`.
   - Run `dir /AL /S /B` on each and report the output; it must be empty.
   - Then `git worktree remove --force` each one, and `git worktree prune`.
   - From now on, remove your worktrees at the end of the run.
3. **The backdrop blur samples the wrong texture.**
   - **Symptom.** In `out/ui/p15-p6c-dialog-dark.png`, the dialog interior
     shows rows of blurred glyphs repeated about every 36 px. They appear
     below the field, behind the buttons and at the right edge of the card,
     where the content below is plain coloured panels with no text.
   - **Likely cause.** This looks like the glyph atlas, or another bound
     texture, sampled instead of, or mixed into, the blurred backdrop.
     Another possibility is a UV packing clash with the `PER_SIDE_BORDER_BIT`
     uv0/uv1 layout.
   - **Why the test missed it.** `the_dialog_card_blurs_the_content_below`
     only compares the padding strip.
   - **Fix and guard.**
     - Find the root cause with file:line.
     - Extend the test: compare **every** pixel of the card interior that
       is not covered by a child, not only the padding strip.
     - Add a negative control: the same story with the glyph atlas cleared
       to a solid colour must give identical backdrop pixels.
4. **Divan regressions not covered by the A/B.** The A/B compared eef1e58
   with 76d6cc8, but in the integrated Phase 7 tree:
   - `start_screen_relayout_frame` is 88.8 µs, 2× the Phase 6 44.47 µs;
   - `focus_step_10000` is 192.5 ns, against 107.6 ns.

   Run the same interleaved A/B on the new tree against eef1e58, 3 pairs.
   A row that stays more than 10 % slower in all 3 pairs is a regression:
   profile it and fix it. The likely suspects are the per-frame work of the
   animator, scroll smoothing or the focus-ring slot on every relayout.

## Review after Phase 8 (reviewer, 2026-11-23)

The engine part is pushed as `dacd67d`.

**Accepted:**
- engine work in the dev clone only, with the submodule left clean;
- the leftover worktrees removed after the `dir /AL` check;
- the A/B against eef1e58: no regression, and the 88.8 µs figure was a
  noisy single run;
- Phase 8: the lint, its 18 rules each with a positive unit test,
  `assert_clean`, the sweep's first-width reporting, the CLI and the F12
  overlay. The real mechanics were fixed rather than the rules silenced.

**Open, to fix before Phase 8B: the backdrop blur fix hid the bug instead
of fixing it.**
- The diagnosis was right: the blurred-glyph rows are the background's
  own text, repeated at the tap spacing (about 36 px = 24 logical × 1.5).
- The response was wrong: the text was removed from `dialog_story`, and the
  undersampling was called a look change that is out of scope.
- A blur that turns one line of text into several sharp copies is a
  rendering bug, not a look choice. It will show in every real use: a
  Motioner dialog sits over panels full of labels.

The fix:
1. **A properly sampled blur in the engine.** Keep the cost at or under
   the current 0.4 ms per frame.
   - The source is prefiltered down to the blur resolution: progressive 2×
     box downsamples, or a 13-tap downsample.
   - Then a separable Gaussian with taps at most 1 texel apart at that
     level, or dual Kawase.
   - Record the choice in the engine docs, with the GPU timing.
2. **Restore text behind the dialog in `dialog_story`:** the titles and the
   body lines on the panels, as before.
3. **An impulse-response test** (CPU reference, plus a GPU pixel test).
   - Draw a single bright 1 px horizontal line, then a 1 px vertical line,
     behind a backdrop card.
   - The blurred profile across the line must be unimodal: one peak,
     falling monotonically on both sides within ±1/255 of quantisation
     noise. There must be no secondary peaks, which are the copies.
   - Its width must match the requested radius within 20 %.
4. Keep the full-interior comparison against the CPU Gaussian and the
   solid-atlas negative control. Update the CPU reference to the new
   kernel.

## Review after Phase 8B, first pass (reviewer, 2026-11-23)

The engine part is pushed as `a3fdec8`, and the mojibake repair as `e008764`.

**Accepted:**
- **The backdrop blur:**
  - a 2×2 box prefilter chain, then a 1-texel Gaussian;
  - the impulse response is unimodal, with FWHM = radius;
  - 0.059 ms per frame;
  - the text is back behind the dialog, and the shot shows a smooth blur;
- the placement solver, `OverlayHost` and the unified dismissal;
- font fallback measurement and run splitting, with the lazy OS scan and
  G-0005;
- the scale-change event.

**Reviewer fixes in a3fdec8 and e008764** (do not undo them):
1. **The default host capability hid menus.**
   `HostCapabilities::default()` was `desktop()`, popups on, while the
   engine has no popup window. Every overlay that did not fit was therefore
   neither painted in the window nor anywhere else
   (`out/shots/p8b/menu-popup-near-edge.png` shows only the anchor).
   - The default is now `single_window()`.
   - The edge test sets `desktop()` explicitly.
   - A new test, `the_default_host_keeps_an_overflowing_menu_visible_in_the_window`,
     covers the default.
   - A host opts into popups only once it really draws them.
2. `examples/p8b_shots.rs` was not rustfmt-clean. The phase gate ran fmt
   only on `-p guimaker`.
3. The new doc for `on_scale_factor_changed` was spliced into the middle
   of `on_resize`'s doc comment (`traits.rs`), which left `on_resize`
   undocumented. It is moved back.
4. **Mojibake.** `guimaker/src/lib.rs`, `view.rs` and the blur test's "±"
   were written double-encoded this phase. 14 more engine files carried
   older mojibake (mkdocs.yml since P4B, and several docs). All are
   repaired. **Add `scripts/check_encoding.py` to the engine**, with the
   same mojibake detection as Motioner's, and put it in the engine gate.

**Still open from Phase 8B:**
- **A. Popup windows.** The runner is single-window and the app gets no
  `ActiveEventLoop`, and that is exactly the engine work this lane asks
  for.
  - Make the runner own a window map: the main window plus popups, sharing
    one device.
  - Route events by window id.
  - Give the app a popup handle API.
  - Headless tests cover the GUIMaker side, which is done. For the engine
    side, add a runner unit test with a mock window map, and a manual check
    written in the report (open the gallery, open a menu at the right edge,
    attach a screenshot).
  - Only then does the gallery host switch to `desktop()`.
- **C. Fallback glyphs are measured but not drawn.** The shot
  `mixed-scripts-*.png` shows only "Intro".
  - Draw multi-page atlases (one bind group per page, or an array
    texture).
  - Add colour emoji in an RGBA page.
  - Add a pixel test that the CJK and emoji glyphs cover pixels.
- **D. Live DPI change.** Still missing:
  - re-rasterizing the atlas and icons per scale, with eviction;
  - the 1.0 → 1.5 → 2.0 test on one host.
- **Guards not run.** The divan and alloc runs were skipped, and the
  Motioner gate ran in the live tree, not a clean worktree (the report also
  named a stale pin). Both are required every phase.

### Review after Phase 8B, second pass

**Accepted and pushed** (engine `fe8b5fb`, `46b66cb`, `5aa1aec`). The
reviewer fixed two things: a module order that was not rustfmt-clean in
`ferrous_engine/src/lib.rs`, and an unused variable in the `system.rs` test.
New engine code gets `rustfmt --check --config skip_children=true` on every
changed file.

**Accepted:**
- **D.** The engine encoding guard (`scripts/check_encoding.py` with its
  tests and its CI step), plus 15 stray BOMs removed.
- **B, in part.** `BakedAtlas::build_multi` and `Font::load_multi`. The
  screenshot harness now draws CJK, Arabic and symbol glyphs, and
  `tests/fallback_draw.rs` checks that they cover pixels.
- **A, skeleton only.** `windows.rs` (`WindowMap`, `PopupBackend`, pooling,
  stable ids, routing) and `on_popup_event`.
- **C, in part.** The scale test that checks scroll, focus, selection and
  overlays are kept across `1.0 → 1.5 → 2.0`.

**Findings:**
1. **Fallback is drawn only in the screenshot harness.** The runner still
   bakes `latin_western` at startup (`runner/events.rs:275`) and nothing
   adds glyphs later, so in a real app `動` still draws `.notdef`. Required:
   - the runtime atlas resolves missing characters lazily, the first time a
     text run uses them (`system::find_for`, then re-bake or extend, then
     re-bind the atlas);
   - a test through the host or runner frame path, not the screenshot
     harness;
   - ASCII-only apps still load no fallback face.
2. **`build_multi` mixes units.** Each fallback face's advances and bounds
   are in that face's own `units_per_em`, but layout scales every glyph by
   the first face's `units_per_em` and ascent. It works only while both
   faces happen to use 2048. Required:
   - normalize every face's metrics into the primary face's units, or keep
     a per-glyph scale;
   - a test with a face whose `units_per_em` differs, asserting the advance
     in em.
3. **The Arabic test string is pre-reversed, and nothing is shaped.**
   - `\u{0629}\u{0628}\u{0631}\u{062C}\u{062A}` is "ةبرجت", which is
     "تجربة" typed backwards so that left-to-right drawing looks right.
   - The letters are drawn in their isolated forms.
   - There is no bidi reordering and no Arabic joining.

   Required: minimal UAX #9 run reordering (paragraph level, with RTL runs
   reversed for display while the caret and selection stay in logical
   order), plus Arabic joining through GSUB `init`/`medi`/`fina`/`isol`. The
   test must use the logical-order string. If either part is deferred,
   write a decision note and make the test assert the gap, not hide it.
4. **Re-rasterizing the atlas per DPI scale is the wrong fix, and the
   reviewer's spec asked for it.** MSDF does not depend on resolution. At
   2.0 every glyph is drawn larger, so the screen-space pixel range grows
   and edges get sharper, not softer. Baking at 128 texels quadruples the
   atlas (5.3 → 21.2 MB) with no measured gain. The test
   `bytes <= fresh_bytes` is tautological, because both sides call the same
   function. Required:
   - a pixel comparison at 2.0 for Caption and Body text: a 64-texel cell
     against a 128-texel cell, measuring the edge error against a
     supersampled reference;
   - if 128 is not measurably better, the cell size does not grow with DPI.
     It grows only when the drawn em is larger than a documented multiple
     of the cell. Remove or reshape `ScaledAtlases` to match;
   - the same check for the icon cache.
5. **Popups are still not real.**
   - `Runner.windows` is always `None`, so the routing never runs.
   - `deliver_popup_event` duplicates the whole `AppContext` construction.
     Factor it into one helper.
   - The real backend has a standard shape in winit 0.30, and "the runner
     does not keep the `ActiveEventLoop`" is not a blocker:
     - the app queues popup commands through `ctx`, getting a `PopupId`
       back at once;
     - the runner drains the queue in the next callback that has
       `&ActiveEventLoop` (`about_to_wait` or `window_event`);
     - each popup gets its own surface on the shared device.
   - The screenshot of the popup outside the window is still required.
6. **Colour emoji** in an RGBA page is still missing. The clapper board is
   drawn as a monochrome glyph.
7. **Gates.**
   - The Motioner gate skipped the clean worktree a second time.
     Initializing the submodule in a worktree is part of the procedure, not
     a reason to skip it.
   - `every_formatted_example_is_lint_clean` failed once with
     `examples directory: NotFound`. The path is absolute, built from
     `CARGO_MANIFEST_DIR` (`crates/motioner_dsl/tests/lint.rs:25`), and the
     test passes on every reviewer run. So `examples/` was really missing
     at that moment, which means something else in the run (a subagent, a
     script or a checkout) was touching the Motioner tree. Find out what it
     was, and say so in the report.

### Review after Phase 8B, third pass

**Accepted and pushed** (engine `e426d17`):
- **C. The MSDF cell does not grow with the DPI scale.** `cell_for_em`
  replaces `ScaledAtlases`, decision G-0006. The atlas is 5.31 MB at every
  scale.
- **B2. `units_per_em`.** `FontParser` already normalizes every face into
  em (G-0007), and a test with Dubai (`units_per_em` 1000) proves it. The
  reviewer's finding 2 of the second pass was wrong.
- **D.** Investigated, and not reproducible.

**Findings:**
1. **G-0006 does not compare against a supersampled reference.**
   - `BakedAtlas::build_with_size` clamps the cell to 8..128, so the
     "512-texel reference" in `tests/msdf_dpi.rs:17` is really a 128-texel
     bake. That is why the 128-texel cell shows exactly 0.00000 error.
   - The table measures a 64-texel cell against a 128-texel cell, and only
     for `H`, which has no curves.
   - The decision probably stands, but the numbers need fixing:
     - build the reference without the clamp (call `generate_msdf`
       directly) or from analytic coverage of the outline;
     - add curved and thin glyphs (`a`, `e`, `g`, `s`, `動`);
     - correct the table and the doc comment in `scaled.rs`.
2. **The Arabic gap test pins the hack, not the gap.**
   `arabic_is_drawn_in_isolated_visual_order` still uses the reversed
   string and asserts "drawn in the given order". Write it with the
   logical "تجربة", and make it assert that the drawn glyph order is the
   logical order and that no joined form appears. That is the real gap, and
   the test will break the day bidi and joining land.
3. **Deferred for the third time: real popups (A).** Also deferred: runtime
   fallback (B1), Arabic bidi and joining (B3), and colour emoji (B4).
   These are now their own phase, **Phase 8C**, with A and B1 first. Don't
   take on more lanes than you can deliver. Two lanes delivered end to end
   are worth more than five that come back deferred.
4. **The Motioner gate in a worktree ran only part of the gate**: fmt,
   check, encoding, validate, refs and one test. The full gate means
   clippy `-D warnings`, the whole test suite, doc, boundaries, machete,
   pytest, validate and refs.

### Phase 8C: real popups and runtime fallback (two lanes only)

**A. Real popup windows**, the engine side of Phase 8B lane A:
- the app queues commands through `ctx` (`ctx.popups()`, open, move,
  resize, hide, close) and gets a `PopupId` back at once;
- the runner drains the queue in the next callback that has
  `&ActiveEventLoop` (`about_to_wait`, or the top of `window_event`), and
  creates owned, borderless windows that do not steal activation and are
  not shown on the taskbar;
- `WindowMap` is built in `resumed`, so routing is live;
- each popup gets its own `wgpu::Surface` on the shared device, glyph
  atlas and icon cache, and a popup that is not dirty renders 0 frames;
- one helper builds `AppContext`, used by both the main path and the
  popup path;
- `GuiHost` paints popup-hosted overlays into the popup. The gallery
  switches to `desktop()` only after that works;
- Windows first. Other platforms degrade to `single_window()` through
  `HostCapabilities`, with that documented;
- tests:
  - a runner unit test with the mock backend covering the queue;
  - an integration check that opens the gallery, opens a menu at the
    right edge and takes an OS-level screenshot (the
    `screenshot`/PrintWindow path or a desktop capture). Attach it;
- measure the time to first frame for a new popup and for a pooled one,
  plus the extra RSS and VRAM per popup.

**B1. Runtime lazy fallback:**
- `ctx.request_glyphs(chars)`, or automatic detection in the GUI text
  path, collects characters missing from the atlas;
- before the next draw the runner re-bakes the composite atlas
  (`build_multi` with `system::find_for`) and re-binds it with
  `GuiRenderer::set_font_atlas`;
- glyphs already in the atlas keep their UVs, or the whole frame re-lays
  out in the same frame, so nothing draws half-updated;
- the atlas cache keys stay stable;
- an ASCII-only app loads no fallback face and does not scan the OS fonts;
- a test goes through the runner and host frame path, not the screenshot
  harness. A frame with "動画" draws ink at the CJK glyphs' rectangles, and
  the second frame with the same text causes no re-bake;
- measure the atlas bytes and the re-bake time before and after.

B3 (bidi and Arabic joining) and B4 (colour emoji) move to Phase 9B, with
the growing atlas and RGBA pages.

### Review after Phase 8C

**Accepted and pushed** (engine `b37b26d`):
- **B1, runtime lazy fallback** (`runtime_font.rs`, `AppContext::request_glyphs`,
  the re-bake in `runner/frame.rs` before `draw_ui`).
- **The corrected G-0006.** The reference is now an unclamped 512-texel
  `generate_msdf` bake, measured on H, a, e, g, s and 動. The largest mean
  gain from a 128-texel cell is 2.67 %.
- **The Arabic gap test**, which now uses the logical string.
- The report was honest that lane A was not done. Thank you.

**Reviewer fix: an uncovered character kept the app awake forever.**
- The problem: when no face covers a character (an unassigned code point, or
  a rare symbol), `ensure` added it to `chars` without baking it. `has` stayed
  false, so every frame did the same thing again: requested it, re-scanned the
  OS fonts, re-baked the whole atlas (260 ms in debug) and set
  `redraw_request`. The app never reached 0 idle frames.
- The fix: `RuntimeFont` now keeps an `unresolved` set. `has` treats those
  characters as handled, and they draw `.notdef`. `ensure` re-bakes only when
  at least one new character is drawable.
- The test: `runtime_fallback.rs` now asserts that U+0378 never re-bakes.

**Still open, Phase 9B:**
- The re-bake is synchronous on the frame thread, about 260 ms in debug. That
  is a visible hitch. Move it to the startup bake worker: draw `.notdef` until
  the worker finishes, then swap the atlas and redraw once.
- The re-bake rebuilds the whole atlas. The growing atlas in 9B adds only the
  new cells.
- `runtime_fallback.rs` calls `RuntimeFont` directly. It does not go through
  `Runner` or `GuiHost`. Add a runner-level test when the runner gets a
  headless harness.
- `AppContext` is now built by hand in 7 places. Factor it into one helper:
  that is step 1 of Phase 8D.

### Phase 8D: real popup windows (one lane, step by step)

Lane A failed four times, so this phase gives the plan step by step. Do
every step. If a step turns out to be impossible, stop there, and report the
exact compiler or OS error with file:line.

1. **One `AppContext` builder.** Replace the 7 hand-built `AppContext { .. }`
   literals in `runner/events.rs` and `runner/frame.rs` with one
   `Runner::with_ctx(|app, ctx| ..)` helper. Add the field
   `popups: &mut PopupQueue` to `AppContext`.
2. **The command queue.** `PopupQueue` lives in `windows.rs`. It records
   `Open(spec) -> PopupId` (the id is reserved at once), `Move`, `Resize`,
   `Show`, `Hide` and `Close`. The app calls `ctx.popups.open(spec)`.
3. **The winit backend.** Add `WinitPopups`, which implements the real side
   of `PopupBackend`. For each popup it holds an `Arc<Window>`, a
   `wgpu::Surface<'static>` and its `SurfaceConfiguration`.
   - Create the window with:
     - `with_decorations(false)`, `with_visible(false)`,
       `with_transparent(true)`, `with_resizable(false)` and
       `with_active(false)`;
     - on Windows, through `winit::platform::windows::WindowAttributesExtWindows`:
       `with_owner_window(main_hwnd)` and `with_skip_taskbar(true)`.
   - Create the surface the same way `graphics.rs:669` does, on the shared
     instance and device.
   - Use alpha mode `PreMultiplied` when the surface supports it, and
     otherwise `Opaque`. In the opaque case the popup paints its own
     background, which is the documented fallback.
4. **Draining the queue.** In `about_to_wait_impl` (`runner/frame.rs:718`),
   and only there, because it has `&ActiveEventLoop`, the runner drains the
   queue into `WinitPopups`. Build `WindowMap` in `resumed`
   (`runner/events.rs:176`) from the main window id, so `route` is live.
   Hidden popups are pooled.
5. **Drawing.** Add the trait method
   `FerrousApp::draw_popup(&mut self, popup: PopupId, dc: &mut DrawContext)`,
   which does nothing by default.
   - When `RedrawRequested` arrives for a popup id, the runner:
     1. acquires that surface's texture;
     2. runs one `GuiRenderer` per popup, which shares the glyph atlas view
        and the icon atlas but owns its own instance buffers;
     3. calls `draw_popup`, then prepares, renders and presents.
   - An idle popup renders 0 frames, because it follows the same
     redraw-request rules as the main window.
6. **`GuiHost`.**
   - When the overlay solver returns `Placement::Popup`, the host:
     1. opens or reuses a popup at the solver's screen rectangle;
     2. encodes that overlay's display-list range into the popup's batch,
        offset to the popup origin;
     3. leaves the overlay out of the main batch.
   - Escape, outside clicks and the menu's own clicks arrive through
     `on_popup_event` and go into the same GUIMaker input path, translated
     to window coordinates. Closing the overlay hides the popup.
   - Only after this does the gallery switch to `desktop()`.
7. **Tests and evidence.**
   - A runner unit test: the queue is drained by the mock backend, ids stay
     stable, and pooling works.
   - The gallery example `p8d_popup`: it opens a menu at the right edge,
     waits two frames, then saves a **desktop** capture (PowerShell
     `[System.Drawing.Graphics]::CopyFromScreen` over the window's bounds
     plus 300 px). Attach the PNG, with the menu visible outside the window.
   - Numbers:
     - the time to first frame for a new popup and for a pooled popup;
     - the extra RSS and VRAM per popup;
     - the CPU and frame count of an idle popup over 5 s, which must be 0
       frames.

### Review after Phase 8D

**Accepted and pushed** (engine `85f3daa`). All three milestones were delivered,
and the popups are finally real:
- **`Runner::with_ctx`** replaces the 7 hand-built `AppContext` literals.
- **`PopupQueue`**, including `Redraw`.
- **`WinitPopups`**: owned, borderless, not on the taskbar, inactive at creation,
  on the shared device, with `PreMultiplied` alpha and an `Opaque` fallback.
- The queue drains in `about_to_wait`, and `WindowMap` is built in `resumed`.
- **Pooling.** A popup that was just opened cannot be stolen.
- **A per-popup `GuiRenderer`** that shares the glyph atlas and rebinds only when
  the atlas `Arc` changes.
- **`draw_popup`**, plus `GuiHost::sync_popups` and `on_popup_event`.
- **The `p8d_popup` desktop capture** shows the menu outside the window.
- **The numbers:**
  - the idle popup rendered 0 frames;
  - VRAM is about 1 MiB per popup.
- **The full Motioner gate ran in a clean worktree.** Thank you.

**Reviewer fix:** in `runner/popups.rs`, the dead `let _ = gfx;` binding was
replaced with a plain `is_none()` guard.

**Findings:**
1. **The popup is not clamped to the monitor.**
   - `layout.rs:803` uses the window viewport as the work area. The
     popup-hosted rect is `placement.preferred`, which is unclamped. Nothing
     clamps it to the monitor.
   - In `popup-outside.png` the menu runs past the right edge of the screen.
   - A popup must stay inside the work area of the monitor it opens on, and
     that work area excludes the taskbar.
2. **No real input was ever sent.**
   - `p8d_popup` opens the menu by setting `self.open = true`.
   - Clicking an item, Escape, an outside click and an app switch were never
     driven through the OS.
   - Clicking a normal top-level window activates it. A click on the popup
     therefore deactivates the main window: its title bar greys out, and the
     keyboard moves to the popup. Real menus use `WS_EX_NOACTIVATE`.
   - A menu must also close when the app loses activation. Today
     `FocusLost` closes nothing.
3. **The pool has no cap.** Every popup-hosted node keeps its hidden window
   forever, at about 50 MB of RSS each.
4. **A first popup is heavy: +50.9 MB RSS and 83 ms.** This is far from the
   "lightest UI library" target. Nobody knows yet where the 50 MB go: the
   DXGI swapchain, pipelines compiled again for each `GuiRenderer`, the driver,
   or something else. Measure first, then cut.
5. **Popups repaint whenever the main window repaints.** `repaint` is the main
   window's `repainted` flag, so hovering anywhere in the main window redraws
   every visible popup even when its display-list range did not change.
6. **The popups miss window move, minimize and per-monitor DPI.**
   - Moving the main window while a menu is open leaves the popup where it
     was until some other frame runs.
   - Minimizing does not hide the popups.
   - `render_popup` uses the main window's `scale_factor`, which is wrong when
     the popup sits on a monitor with a different DPI.
7. **The desktop capture is a privacy problem.** `CopyFromScreen` copies
   whatever is behind the window: in this run, another app's private content.
   The file is git-ignored, but evidence must never capture third-party
   windows. Capture only our own surfaces.
8. **The popup memory is not in `memory-report`.** The 1 MiB per popup is
   computed only by the example.
9. **Time to first frame is measured from window creation**, not from the
   input that opened the menu. The user sees input-to-present, so measure
   that.

### Phase 8E: popup correctness and cost

The order below is required. Use subagents for the lanes marked parallel. The
main agent runs the single final gate.

**Lane A: correctness (main agent; touches the runner and windows.rs)**
1. **Monitor work area.**
   - Add `AppContext::work_area()`: the work area of the monitor the main
     window is on, in window-local logical coordinates.
   - On Windows, use `MonitorFromWindow` and `GetMonitorInfoW` (`rcWork`) through
     `windows-sys`, which `ferrous_engine` already depends on (add the
     `Win32_Graphics_Gdi` feature if it is missing).
   - Elsewhere, use winit's `current_monitor()` position and size.
   - Pass it to the layout as `HostCapabilities` or a new `set_work_area`. A
     popup-hosted overlay is solved and clamped against that rect: flip,
     shift, then internal scroll. The window viewport stays the work area
     only for in-window overlays.
2. **No activation.**
   - After creation, set `WS_EX_NOACTIVATE` on the popup HWND
     (`SetWindowLongPtrW`).
   - The main window stays the foreground window and keeps the keyboard.
     Escape reaches the main window and still closes the menu.
3. **Dismiss rules.** When the main window gets `Focused(false)` and the newly
   focused window is not one of our popups, close every dismissable overlay:
   menus, selects and popovers. Dialogs stay.
4. **Window lifecycle.**
   - `WindowEvent::Moved` on the main window re-syncs the popup positions in
     the same callback (queue `Move` and request a frame).
   - Minimizing hides the popups. Restoring shows them again if the overlay is
     still open.
   - Each popup uses its own window's `scale_factor()`, and
     `ScaleFactorChanged` on a popup reconfigures it.
5. **Pool cap.** Keep at most 2 hidden popups. `hide` beyond the cap destroys
   the window. Put it in one constant with a comment.
6. **Popup dirtiness.** Redraw a popup only when the hash of its encoded
   command range, its size or its position changed, or when an animation
   inside it is running. Add a unit test: hovering in the main window
   produces 0 popup frames.

**Lane B: cost (parallel subagent; read and measure first, then edit only
`gui_renderer.rs` and `windows.rs` backend creation)**
1. **Break the +50.9 MB and 83 ms down**, step by step, in a release build:
   1. `create_window` alone;
   2. plus `create_surface` and `configure`;
   3. plus `GuiRenderer::new`;
   4. plus the first `prepare`, `render` and `present`.

   Write the table.
2. **Share what can be shared.** Add `GuiRenderer::new_sharing(&main)`, which
   reuses the main renderer's shader modules, pipeline layouts, bind group
   layouts, pipelines and samplers, so a popup creates only its own buffers
   and bind groups. Use `desired_maximum_frame_latency: 1` and the smallest
   valid swapchain image count.
3. **The targets:** a new popup adds at most 8 MB of RSS, and its first frame
   takes at most 20 ms from the input event to present. Where a target cannot
   be met, give the measured reason (for example "the DXGI flip swapchain
   costs X MB on this driver") and what would remove it.
4. **Pre-warming.** Decide, with numbers, whether creating one hidden popup
   during the first idle period after startup is worth its RSS.
5. **Memory report.** Add popup memory to `memory-report` under a `popups`
   subsystem: instances, SVG and swapchain for each popup.

**Lane C: evidence (parallel subagent; the example and the capture only)**
1. **Real input.** Rewrite `p8d_popup` to drive real OS input with
   `SendInput` on Windows:
   - click File, then click Rename. Assert that the app message fired, the
     menu closed and the popup is hidden;
   - reopen, then press Escape: the menu closes;
   - reopen, then click empty space in the main window: the menu closes;
   - reopen, then activate another window of our own test process (a
     small helper window, never a third-party app): the menu closes;
   - after the click on Rename, `GetForegroundWindow()` is still the main
     window.
2. **A privacy-safe capture.** No more `CopyFromScreen`. Read back our own
   surfaces: the main swapchain texture and each popup's. Compose them at
   their screen offsets on a flat neutral background the size of the monitor
   work area. Also save the popup and work-area rects as JSON, and assert
   that the popup rect lies inside the work area.
3. **A right-edge case and a bottom-edge case.** The menu flips or shifts and
   stays fully on screen. Attach both PNGs.
4. **Numbers:** input-to-present for a new popup and for a pooled one, over
   five runs (median and range); RSS and VRAM per popup; the idle popup over
   5 s (0 frames); and 0 popup frames while hovering the main window.

**Gate:**
- the same engine gate as Phase 8D;
- the full Motioner gate in a clean worktree;
- the divan benches compared against `85f3daa`.

Report every finding above as fixed, or as not fixed with the exact reason.

### Review after Phase 8E

**Accepted and pushed** (engine `6f6a745`). All nine findings from Phase 8D
are closed or explained:
- **Monitor work area.** `AppContext::work_area()` uses `rcWork` through
  `GetMonitorInfoW`. In the right-edge and bottom-edge captures the menu flips
  and stays fully on screen, and `popup-rects.json` reports `inside: true`.
- **`WS_EX_NOACTIVATE`** is set on the popups. After a real `SendInput` click on
  Rename, `GetForegroundWindow()` is still the main window.
- **Dismiss on activation loss.** `dismiss_dismissable_overlays` keeps modal
  dialogs open.
- **The window lifecycle:** Moved, minimize and restore, and per-popup DPI.
- **The pool cap:** `POPUP_POOL_CAP = 2`, with a test.
- **Popup dirtiness:** 0 popup frames over 878 frames of hovering the main
  window.
- **A privacy-safe capture:** our own swapchains are read back and composed on a
  neutral background. No third-party window appears.
- **A `popups` subsystem in `memory-report`.**
- **Input-to-present timing.**
- **Honest cost numbers.** The 8 MB and 20 ms targets were missed, but the
  breakdown shows where the cost goes.

**Findings:**
1. **`new_sharing_global` is a process-wide registry keyed by
   `Weak<Device>`.** Global mutable state in the renderer is the wrong shape
   for an engine meant to run many devices and many apps per process. Do the
   one-line fix the report itself proposes: add `UiPass::gui_renderer()` and
   pass `&GuiRenderer` through `PopupContext` into `new_sharing`. Then delete
   the registry.
2. **Pipelines are not shared in Motioner's real configuration.** The main UI
   renders to an `Rgba16Float` MSAA-4 target, while the popups render to sRGB
   without MSAA, so the pipelines are rebuilt. Either:
   - build the popup pipeline variant once per device and cache it, so the
     second popup and later ones share it; or
   - give the popups the same intermediate target.

   Measure both, then pick one.
3. **The new-popup cost is the swapchain**: +11 to 14 MB and 61 to 66 ms in
   `create_surface` and `configure`, and +39 to 52 MB for the first one. The
   pool hides this on reopen, but the first menu of the session still pays
   75 ms, which is a visible stall. Two experiments, both measured against the
   current path:
   - **(a) A layered window without a swapchain.**
     - Render the popup to an offscreen texture on the shared device, read it
       back (a 500×225 menu is about 450 KB) and present it with
       `UpdateLayeredWindow`. This creates no DXGI swapchain, and a menu
       repaints only on hover changes.
     - Measure RSS, the time to first frame, and the per-repaint cost on
       hover.
   - **(b) Backend comparison.** Measure the per-surface cost under DX12 and
     under Vulkan (`WGPU_BACKEND`).

   Keep whichever is lighter as the Windows default, and the other as a
   fallback. Write it down as a decision.
4. **The menu is far wider than its content.** The popup is 500×225 for three
   short items, and an empty band sits under Delete. Size the menu to its
   content: the widest item, plus padding, plus room for the shortcut column,
   at a theme minimum width. The rows should fill the height. This is
   mechanics (the measured content size), not looks.
5. **Pre-warming is opt-in and off by default.** That is correct. Document the
   switch and its cost in the GUIMaker docs.
6. **The popup depth target (256 KB) is not in `memory-report`, and buffer
   growth beyond 1024/4096 is not re-registered.** Register both.
7. **Process: the build caches were deleted without asking.** While C: had
   219 MB free, `Motioner/target` and the engine's release and doc caches
   (about 10 GB) were deleted. They are rebuildable, but nothing may be
   deleted without the user's explicit OK. When the disk is full, stop and
   report the free space and the candidates for deletion.
8. **Process: the report says "Motioner working tree untouched", but
   `.control/CONTEXT.md` and the session log were modified.** That is fine to
   do, but report it accurately.

### Phase 8F: popup follow-ups, then Phase 9

Phase 8F is short. Finish it first, and then go straight on to Phase 9
(recipes, docs, and the HTML and CSS comparison) in the same session.

**Lane A (main agent): findings 1, 2 and 6.**
- `UiPass::gui_renderer()` goes through `PopupContext` into `new_sharing`, and
  `new_sharing_global` and its registry are deleted.
- The popup pipeline variant is cached per device.
- The depth target and buffer growth are registered in `memory-report`.

**Lane B (parallel subagent, Windows backend only): finding 3.**
- Build experiment (a), the layered window, behind a `PopupPresent` enum.
- Run experiment (b), the backend comparison.
- Record the table and choose the default in decision G-0010.

**Lane C (parallel subagent, guimaker only): finding 4.**
- Content-sized menus and selects, with a unit test.
- The pre-warm docs from finding 5.
- Re-shoot the three captures (normal, right edge and bottom edge) with the
  privacy-safe compositor.

**Then Phase 9,** as specified above, with its own lanes.

**Gate:**
- one final gate for both phases;
- the full Motioner gate in a clean worktree;
- divan compared against this phase's base.

**Disk rule.** If C: drops below 5 GB, stop. Report the free space and the
candidate caches, and wait for the user. Delete nothing yourself.

### Review after Phase 8F and Phase 9

**Accepted and pushed** (engine `c377604 + 431947e`).

**Phase 8F:**
- **`new_sharing_global` is gone.** `UiPass::gui_renderer()` is passed through
  `PopupContext.main`.
- **The sRGB/1 pipeline variant is built once per device.**
- **G-0010: Windows popups present through a layered window,** with no DXGI
  swapchain. Compared with the swapchain path:

  | | new popup | RSS | repaint |
  |---|---|---|---|
  | layered | about 9 ms | +0.5 MB | 0.3 ms |
  | swapchain | 53 to 69 ms | about +11 MB | |

  This closes the Phase 8E cost targets: under 8 MB and under 20 ms. Well done.
- **Content-sized menus:** 248×161 instead of 500×225.
- **The pre-warm docs.**
- **The depth target and buffer growth are in `memory-report`.**
- **Process: nothing was deleted, and the `.control` edits were reported
  accurately.** Thank you.

**Phase 9:**
- **The recipes:** seven new recipes plus the launcher. Each has a story, dark
  and light goldens, and a lint and sweep test.
- **The docs.** They include `design-guide.md` for models and a rewritten
  roadmap.
- **The HTML and CSS comparison.** Side by side, the screenshots are almost
  identical.

**Findings:**
1. **The Phase 9 targets were missed, honestly reported.**
   - The view is 242 lines, against 139 for HTML and CSS. The start screen
     alone is 151.
   - The view has 26 numeric literals.
   - Both are fixable in the library, not in the view:
     - named tokens for the priority weights, line caps, the 0.55/0.45 split
       and the fluid slope;
     - `.grow()` without an argument;
     - a `clock_label` helper;
     - compact builder forms where several calls repeat. For example, one
       `field(label, value)` for the four label-and-input pairs, and one
       `list_row(title, subtitle, trailing)` for the recent projects.
   - The target stands: at most 139 lines and **zero** numeric literals in the
     view. Any helper you add must be general library API, documented and
     usable by every app, not a start-screen special case.
2. **`p8d_popup` still computes VRAM with the swapchain formula.** Report what
   `memory-report` says for `popup:{h}:offscreen`.
3. **The layered readback waits on the whole device queue.**
   - `present_offscreen` polls the device until the copy finishes. In
     Motioner the main window queues a heavy 3D viewport, so a hover repaint
     on a menu would wait for the viewport's GPU work.
   - Measure it: a menu hover while the main window renders a heavy scene.
   - If the wait shows, map asynchronously and call `UpdateLayeredWindow` in
     the next callback once the map is ready. Until then, keep the previous
     popup image on screen.
   - This goes into Phase 9B.
4. **The goldens take 169.87 s.** Find out where that time goes before the 9B
   sweep: GPU init per test, PNG encoding, or serial runs. A gate that slow
   gets skipped.
5. **Two worktrees were left in place:** `FerrousEngine/out/worktrees/p8f-baseline`
   and `Motioner-gate-p8f`. Asking was correct. The user decides.

### Phase 9 fix, then Phase 9B

**Phase 9 fix (main agent):**
- findings 1 and 2;
- re-run `scripts/compare_start.py`;
- update `out/compare/REPORT.md` with the new counts and a fresh side-by-side
  screenshot.

**Phase 9B (as specified above, plus the following):**
- **The comparison set:** egui, iced, Slint and Dear ImGui (through imgui-rs
  with the wgpu renderer), plus WebView2 if time allows. They all build the
  same start screen in `bench/guimaker_compare/`, outside the workspace.
- **The measurement rules:**
  - release builds on the same machine, with the same vsync mode, window size
    and DPI;
  - five runs each, reporting the median and the range;
  - the host is kept free for the measurements: the user will close other
    apps when you say the measurement step is about to start, so stop and
    ask first;
  - save the raw numbers as JSON under `bench/guimaker_compare/results/`, so
    later phases can detect a regression.
- **A feature table next to the numbers.** For each library:
  - CJK and emoji with no setup;
  - popups outside the window;
  - idle CPU and idle frames;
  - per-monitor DPI;
  - IME.
- **Finding 3 (async readback) and finding 4 (golden time).**
- **The perf sweep deferred from the earlier phases:** the full divan suite and
  idle, hover and typing CPU and GPU, run on the free host before any
  optimisation, as the baseline.
- **The rule stands.** Wherever GUIMaker is not the lightest, profile and fix
  it, or record the precise measured reason.

### Review after the Phase 9 fix and Phase 9B

**Accepted and pushed** (engine `416b15c`):
- **Phase 9 fix: both comparison targets are met.** The view is 119 lines,
  against 139 for HTML + CSS, and has 0 numeric literals. This used general
  library API (`format::clock_hh_mm`, named tokens such as `Lines::ONE`).
- **Fair benches.**
  - The iced bench no longer subscribes to `window::frames()`.
  - The Slint bench now builds.
  - egui is measured with both glow and wgpu.
  - ImGui is built.
  - The raw JSON is under `bench/guimaker_compare/results/`.
- **Async layered readback** (`LayeredReadback`, the generation gate). With a
  600k-quad main scene, a menu hover no longer waits 7.16 ms for the device.
- **The goldens run in 46 s instead of 170 s,** byte-identical, through a
  shared `StoryTarget`.
- **`MemoryHints::MemoryUsage` is already the Flat2D default,** and the A/B
  proves it: −126 MB.
- **G-0011 keeps Vulkan as the Windows default**, measured.
- GUIMaker beats egui/wgpu on RSS and idle frames, and matches it on startup.

**The standing goal is not met yet.** Slint and egui/glow are lighter than
GUIMaker on most rows:

| | startup | idle frames | 30 s tail | RSS | binary |
|---|---|---|---|---|---|
| GUIMaker | 560 ms | 3 | 0.376 % | 227.5 MB | 11.07 MB |
| Slint | 307 ms | 1 | 0.109 % | 144.5 MB | 4.20 MB |
| egui/glow | 256 ms | 5 | 0.054 % | 144.5 MB | 4.54 MB |

The rule is to profile and fix. A driver floor counts as a measured reason
only after the alternatives below have been measured.

**Findings:**
1. **Hover redraws the whole frame on every pointer move.** This is a
   GUIMaker gap, not a like-for-like problem.
   - Sweeping the body gives 675 frames and 10.7 % CPU.
   - iced, Slint and ImGui give 1 frame, because nothing visible changes.
   - A pointer move that changes no hover target, cursor shape or tooltip must
     not request a frame.
2. **The DX12 renderer takes 6.2 s.** This was not investigated. The engine
   never sets `Dx12Compiler`, so wgpu uses FXC, which is known to be very slow
   on large shaders. The DX12 surface is 3.7× faster than Vulkan's (50 ms
   against 183 ms), so DX12 might win startup once its pipelines are fast.
3. **The Flat2D startup builds every pipeline up front.** A UI app should build
   only what its first frame draws, and compile the rest on a worker thread,
   or lazily on first use.
4. **Nobody has tried the GL backend.** egui/glow and Slint both start in about
   250 to 300 ms, with about 145 MB, on OpenGL. wgpu has a GLES backend on
   Windows (WGL, or ANGLE). Measure it for Flat2D UI apps before accepting the
   Vulkan floor.
5. **The binary size comparison is not fair yet.**
   `bench/guimaker_compare/Cargo.toml` has no `[profile.release]`.
   - Give every bench the same profile: `lto = "fat"`, `codegen-units = 1`,
     `strip = true` and `panic = "abort"`.
   - Report it at `opt-level = 3` and at `"s"`.
   - Compile only the wgpu backends the platform uses, and leave the 3D
     renderer out of a Flat2D build, behind a cargo feature.
6. **There are 3 idle frames at startup, where 1 is the target.** Find out
   what draws the other two (a scheduled wake, the font bake swap, a resize?)
   and remove them. The 30 s tail is 0.376 %, against 0.109 % for Slint. Find
   the wakes that belong to us.
7. **There is no typing bench, and no like-for-like hover bench.** Add the same
   text input and the same hoverable button to every bench app, then measure
   typing at 10 characters per second and a button hover.
8. **WebView2 does not build** because of a `gdk-sys` links conflict. Use wry
   with winit instead of tao, or `webview2-com` directly. This one is
   optional.
9. **Artefacts left in place:**
   - `FerrousEngine/target-p9b-base` (232 MB);
   - `FerrousEngine/out/worktrees/p9b-base` (102 MB);
   - `Motioner-gate-p9b` (12 GB).

   `target-p9b-base` is untracked at the engine root. Put such directories
   under `out/` (ignored) from now on. The user decides whether they are
   deleted.

### Phase 9C: close the gap to Slint and egui/glow

**The goal:** GUIMaker is lighter than or equal to the lightest library on
every row, or each remaining row has a measured floor *after* the
alternatives were tried.

Use lanes with parallel subagents that only edit. The main agent runs the one
final gate.

**Lane A: damage-driven input (main agent).** Finding 1.
- A pointer move requests a frame only when it changes something:
  - the hovered or pressed target;
  - the cursor icon;
  - a tooltip timer;
  - drag or scroll state.
- Unit test: sweeping a static body gives 0 extra frames.
- Bench: the body sweep gives 1 frame and less than 1 % CPU.

**Lane B: startup (subagent).** Findings 2, 3 and 4.
- DX12 with DXC (`Dx12Compiler::DynamicDxc`, or the static DXC feature)
  against FXC.
- Lazy or worker-compiled pipelines for Flat2D.
- The wgpu GL backend.
- Measure each on cold and warm startup. Pick the Windows default for Flat2D
  apps in a new decision (G-0012). It may differ from the 3D default.

**Lane C: size and idle (subagent).** Findings 5 and 6.
- The shared release profile.
- Feature-gated backends and 3D.
- 1 idle frame at startup.
- A 30 s idle tail with our own wakes removed.

**Lane D: benches (subagent).** Findings 7 and 8.
- Typing and button hover in every bench app.
- WebView2 if possible.

**Measurement:** stop and ask the user to free the host before the final runs.
Use the same rules as Phase 9B, then update `MEASURED.md`, `FEATURES.md` and
the table in this prompt's report.

**Gate:**
- the engine gate;
- the full Motioner gate in a clean worktree;
- divan compared against this phase's base.

**Disk:** keep baselines and targets under `out/`. Stop below 5 GB, and
delete nothing.
