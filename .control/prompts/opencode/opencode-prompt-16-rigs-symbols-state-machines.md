# opencode prompt 16 — Motioner: imports, symbols and character rigs (a Rive-class system, code-first: `use("mascot.svg")`, custom properties, per-frame drivers, bones, meshes, poses, state machines)

You are the orchestrator of a very long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`). Run this prompt **after**
`opencode-prompt-13-motion-std-and-agent-tools.md` is committed (it builds on
its components, presets and tool runtime). Engine work, if any, goes to the
dev clone at `C:\Users\jvchi\CARPETAS\FerrousEngine` as **generic** features
documented in the engine docs (rustdoc, `crates/<crate>/docs`, `mkdocs.yml`
nav, tests, an example); the reviewer pushes and bumps the pin. If prompt 15
phase 10 (Motioner on GUIMaker) is committed when you reach the UI phase,
build the new panels with GUIMaker; otherwise follow the current editor
toolkit and keep each panel in its own module with logic separated from
drawing.

Reference (read-only, outside the repo):
`C:\Users\jvchi\CARPETAS\DailyAiRecap\mascot_controller.html`. It is a hand
made parametric character: about 60 parameters (squash/stretch, 4 movable
corners, perspective, bulges, per-eye width/height/lids/pupil/brows with
left/right mirroring, mouth smile/open/teeth/corners, glow, sparks), a
geometry function `apply()` that derives every path from the parameters
(e.g. eyelids computed with trigonometry), 17 presets that are **partial
poses** blended with a tween, two keyframed clips ("salto" squash jump and
"giro" turn-around with derived parameters), a look-at pad driving the
pupils, blink, and an SVG export. **That mascot is the acceptance test of
this prompt**: Motioner must rebuild it as a rig, more conveniently than the
HTML, and render the same poses and clips.

## Why this matters

The user wants Motioner to have a character and interactive-graphics system
as complete as Rive, and better where Motioner's nature allows:

- **Code-first.** A rig is readable MDF that diffs in git, that the UI
  writes through (D-0036) and that the future small model and the in-app
  agent can write and drive ("make the mascot sad and jump at 2 s").
- **One import model for everything.** `use("mascot.svg")`,
  `use("photo.png")`, `use("music.wav")`, `use("clip.mp4")`,
  `use("lib/titles.motion")` all return a *definition* you instantiate by
  calling it. Re-importing a changed file keeps every animation, because
  parts are addressed by stable ids.
- **Timeline + state machine together.** In Rive a state machine runs at
  runtime; in Motioner its inputs are also animatable on the timeline, so a
  video can say "at 3 s set `mood = sad`" and the machine performs the
  transition deterministically. The same rig later runs interactively
  (preview listeners now, a headless runtime for export later).
- Everything deterministic, frame-exact, renderable to video with motion
  blur, effects, masks and (prompt 14/T-RENDER-0014) 3D layering.

Target shape (illustrative; the ADR decides the exact syntax):

```python
mascot = use("rigs/mascot.motion")     # a rig module built on mascot.svg
m = mascot(pos=(960, 540), scale=3)

m.pose("happy", at=1.0, dur=0.35, ease="power2_out")
m.mouth_open.key(2.0, 13)
m.look.key(3.0, (0.6, -0.2))           # vec2 property driving both pupils
m.play("jump", at=4.0, loops=2)
m.mood.key(6.0, "sad")                 # state machine input on the timeline
```

```python
# rigs/mascot.motion
art = use("../assets/mascot.svg")      # a symbol: parts addressable by SVG id

def mascot(pos=(0, 0), scale=1):
    r = rig(art, pos=pos, scale=scale)
    r.prop("squash", 1.0, range=(0.4, 2.0))
    r.prop("mouth_open", 0.0, range=(0, 20))
    r.prop("lid_top", 0.0, range=(0, 1), mirror=("eye-L", "eye-R"))
    r.prop("look", (0, 0), range=((-1, -1), (1, 1)))
    r.driver(lids)                     # per-frame pure function, see Phase 4
    r.aim(r.part("pupil-L"), r.look, radius=6)
    r.pose("happy", ms=10, mo=9, lid_bottom=0.3)
    r.clip("jump", [...])
    r.machine(...)                     # Phase 7
    return r
```

Quality bar: top-tier studio. Zero duplication, rustdoc on public items,
typed errors, no panics on runtime paths, regression tests that fail without
the change, no warnings. Work phase by phase; run the gate after each phase.
If the job must be split across runs, stop cleanly at a phase boundary and
report.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the repository (and the engine dev clone for generic
  engine features). Read the DailyAiRecap file; never modify it. Never edit
  `engine\ferrous`. Do not commit, stage, stash, reset, checkout, rebase or
  push: the reviewer commits.
- `CARGO_INCREMENTAL=0`, `-j 2` (8 GB RAM; if the pagefile runs out use
  `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`). Release builds only
  for measurements. Stop if free space on C: drops below 5 GB.
- `cargo fmt` (never `--all`), `cargo machete crates`, clippy pedantic
  `-D warnings`; every `#[allow]` has a reason.
- **New dependencies need an ADR first** (`pctl decision-new`). Expected:
  an SVG parser (evaluate `usvg` vs the engine's `ferrous_svg`/`lyon_svg`,
  whose `svgparser 0.8.1` already triggers a future-incompatibility warning;
  prefer one maintained parser, used through a generic engine feature if the
  engine also needs it) and possibly `resvg` as a **dev-dependency** to
  produce reference rasters for SVG fidelity tests.
- **Boundaries**: `motioner_dsl` stays engine-free and file-system-free (it
  receives parsed asset data through the host/resolver like images today);
  file reading and SVG parsing in `motioner_io`; per-frame evaluation of
  rigs (drivers, deformers, constraints, poses, state machines) in
  `motioner_eval`, as pure data → data; drawing in `motioner_render`; UI in
  `motioner_ui`. A headless rig runtime must not depend on the editor.
- **Determinism and goldens.** Every existing golden stays byte-identical.
  Rig evaluation is a pure function of (rig, time, inputs); no clocks, no
  hash-map iteration order, no floating-point reassociation across runs.
- **Performance.** The interpreter budget of prompt 12 holds for programs
  without rigs (median within +7 % of the baseline recorded in CONTEXT).
  The mascot rig evaluates in under 1 ms per frame in release (target
  0.3 ms) and a scene with 50 mascot instances previews at 60 fps on the
  reference machine, or the report says why.
- **Write-through (D-0036) applies to everything new**: property edits,
  keys, poses, clip edits, bone transforms, state machine edits from the UI
  become MDF code with the same planner guarantees (verify, exact undo,
  fallback with reason). Add corpus cases under
  `crates/motioner_dsl/tests/write_through/` for each new edit kind.
- Every new language feature: spec (`spec/mdf-v0.md`), printer/fmt, lint
  rules where useful, `motioner tools --schema` descriptors (prompt 13's
  single generator), traces (prompt 12b), docs page with examples.
- Screenshots: `scripts/ui_shot.ps1 -Out out/ui/p16-NN-<what>.png`.

Phase gate: `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`, `pctl doc-check-refs`.
Engine clone (when touched): its own fmt/clippy/test/doc gate.

Stop conditions: C: below 5 GB; a gate you cannot make green after two
honest attempts; an accepted ADR that a phase would contradict.

Parallelism: Phases 1 and 2 are sequential. After Phase 4, Phases 5, 6 and
7 can run as three subagents (deformers / poses+clips / state machines)
because they share only the property and driver model from Phases 3-4; the
orchestrator integrates. Phase 8 (UI) after them. Phase 10 (runtime spike)
can run alongside Phase 8.

---

## Phase 0 — Study, audit and decisions (no feature code)

1. Study Rive's feature set (artboards and nested artboards, bones and
   skinning, meshes with vertex weights, constraints — IK, distance,
   transform, translation/rotation/scale, follow path, scroll — state
   machines with number/bool/trigger inputs, blend states 1D/additive,
   transitions with conditions/durations/exit times, layers, listeners,
   events, data binding / view models, text runs, layouts, audio events,
   scripting) and Lottie's (shape layers, precomps, expressions). Write a
   comparison table in `docs/rigs.md`: each feature → Motioner equivalent,
   phase that delivers it, or "deferred, why". The goal is parity on what
   matters for character and UI animation and a clear "better because" list.
2. Audit the current model: execution once per load (spec §2), keys and
   animators in `motioner_core::animation`, `use()` (spec §10, still a v0
   proposal), asset resolution by key (spec §7.7, D-0035/D-0045 `asset()`
   reserved), origins and the planner, `motioner_eval` sampling. Record what
   per-frame evaluation needs and its cost.
3. ADRs (`pctl decision-new`), each accepted before the phase that needs it:
   - **Unified imports**: `use(path)` dispatches by file kind and returns a
     *definition* (module, symbol, image, audio, video, font; later glTF and
     Lottie), callable to instantiate, with metadata attributes (`width`,
     `height`, `duration`, `parts`...). `image("key")`/`audio`/`video`
     remain as sugar with identical output (goldens unchanged); decide what
     the planner writes when the Assets panel places an asset; sandboxing
     and resolution reuse spec §7.7. Hot reload and identity rules.
   - **Rigs and the per-frame evaluation model**: properties (types: float,
     int, bool, enum, color, vec2, part reference; default, range, step,
     mirror groups, units), how instances animate them (keys on the
     instance), **drivers** (pure MDF functions compiled to a restricted
     per-frame evaluator: read properties and time, write properties of the
     rig's own parts and path points; no node creation, no I/O, bounded
     steps, results cached by input tuple) vs **native deformers** (bones,
     skinning, mesh/lattice, blend shapes, constraints in Rust). Explain why
     both, and the cost budget.
   - **Poses, clips, layers and state machines**: data model, blending math
     (per-property type: lerp, slerp for angles, colour in linear space,
     enums/bools step), layering (override/additive, per-property masks),
     machine semantics frame-exact on the timeline, events.
   - Dependencies (SVG parser, optional `resvg` dev-dependency).
4. Copy the reference mascot into the repo as test data:
   `examples/rig/assets/mascot.svg`. The controller generates its SVG with
   JavaScript; the user can export it with the "Descargar SVG (pose actual)"
   button in the Neutral pose. If the file is not present, reproduce the
   Neutral SVG from the template strings and `apply()` in the HTML (keep the
   ids: `mascot`, `body`, `face`, `eye-L/R`, `pupil-L/R`, `lid-top-*`,
   `lid-bottom-*`, `brow-*`, `mouth`, `mouth-shape`, `mouth-teeth`,
   `mouth-line`, `glow`, `sparks`...). Also save the 17 presets and the two
   clips as JSON fixtures extracted from the HTML for later comparison.

## Phase 1 — Unified imports and definitions

- Implement the ADR: `use()` for `.motion` modules (finish spec §10, move it
  out of "deferred" in §17), images, audio, video, fonts and SVG (symbol
  stub until Phase 2). Definitions are values; calling one creates the
  node(s) with the usual keyword arguments; attributes expose metadata.
- `image("key")`, `audio(...)`, `video(...)` keep working and produce
  identical documents; a lint suggests nothing (both are valid style) unless
  the ADR says otherwise.
- Hot reload: a changed imported file re-runs the program; nodes keep their
  identity, selection and keys.
- Write-through: placing an asset from the Assets panel inserts the
  import/instance per the ADR; corpus cases.
- Tests: every kind imported, metadata, sandbox errors (absolute, `..`,
  missing, wrong kind) with spans, module cycles, hot reload identity.

## Phase 2 — SVG symbols (any SVG becomes an editable, addressable symbol)

- Parse SVG into a symbol: groups, paths (all commands, arcs converted
  exactly), basic shapes, transforms, fill/stroke (width, caps, joins,
  dashes, miter), opacity, linear/radial gradients, clip paths and masks
  (map to D-0033 masks), `<use>`/`<defs>`, `fill-rule`, `viewBox`/
  `preserveAspectRatio`, text as paths or text nodes (decide), embedded
  images. Unsupported features produce warnings with the element id, never
  silent drops.
- Every element with an `id` (or an Inkscape/Figma/Illustrator layer label)
  is an addressable part: `m.part("eye-L")`, iteration over `m.parts`, and a
  sanitised attribute form if the ADR adopts it. Parts are real nodes:
  animatable, overridable, maskable, write-through capable.
- "Unpack to code": a command (UI and CLI) that converts an imported symbol
  into plain MDF statements in the program (printer-formatted), for users
  who want to own the geometry.
- Fidelity: render each test SVG (a set of 20+ under
  `crates/motioner_io/tests/svg/`, including the mascot and SVGs exported
  from Figma, Illustrator and Inkscape you author by hand) and compare with
  a reference raster (resvg dev-dependency if its ADR is accepted, else
  committed reference PNGs) under a documented tolerance.
- Re-import: editing the SVG on disk keeps keys on parts whose ids remain;
  a removed id reports which keys were orphaned.

## Phase 3 — Components with custom properties

- `rig(symbol_or_group, ...)` (name per ADR) creates a component instance
  with declared properties (types from the ADR). Properties are animatable
  on instances with the existing key API (`m.mouth_open.key(t, v, ease=...)`),
  appear as rows in the timeline, as sliders/fields in the inspector, are
  queryable (D-0024 `query`) and settable by commands (`set mascot.mouth_open
  12`), and are part of `motioner tools --schema` for agents.
- Mirror groups: one property edits both sides (with sign flips per the
  ADR, like the HTML's `MIR` table); per-side override when needed.
- Nested components (a rig inside a rig, Rive's nested artboards) with
  property forwarding.
- Write-through for property values, keys and declarations; corpus cases.

## Phase 4 — Per-frame drivers and the evaluation model

- Implement the ADR's driver model: `r.driver(fn)` registers a pure MDF
  function evaluated per frame by `motioner_eval` from the property values
  at that frame; it writes properties of the rig's parts (transforms,
  colours, opacity, path points/commands). The HTML's `lid()` and
  `corners()` must be expressible as drivers.
- Compile drivers once into a compact, sandboxed form (no allocation per
  frame in the hot path where possible); cache results by property tuple;
  bounded steps with a typed error that names the driver and frame.
- Bake: a command converts a driver-driven animation into plain keys for a
  frame range (for export to tools without drivers).
- Tests: determinism (two evaluations equal bit for bit), scrubbing
  backwards equals forward, caching invisible, limits, error spans; bench
  of the driver path.

## Phase 5 — Deformers and constraints (native, in Rust)

- Bones: hierarchy, bind pose, skinning of path points with weights
  (auto-weights by distance as a starting point, editable), bone keys.
- Mesh deformation of **images and video** (textured triangle mesh) and of
  shapes: lattice/free-form deform (the mascot's 4 movable corners and
  bulges are a 2x2 lattice with edge bends). Textured mesh drawing is a
  generic FerrousEngine feature if the engine lacks it.
- Blend shapes (morph targets): named shapes with compatible topology
  blended by weight; a helper that checks/creates compatible topology.
- Constraints: aim/look-at with radius clamp (pupils), 2-bone IK with pole,
  follow path, copy/limit transform, distance, and spring follow-through
  (use T-DSL-0011 springs if prompt 13 or earlier delivered them, otherwise
  implement the spring here and close that part of T-DSL-0011).
- Evaluation order documented (properties → drivers → deformers →
  constraints → draw) and tested.

## Phase 6 — Poses, clips and layers

- Poses: named partial property sets on the rig; `m.pose(name, at, dur,
  ease)` writes the transition as keys or as a pose track (ADR); blending
  per property type; mirrored poses (`"wink"` on the other side).
- Clips: reusable keyframed sequences on rig properties (the HTML's
  `[start, duration, ease, partial pose]` format maps 1:1), `m.play(name,
  at, loops, speed, blend_in, blend_out)`.
- Layers: override and additive layers with per-property masks so a blink
  or breathing layer plays over a talking clip; procedural helpers (blink
  every N seconds with jitter, breathe, idle sway) using deterministic noise.
- Write-through: edits in the pose library and clip editor become code.

## Phase 7 — State machines

- Inputs (number, bool, trigger, enum), states (pose, clip, blend 1D/2D by
  input), transitions (conditions on inputs, duration, ease, exit time,
  interruptibility), any-state transitions, layers of machines, events
  emitted on transitions/clip markers (an event can trigger an audio clip
  at its exact frame).
- Timeline integration: inputs are animatable on the instance
  (`m.mood.key(6.0, "sad")`, `m.jump.fire(4.0)`); the machine is evaluated
  frame-exactly and deterministically, including when scrubbing backwards
  (evaluate from the last keyed checkpoint; document the rule).
- Preview listeners: in the preview window, pointer position/hover/click
  can drive inputs (the HTML's look pad becomes a listener) for interactive
  testing; recording listener input to keys is a command.

## Phase 8 — Editor rig mode

- Rig panel: property sliders grouped like the HTML's sections, with the
  left/right/both toggle and mirror, pose library (thumbnails rendered from
  poses, apply with duration/ease, save current as pose), clip list with
  play/loop, on-canvas handles for lattice corners, bone tool, weight
  painting (basic), aim targets.
- State machine graph view: states and transitions as a node graph,
  inputs list, live state highlight during playback.
- Timeline rows for properties, pose tracks and machine inputs.
- Everything goes through D-0024 commands and write-through; undo exact.
- Screenshots `p16-01..` of each tool.

## Phase 9 — Acceptance: the mascot

- `examples/rig/mascot.motion` (rig module) and
  `examples/rig/mascot_demo.motion` (a 15 s video using it: all 17 presets
  as poses, the jump and turn clips, look-at, blink layer, a state machine
  for mood driven by keyed inputs, an event that plays a sound).
- Compare each preset and key frames of both clips with the HTML's
  geometry (fixtures from Phase 0): the rig's derived paths must match the
  HTML's `apply()` output within a documented tolerance; side-by-side
  screenshots in the report.
- The rig is shorter and clearer than the HTML (line count and a short
  qualitative note in the report).
- Goldens for a few frames of the demo; `motioner corpus check` passes on
  it; `<name>.prompt.md` briefs for the new examples.
- Agent tools: an agent can list the rig's properties, poses, clips and
  inputs from `motioner tools --schema` / `query` and drive them by name.

## Phase 10 — Headless rig runtime (foundation for interactive export)

- A crate (name per ADR, e.g. `motioner_rig`) that loads a compiled rig and
  evaluates it given time and inputs, with no editor or window dependency;
  the editor and the renderer use it, so there is one implementation.
- ADR for interactive export (web/wasm player, embedding API with inputs and
  events, file format) with a spike that runs the mascot headless from a
  small CLI (`motioner rig run --input mood=sad --frames 60 --out dir`).
- Backlog tasks for the wasm player, Lottie import/export and auto-rigging
  (suggest properties from part names like eye/pupil/lid/brow/mouth).

## Phase 11 — Docs, measurements, report

1. Release measurements: interpreter budget for programs without rigs,
   mascot evaluation per frame (median/p90), 50-instance preview fps, SVG
   import time for the test set, write-through latency for property edits.
2. Docs: `docs/rigs.md` (with the Rive/Lottie comparison table filled in),
   `docs/imports.md`, spec sections, `docs/agents.md` rig tools, README.
3. Tasks: one per phase with `pctl task-new`, events with
   `pctl session-log`, close what is done, backlog the rest. Update
   `.control/CONTEXT.md`.
4. Final report **in Spanish** at `out/reports/PHASE-16-REPORT.md`: what
   changed per phase, the mascot comparison with screenshots, what is at
   Rive parity, what is better, what is deferred and why, ADRs and
   dependencies, performance numbers, gate results, free space on C:.
