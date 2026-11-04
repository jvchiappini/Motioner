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

### The goal

A person (or a model) describes **structure and intent**, never coordinates,
and gets a screen that looks like Linear, Raycast, Figma or Vercel's Geist
without trying. Concretely:

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
4. **Interaction is free.** Hover, pressed, focus ring, disabled, selected and
   their animated transitions come from the component, not from the caller.
   Keyboard focus and activation work everywhere.
5. **Feedback for whoever writes UI.** A text dump of the laid-out tree, a
   design lint, and a headless screenshot of any view at any size, scale and
   theme. A model iterates on UI the way it iterates on code: write, look,
   lint, fix.
6. **Less code than HTML + CSS** for the same screen (measured in Phase 7).

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

GUIMaker gate: `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`cargo machete`.
Motioner gate (phases 8–9): the full gate of prompt 11 (fmt, clippy, test,
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
     `crates/guimaker` (engine-free core: view tree, style, theme, layout,
     input routing, state, display list, lint, inspect),
     `crates/guimaker_ferrous` (backend: display list → engine `GuiBatch`,
     text measurement and shaping from `ferrous_font`, images, offscreen
     screenshot), `apps/gallery` (storybook), `apps/designer` (the old
     designer, revived in Phase 9). The old `src/` moves or is deleted; no
     dead code stays.
   - **Programming model.** Recommended, justify or replace: a declarative
     tree rebuilt every frame from the app's state (cheap, no stale widgets,
     ideal for a model to write), laid out, then input is routed through
     *this frame's* tree (no one-frame lag), then painted. Persistent state
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
- **Input routing** through the laid-out tree: hit testing respects clip and
  z-order; pointer capture during drags; hover/press/focus state in the
  keyed store; keyboard focus order follows the tree, `Tab`/`Shift+Tab`,
  `Enter`/`Space` activate, `Esc` closes the top overlay.
- **Display list**: the core's output is a flat, backend-agnostic list
  (rounded rect with per-corner radii, border, shadow, linear/radial
  gradient, image with `fit` (`Cover/Contain/Fill`), text run, icon, clip
  push/pop with radius, layer opacity, backdrop blur request). The backend
  maps it; the core never names an engine type.
- **Inspect**: `ui.inspect()` → stable, indented text dump (kind, key, rect,
  resolved style tokens, text) used by tests and by models. Document it as
  the first thing to read when a layout looks wrong.

Tests: layout snapshots via `inspect` at 3 sizes × 2 scales; focus order;
hit testing under clips and overlays; text clamp/ellipsis; pixel snapping.

## Phase 3 — FerrousEngine backend and the screenshot loop

- `guimaker_ferrous`: display list → `GuiBatch` (reuse the frame batch as the
  engine does since `e482b3c`), text through `ferrous_font` with kerning and
  real weights, SVG/vector icons, images and textures, clips.
- A host adapter for `FerrousApp` (feed input events and scale factor in,
  messages out, paint in `draw_ui`), ~one screen of glue for an app to adopt.
- **Headless screenshot**: render any view to PNG at a given size, scale and
  theme through an offscreen target and the engine's readback (prompt 05).
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

## Phase 4 — Components (each one finished, not sketched)

Every component: all states (rest, hover, pressed, focus-visible, disabled,
selected/checked, loading where it applies), keyboard support, both densities,
light and dark, animated state transitions, a gallery story, golden
screenshots, rustdoc with a 3-line example.

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

## Phase 5 — Motion and polish

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

## Phase 6 — Design lint (the guard rail for models)

`guimaker::lint(&laid_out_tree, &theme) -> Vec<Finding>` with severity, the
offending key path, the rect, and a fix hint written for a model to act on:

- overlapping siblings that are not in a `stack`/overlay;
- text clipped or truncated without an explicit `lines(n)` / ellipsis;
- contrast below the role threshold on the actual background under it;
- `Px` escape hatches and spacing not on the token scale;
- near-miss alignment (edges 1–3 px apart that should match);
- inconsistent radii or elevations among siblings of the same kind;
- hit targets smaller than 24×24 px; icon-only button without tooltip;
- more than one `Primary` action in the same group; empty containers;
- content wider than its `max_width` column; horizontal overflow.

Plus `gallery lint <story>` and a test helper `assert_clean(view)`. Every
gallery story and recipe must lint clean.

## Phase 7 — Recipes, docs and the HTML + CSS comparison

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

## Phase 8 — Motioner adopts GUIMaker (after prompt 11 is committed)

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
5. Screenshots `out/ui/p15-08-after-*.png` at 1280×800 and 1920×1080, scale
   1 and 1.5, plus lint clean on the start view. Compare with Phase 0.

## Phase 9 — Dogfood: the GUIMaker designer, and the backlog

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

## Phase 10 — Report

`pctl session-log` in Motioner and a session note in GUIMaker. Final report
**in Spanish**: what changed per phase; before/after screenshots of the start
screen; the HTML + CSS comparison numbers; decisions (G-NNNN, Motioner ADR)
and dependencies; engine changes waiting for a push and pin bump; gate
results in every repository; free space on C:; what is left.
