# opencode prompt 10 — Motioner: professional motion design (animatable content, paths, gradients, effects, masks, blend modes, typography, images, pro easing)

You are the orchestrator of a very long, multi-phase job in two repositories:

- **Motioner** at `C:\Users\jvchi\CARPETAS\Motioner` (branch
  `feat/m1-foundations`). It renders MDF programs (Python-like DSL) to MP4
  and has a full editor with a typed command layer for agents (D-0024).
  Run this prompt **after** `opencode-prompt-09-zoom-rebind-hybrid-bake.md`.
- **FerrousEngine dev clone** at `C:\Users\jvchi\CARPETAS\FerrousEngine`
  (branch `feat/pure2d-text-and-readback`): renderer work that Motioner
  cannot do on its own (offscreen layers, blur, blend modes, masks, gradient
  fills). Motioner uses the pinned submodule `Motioner\engine\ferrous`,
  never the clone; at the end of each engine part the reviewer pushes and
  bumps the pin with an ADR (procedure in `engine/README.md`). Until the pin
  moves, Motioner phases that need engine features stop at "core + DSL +
  eval + tests + editor" and leave the render hook behind a clear
  `RenderError::Unsupported` (typed, tested), never a panic.

`examples/showcase.motion` (12 s, four scenes) shows what MDF v0 can do
today and, by contrast, what is missing. The goal of this prompt is that a
**`examples/showcase_pro.motion`** at the end looks like the work of a
professional motion studio: animated colours, lines that draw themselves,
gradients, soft shadows and glows, masked text reveals, per-character text
animation, real fonts, images, custom easing and motion blur.

Quality bar: top-tier studio. Zero duplication, rustdoc on public items,
typed errors, no panics on runtime paths, regression tests that fail without
the change, no warnings. Work phase by phase; run the gate after each phase.
The phases are ordered by value; if the job must be split across runs, stop
cleanly at a phase boundary and report.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the two folders above. Never edit
  `Motioner\engine\ferrous`. Do not commit, stage, stash, reset, checkout,
  rebase or push. The reviewer commits.
- `CARGO_INCREMENTAL=0`, `-j 2` (8 GB RAM). Release builds only to measure
  performance in the last phase. Free space on C: about 6 GB; stop below
  5 GB (you may clean caches outside the repositories).
- Motioner: `cargo fmt` (never `--all`), `cargo machete crates`. Clippy
  pedantic `-D warnings`; every `#[allow]` has a reason.
- FerrousEngine: run `rustfmt --edition 2021 --config skip_children=true`
  only on files you edit; keep the 25 CRLF files' endings
  (`git ls-files --eol`); run the engine's tests for the crates you touch.
- **New dependencies need an ADR first** (`pctl decision-new`): what it
  is, why the alternative of writing it is worse, licence, size, transitive
  deps. Candidates you may propose: `lyon_tessellation` (path fills and
  strokes), `ttf-parser`/`fontdue` are already in the engine via
  `ferrous_font` (prefer them), an image decoder already present for PNG
  (`png`; JPEG/WebP need an ADR).
- **Document format** stays MDF (D-0005, D-0011). Every new property is
  readable from MDF, keyable where it says so, overridable through
  `<name>.overrides.motion` (D-0022), bakeable, and visible in `query`.
- **Command layer is law (D-0024).** New editor actions are `Command`
  variants with parity coverage and `docs/agents.md` entries. Existing
  generic commands (`set`, `edit`, `key`, `unkey`, `stopwatch`, ...) must
  work for every new animatable property by name, so agents get the new
  features for free; extend `editor::channels` (one channel table) rather
  than adding special cases.
- **Determinism.** Pixels depend only on the document and the integer
  frame. Every effect (blur, shadows, motion blur, noise) is deterministic
  across runs on the same adapter; no wall clock, no unseeded randomness.
  The golden `hello_f120.png` stays byte-identical (hello.motion uses none of
  the new features). Add new goldens for the new features (see Phase 11) and
  document the tolerance policy for GPU differences in the ADR.
- **Boundaries** (`scripts/check_boundaries.py`): engine types only in
  `motioner_render` and `motioner_ui`; core/DSL/eval engine-free and
  file-system-free (asset bytes enter through `motioner_io`/`motioner_app`).
- Shell gotchas: Bash heredocs containing `'''` break in this environment
  (write files with the editor tool); PowerShell 5.1 drops unescaped `"` in
  native arguments; `core.autocrlf=true`.
- Screenshots: `scripts/ui_shot.ps1 -Out out/ui/p11-NN-<what>.png
  [-Commands ...]`; stills: `motioner still <file> --frame N --out ...`.
  Look at what you render and fix the worst visible problem first.

Phase gate (Motioner): `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`.

Stop conditions: C: below 5 GB; a gate you cannot make green after two
honest attempts; an accepted ADR that a phase would contradict; a needed
engine feature whose pin is not bumped yet (finish the Motioner side behind
`Unsupported` and continue with the next phase).

Before Phase 1, write one ADR **"Professional motion model"** that fixes
the shared decisions below, so later phases do not reinvent them:
- how a content property becomes animatable in `motioner_core`
  (`Animatable<T>` for numbers, colours, gradients; what interpolates and
  what holds);
- colour interpolation space (linear-light sRGB vs OKLab; pick one,
  justify with a rendered comparison in `out/`);
- effect stack model (`node.effect(kind, **params)`, ordered, keyable
  params) and how it maps to render passes;
- mask model (`node.mask(other, mode="alpha"|"luma", invert=False)`);
- blend mode list and their exact formulas (premultiplied alpha);
- asset model (project-relative paths, hashing for caches, where bytes
  are loaded);
- golden-test policy for effects (exact vs tolerance).

---

## Phase 1 — Animatable content properties

Today only transform channels animate; `fill`, `stroke`, `stroke_width`,
`width`, `height`, `radius`, text `size` and `color` are static, which is
the biggest visible limit of `showcase.motion`.

- Core: make them `Animatable` (colours with the chosen space).
- DSL: `node.key("fill", t, "#FF5A6E", ease=...)`, `node.key("width", ...)`.
- Eval: sample them per frame; renderer reads sampled values.
- Overrides/bake/query: `override_key("id", "fill", ...)` works; bake
  writes `.key("fill", ...)`; `query` reports their keys.
- Editor: content rows in the timeline get stopwatches, diamonds and eased
  lines like transform rows; inspector fields key them at the playhead
  (`edit` semantics, D-0024). Extend `editor::channels::CHANNELS` into a
  per-kind channel table; no duplicated per-property code.
- Tests: core interpolation (colour midpoints in the chosen space), DSL,
  eval, overrides round-trip, bake exactness, editor commands, a golden
  still of a colour/size animation.

## Phase 2 — Vector paths and trim paths

- `path(points, closed=False, fill=None, stroke="#ffffff", stroke_width=2,
  cap="round", join="round")` with straight segments and cubic Bézier
  segments (decide the MDF shape: list of points with optional handles, or
  builder methods `move_to/line_to/cubic_to/close`; justify in the ADR).
- Helpers in MDF: `polygon(sides, radius)`, `star(points, inner, outer)`,
  `arc(radius, start, end)`.
- Animatable `trim_start`, `trim_end`, `trim_offset` (0..1) for "draw on"
  effects; stroke `dash` and `dash_offset` (animatable).
- Render: tessellate fills and strokes (lyon via ADR, or your own
  flattening + stroker + polygon fill; anti-aliased edges either way), cache
  tessellation per node and invalidate on change.
- Editor: path nodes in layers/inspector/timeline; bounds and picking;
  gizmo works on the path's transform. Point editing on the canvas is out of
  scope (record it in the backlog).
- Tests: geometry (flattening tolerance, trim lengths), golden stills of
  a star, an arc and a half-drawn path.

## Phase 3 — Gradients

- `fill=linear_gradient(start=(x, y), end=(x, y), stops=[(0, "#..."), (1,
  "#...")])` and `radial_gradient(center, radius, stops)`, usable for fill
  and stroke of rects, circles, paths and text; stop colours and positions
  animatable (Phase 1 machinery).
- Engine Part A (if the UI/Pure2D shaders cannot do it): gradient fills in
  the Pure2D pass, dithered to avoid banding, exact sRGB stops.
- Tests: golden still with a linear and a radial gradient, banding check
  (max step between neighbouring pixels).

## Phase 4 — Blend modes

- Implement `normal`, `add`, `multiply`, `screen`, `overlay`, `darken`,
  `lighten` with the formulas fixed in the ADR (premultiplied alpha). Today
  MDF accepts only `"normal"`.
- Engine Part B as needed (pipeline variants or a compositing pass).
- Editor: blend mode in the inspector (segmented/dropdown), overridable.
- Tests: per-mode golden swatches over a known background.

## Phase 5 — Effects: drop shadow, blur, glow

- `node.effect("drop_shadow", color, offset=(x, y), blur, opacity)`,
  `node.effect("blur", radius)`, `node.effect("glow", color, radius,
  intensity)`; params keyable; effects apply to the node and its subtree
  (groups render to an offscreen layer).
- Engine Part C: offscreen layer targets, separable Gaussian blur (fixed
  kernel derived from the radius, deterministic), compositing back with the
  node's opacity and blend mode. Budget: 1080p60 preview with a handful of
  effects on the RTX 4050 (measure in release, Phase 12).
- Editor: an Effects section in the inspector (add/remove/reorder with new
  commands, params as fields with stopwatches).
- Tests: blur kernel unit tests; golden stills of shadow, blur and glow.

## Phase 6 — Masks and track mattes

- `node.mask(other, mode="alpha"|"luma", invert=False)`; the mask node is
  not drawn itself; mask transforms animate (moving masks for text reveals).
- Engine Part D: stencil or offscreen alpha multiply.
- Editor: mask relationship visible in layers (icon/indent) and the
  inspector; commands to set/clear a mask.
- Tests: golden stills of an alpha mask reveal and an inverted luma mask.

## Phase 7 — Typography

- Fonts: `text(..., font="Inter", weight=600)` loading TTF/OTF from the
  project (`assets/fonts/`) through `motioner_io`, with a clear error for a
  missing font; ship one open-licence family in `assets/fonts/` (licence
  file included, ADR) besides Roboto.
- Layout: `tracking` (letter spacing) and `line_height` (multi-line text with
  `\n`), both animatable; `text_width(content, size, font)` and
  `text_bounds(...)` built-ins so programs can lay out (showcase.motion had
  to hard-code Roboto advances).
- Per-character animation: a text animator in MDF, for example
  `node.animate_chars(property, from, to, start, stagger, duration,
  ease)` or `split_chars(text_node)` returning a group of per-glyph nodes
  with correct kerning; choose one in the ADR (deterministic, overridable,
  bakeable).
- Tests: shaping/advance tests, golden still of tracked, multi-line and
  per-character-animated text.

## Phase 8 — Images

- `image(source, width=None, height=None, fit="contain"|"cover"|"fill")`
  from project-relative paths (PNG now; JPEG/WebP only with an ADR), cached
  on the GPU by content hash; opacity, transform, masks, effects and blend
  modes apply.
- Editor: image layers with a thumbnail icon and size fields.
- Tests: decode, fit modes, golden still.

## Phase 9 — Professional easing and timing

- Custom easing: `ease=bezier(x1, y1, x2, y2)` (CSS cubic-bezier semantics,
  solved numerically and deterministically), `ease="hold"` (step), plus
  `elastic_out`, `bounce_out`, `expo_in_out`, `sine_in_out`. The overrides
  file, bake, `query`, the ease grid (curve previews) and the timeline
  lines support them.
- Motion blur: `comp(..., motion_blur=True, shutter=180, samples=8)`
  sampling sub-frame times deterministically and averaging in linear light;
  off in the editor preview by default (toggle command), on in `render`.
- Helpers: `stagger(nodes, property, values, start, step, duration, ease)`
  and `wiggle(seed, frequency, amplitude)` producing ordinary keys
  (deterministic, bakeable).
- Tests: bezier solver accuracy, motion-blur golden still, helper outputs.

## Phase 10 — Editor quality of life for big scenes

`showcase.motion` showed 472 layers named "Circle 1..300" in a flat list.

- `node.name = "Dot grid"` in MDF (display name; identity unchanged) and
  default names from the variable a node is bound to.
- Outline: collapsible groups (layers panel and timeline), a search/filter
  box (`filter_layers(text)` command), solo/shy toggles.
- Graph editor for one channel's value curve with draggable Bézier handles
  for `bezier` eases (commands for every edit).
- Performance: the editor must stay at 60 fps with 1000 layers (measure,
  fix hot paths, no per-frame allocations in timeline/outline drawing).

## Phase 11 — Showcase Pro and goldens

- Write `examples/showcase_pro.motion` (20–30 s, 1080p60) that uses every
  feature: animated colour palettes, self-drawing line art and icons, a
  gradient background that shifts, glass cards with shadows, a masked
  headline reveal, per-character kinetic typography in a real font, an
  image with a cover fit, blend-mode light leaks, custom bezier timing and
  motion blur. Keep `examples/showcase.motion` as the v0 baseline.
- Goldens: a handful of frames of `showcase_pro.motion` plus one per feature
  (Phases 1–9) in `crates/motioner_testkit/golden/`, with the tolerance
  policy from the ADR.
- Render the MP4 to `out/showcase_pro.mp4` and a contact sheet of eight
  frames to `out/showcase_pro_sheet.png`.

## Phase 12 — Performance, docs, report

1. Release build once: measure `render` ms/frame for both showcases and
   preview fps with the profiler overlay; record numbers in the report and
   in `.control/CONTEXT.md`.
2. Docs: `spec/mdf-v0.md` (or a new `spec/mdf-v0.2.md` if the ADR decides
   the language version moves), `docs/agents.md` (new properties and
   commands), root `README.md`, `crates/motioner_ui/README.md`,
   `crates/motioner_render` docs.
3. Tasks: one task per phase with `pctl task-new`, events with
   `pctl session-log`, close what is done; backlog what is left (canvas
   point editing, audio, video, 3D, particles).
4. Final report (in Spanish): what changed per phase, engine parts and the
   clone head(s) for the pin bump, ADRs, new dependencies (if any), golden
   list, screenshots and render paths, performance numbers, gate results,
   free space on C:, and what is left.
