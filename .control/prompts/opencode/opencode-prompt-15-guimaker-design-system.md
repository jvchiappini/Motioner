# opencode prompt 15 — GUIMaker: a UI library that is beautiful by default and easier than HTML + CSS

You are the orchestrator of a very long, multi-phase job in three folders:

- **GUIMaker** at `C:\Users\jvchi\CARPETAS\GUIMaker` (remote
  `github.com/jvchiappini/GUIMaker`). Today it is a small designer binary
  (`src/main.rs`, `src/panels`, `src/scene.rs`) that does not even build: it
  depends on `../crates/ferrous_app`, which no longer exists. This prompt turns
  the repository into **the UI library of the FerrousEngine ecosystem**, which
  is what D-0004 always meant by "GUIMaker".
- **FerrousEngine dev clone** at `C:\Users\jvchi\CARPETAS\FerrousEngine`: only
  for render primitives the library needs and the engine lacks. **Engine
  features must be generic** (no GUIMaker or Motioner names), with rustdoc,
  crate docs in `crates/<crate>/docs` + `mkdocs.yml` nav, tests and ideally an
  example. The reviewer pushes and bumps pins.
- **Motioner** at `C:\Users\jvchi\CARPETAS\Motioner` (branch
  `feat/m1-foundations`): only in the last phases, to rebuild the start screen
  and the dialogs from prompt 11 on the new library. Run those phases **after**
  `opencode-prompt-11-projects-assets-media.md` is committed.

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
| **Gesture recognition**: click, double/triple click, long press, drag threshold, pointer coalescing, key repeat | FerrousEngine (`ferrous_events`) | the same semantics for canvas and UI; implemented once |
| Render flags, redraw scheduler, damage regions (Phase 0B) | FerrousEngine | the engine owns frames |
| Generic **spatial index** (grid/BVH over AABBs, incremental updates) | FerrousEngine, in a GPU-free crate | used by UI hit testing and by scene picking; one implementation |
| **Scene / canvas picking**: objects with transforms and rotation, path fill containment, stroke distance, text bounds, clips and masks, z-order, ID-buffer picking, marquee and lasso queries, snapping queries, screen ↔ world transforms, pan/zoom-to-cursor camera helpers, 3D raycasts | FerrousEngine (2D scene / renderer crates) | it knows the geometry it draws |
| UI tree: layout, responsive rules, tokens, components | GUIMaker | |
| **UI routing**: which widget gets an event, hover/press state, focus and `Tab` order, shortcuts scopes, overlays and focus traps | GUIMaker | screen-space rects of its own tree |
| Text-editing engine (`Ctrl+A`, selection, undo…) | GUIMaker | a UI concern on top of engine key events and clipboard |
| `canvas` / `viewport` element | GUIMaker reserves the rect, routes events inside it, in local coordinates, to the app | the bridge between both worlds |
| What a pick *means* (select an MDF node, drag a gizmo handle, edit a keyframe) | the app (Motioner) | domain logic |

The GUIMaker core stays free of drawing crates, but it **may depend on the
engine's GPU-free crates** (`ferrous_events`, the spatial index, math) so
nothing is implemented twice.

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

- Work only inside the three folders above. Never edit
  `Motioner\engine\ferrous`. Do not commit, stage, stash, reset, checkout,
  rebase or push in any repository. The reviewer commits.
- `CARGO_INCREMENTAL=0`, `-j 2` (8 GB RAM). Never run two cargo builds at the
  same time. Stop if free space on C: drops below 5 GB (you may clean caches
  outside the repositories).
- GUIMaker and Motioner: `cargo fmt`, clippy pedantic `-D warnings`, every
  `#[allow]` has a reason, `cargo machete`. FerrousEngine: `rustfmt --edition
  2021 --config skip_children=true` only on files you edit; keep CRLF files'
  endings (`git ls-files --eol`); run the engine tests of crates you touch.
- **Decisions.** GUIMaker gets its own decision log:
  `GUIMaker/docs/decisions/G-NNNN.md` (context, decision, consequences,
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
   (`gui_batch.rs`, the WGSL it feeds), `ferrous_font`; GUIMaker `src/` and
   `docs/`. List what each one does well (Motioner's tokens, WCAG tests, text
   baseline/kerning/ellipsis, vector icons are good seeds) and what blocks good
   design.
2. Take screenshots of today's Motioner start screen and New project dialog
   (`scripts/ui_shot.ps1 -Out out/ui/p15-00-before-*.png`). They are the
   "before" of the final report.
3. Write **G-0001 "GUIMaker library architecture"** deciding:
   - **Repository layout**: a Cargo workspace in GUIMaker:
     `crates/guimaker` (core free of engine drawing crates, may use the GPU-free ones: view tree, style, theme, layout,
     input routing, state, display list, lint, inspect),
     `crates/guimaker_ferrous` (thin adapter: draw commands → engine API,
     input events in, text metrics from `ferrous_font`; no drawing logic of
     its own — if the adapter needs more than a mapping, the capability
     belongs in the engine), `apps/gallery` (storybook), `apps/designer` (the old
     designer, revived in Phase 9). The old `src/` moves or is deleted; no
     dead code stays.
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
   - **Engine dependency** so that Motioner builds with **one** copy of the
     engine: e.g. GUIMaker depends on the engine by git URL + rev, and
     Motioner `[patch]`es that URL to its `engine/ferrous` submodule paths.
     Verify the mechanism with a throwaway check before deciding.
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
`app/pointer.rs`, `app/drag.rs`: read them, move gesture logic to the engine
and UI logic to GUIMaker, and leave Motioner importing both (no copy
remains). Engine parts first (dev clone, generic, documented), then GUIMaker
on top:

- **Engine (`ferrous_events`)**: gesture recognition (click with the platform
  double-click time and distance, double/triple, long press, drag start
  threshold, pointer capture, coalescing, key repeat, modifier state) as a
  pure, tested state machine fed by raw events and the injected clock; the
  generic spatial index in a GPU-free crate.
- **GUIMaker (`guimaker::input`)**: routing of those gestures through the UI
  tree, hover/press/focus, shortcuts, overlays, drag and drop between UI
  elements, and the text-editing engine.

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
- **Docs** (`GUIMaker/docs/`, English, mkdocs-ready): getting started (a
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
   reference) as standalone bench apps in `GUIMaker/bench/` (excluded from
   the workspace; their dependencies never reach the library; a decision
   records that). Measure for each: idle CPU/GPU, CPU and GPU per frame while
   hovering and typing, RSS, GPU memory, startup to first frame, release
   binary size. Report a table. Where GUIMaker is not the lightest, profile
   (`cargo flamegraph`/ETW, heap counts) and fix it, or record precisely
   why and what it would take.
5. Optimise until every row of the budget table is met; the bench numbers go
   in the GUIMaker README.

## Phase 10 — Motioner adopts GUIMaker (after prompt 11 is committed)

1. Motioner ADR (`pctl decision-new`) **"Editor chrome on GUIMaker"**:
   amends D-0004/D-0018/D-0020/D-0021 — GUIMaker pinned as a submodule
   (e.g. `ui/guimaker`, shallow, documented in a README like
   `engine/README.md`), the engine `[patch]` from G-0001 so one engine copy is
   built, `scripts/check_boundaries.py` allows `guimaker*` only in
   `motioner_ui`, and the plan for which panels migrate when.
2. Motioner's theme becomes a GUIMaker `Theme` built from the D-0021 seeds
   (same accent, compact density for the editor, comfortable for the start
   screen). `theme.rs` keeps only what is Motioner-specific; the icons and
   any token now in GUIMaker are deleted from Motioner.
3. **Rebuild the start screen and the New project dialog** on GUIMaker,
   following the launcher recipe, keeping `StartAction` (now the message
   type) and every `Command` and parity test from prompt 11 working. Target
   design: a comfortable-density centred column; wordmark + one-line tagline;
   two large action cards (New project / Open project: icon, title, one-line
   description, keyboard hint); "Recent projects" section as a responsive
   grid of cards with the real 16:9 thumbnail from `cache/`, name, relative
   time ("2 h ago"), middle-ellipsised path in `Mono`/faint, hover lift,
   context menu (Open, Reveal in Explorer, Remove from list), missing entries
   greyed with a Remove action; an Examples row; a quiet footer (version,
   docs). Empty state when there are no recents. New project dialog: modal
   with form rows, size presets (1080p, 4K, square, vertical) as a segmented
   control plus custom fields, inline validation, Create as the only primary.
4. Also move to GUIMaker every menu, dialog, tooltip and toast prompt 11 added
   (File menu, import errors…) if it is self-contained. Delete
   `StartLayout`, the hand-computed rects and any helper left without users.
   Zero duplication: no Motioner widget may re-implement a GUIMaker one.
5. Motioner's text fields (inspector, New project form, agent composer later)
   use the GUIMaker text-editing engine; `text_edit.rs` and any duplicated
   pointer/drag helpers are gone from Motioner. The composition canvas uses
   the engine gestures and Phase 4B picking; the canvas sits in the GUIMaker
   layout as a `canvas` element and its own clicks never go through UI
   widget hit testing.
6. Screenshots `out/ui/p15-10-after-*.png` at 800×600, 1280×800, 1920×1080
   and 3840×2160, scale 1 and 1.5, plus lint clean on the start view across
   the sweep. Compare with Phase 0.

## Phase 11 — Dogfood: the GUIMaker designer, and the backlog

- Revive `apps/designer` on the library (it is the best stress test): canvas
  with the app shell recipe, component palette, inspector built from forms,
  live preview of the designed view rendered by the library itself, and
  codegen that emits **GUIMaker builder code** (the old generator emitted
  absolute positions — delete it). Keep scope to what the library proves; the
  rest goes to the roadmap.
- Backlog (GUIMaker `docs/roadmap.md` and Motioner tasks with
  `pctl task-new`): migrating Motioner's inspector, layers list, top bar and
  agent panel (prompt 14) onto GUIMaker; engine `ferrous_ui_core` pieces that
  GUIMaker supersedes; accessibility tree export; RTL/IME text input.

## Phase 12 — Report

`pctl session-log` in Motioner and a session note in GUIMaker. Final report
**in Spanish**: what changed per phase; before/after screenshots of the start
screen; the HTML + CSS comparison numbers; the resource table against the
budget and against egui/Slint/WebView2, and Motioner's idle and startup
numbers before and after; decisions (G-NNNN, Motioner ADR)
and dependencies; engine changes waiting for a push and pin bump; gate
results in every repository; free space on C:; what is left.
