# opencode prompt 16b — Motioner: finish prompt 16 (review fixes, editor use() resolver, rig manifest and tools, rig write-through, geometry channel, native deformers and constraints, exact mascot fidelity, goldens, rig CLI)

You are the orchestrator of a very long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`). This prompt **finishes**
`opencode-prompt-16-rigs-symbols-state-machines.md`. Its phases 0-4, 6, 7, 9
and parts of 10-11 are committed (see `git log`: "prompt 16" commits). Read
prompt 16 in full first: its reference file, ground rules (section 0), phase
gate, stop conditions and contracts (D-0024, D-0033, D-0036, D-0051..D-0055)
apply unchanged, and its phases 5, 8 and 10 are the specification this
prompt completes. Where this prompt and prompt 16 disagree, this prompt
wins. Read `out/reports/PHASE-16-REPORT.md`, `docs/rigs.md` and the
`.control/CONTEXT.md` prompt 16 section for the state you start from.

Backlog tasks to close: T-RIG-0013 (deformers, geometry channel),
T-RIG-0014 (editor resolver, manifest, write-through), T-RIG-0016 (goldens,
comparison matrix), and the `motioner rig run` spike from prompt 16 phase 10.
T-RIG-0009 (rig panel drawing) and T-RIG-0015 (wasm player, Lottie,
auto-rigging) stay backlog except where a phase below says otherwise.

Prompt 15 (GUIMaker) has **not** run yet. Do not touch
`C:\Users\jvchi\CARPETAS\GUIMaker`. Engine work (textured mesh, phase E5)
goes to the FerrousEngine dev clone at `C:\Users\jvchi\CARPETAS\FerrousEngine`
as a **generic** feature documented in the engine docs (rustdoc,
`crates/<crate>/docs`, `mkdocs.yml` nav, tests, an example); never edit
`engine\ferrous`; the reviewer pushes and bumps the pin.

Do not commit, stage, stash, reset, checkout, rebase or push: the reviewer
commits. Quality bar: top-tier studio; zero duplication, rustdoc on public
items, typed errors, no panics on runtime paths, regression tests that fail
without the change, no warnings.

**Work one phase at a time and STOP after each phase** with the gate green
and a short phase report (what changed, numbers, gate results, free space on
C:). The user will reply "continúa con la fase N". If a phase must be
split, stop cleanly at a sub-step boundary and say so.

Gate (unchanged from prompt 16): `cargo fmt -- --check` (never `--all`: it
reaches the engine submodule), `cargo clippy --workspace --all-targets -j 2
-- -D warnings`, `cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`, `pctl doc-check-refs`, plus the
new encoding check from E0. Engine clone, when touched: its own gate.

Order:

```
E0 fixes ─► E1 resolver + manifest ─► E2 rig write-through + tools
        ─► E3 geometry channel ─► E4 deformers/constraints (shapes)
        ─► E5 textured mesh (engine) ─► E6 exact mascot + goldens + matrix
        ─► E7 rig CLI, panel logic, measurements, docs, report
```

E4 and E5 may run as two subagents (E4 in `motioner_rig`/`motioner_eval`,
E5 in the engine clone + `motioner_render`); the orchestrator alone edits
shared files (`Cargo.toml`, `lib.rs` module lists,
`scripts/check_boundaries.py`, `main.rs` dispatch) and integrates.

---

## Phase E0 — Review fixes and missing measurements

The reviewer found these in prompt 16's output:

1. **Encoding.** `Cargo.toml`, `crates/motioner_dsl/Cargo.toml`,
   `docs/rigs.md` and `.control/architecture/motioner_rig.md` were written
   with a UTF-8 BOM and CRLF (PowerShell `Set-Content`/`Out-File`). The
   reviewer stripped them. Never write files with a BOM or CRLF; use the
   file tools or `[IO.File]::WriteAllText(path, text, (New-Object
   Text.UTF8Encoding $false))`. Add a check (in `scripts/check_boundaries.py`
   or a small `scripts/check_encoding.py` wired into the gate) that fails on
   a BOM or CR in tracked text files outside `engine/`, with a test.
2. **Missing measurements from prompt 16 phase 11** (release, reference
   machine, median/p90 over ≥ 20 runs): interpreter budget for programs
   without rigs (must stay within +7 % of the baseline in CONTEXT; if not,
   fix it here — e.g. the `use()`/rig paths must cost nothing when unused),
   load cost of a scene with 50 mascot instances (bake time, key count,
   memory) and its preview fps, and write-through latency for a property
   edit (after E2; record "pending E2" here).
3. **Stroke under non-uniform scale.** In `mascot_enojado_f160.png` the
   mouth is scaled and its stroke collapses to a hairline. Until E3 replaces
   scaling with geometry, strokes on rig parts must keep their authored
   width (SVG `vector-effect: non-scaling-stroke` semantics, opt-in per part
   or per symbol, honoured by the SVG parser). Golden test.
4. T-RIG-0011's acceptance text says the CLI is done while it is not;
   correct the task file (E7 delivers it).

## Phase E1 — Asset resolver in the editor session and the rig manifest (T-RIG-0014 part 1)

- The editor session, preview, query, edit, corpus and bake paths resolve
  `use()` of SVG and `.motion` through the **same** `AssetResolver`
  (`motioner_dsl::assets`). `motioner_ui` must not depend on `motioner_io`:
  the host (`motioner_app`) injects the resolver into the session at
  construction; tests use an in-memory resolver. No second resolution path,
  no `NoAssets` fallback in the editor once a project is open.
- **Rig manifest**: a typed, serialisable description of every rig instance
  in a document (properties with type/range/step/group/mirror, poses,
  clips, layers, machine states/transitions/inputs/events, addressable
  parts with their ids) produced by the compiler and stored on the
  document, versioned, with a stable order. ADR if it changes the document
  model. `motioner query rig <instance>` prints it; schema in
  `motioner tools --schema`.
- Tests: opening `examples/rig/mascot_demo.motion` in a headless editor
  session yields the same document and manifest as `motioner run`.

## Phase E2 — Rig write-through, undo and agent tools (T-RIG-0014 part 2)

- Through the D-0036 planner (verify, exact undo, fallback with reason):
  set a property value, key/unkey a property, edit/save/delete a pose, edit
  clip keys/loop/speed, add/remove/retarget machine states, transitions and
  input keys. Corpus cases under `crates/motioner_dsl/tests/write_through/`
  for each edit kind, including mirrored (left/right/both) properties and
  edits inside a used `.motion` module (the edit lands in the module file,
  or falls back with a reason — decide in an ADR addendum to D-0036).
- Rig tools in the prompt 13 catalogue (family `rig`, single generator):
  `rig_list`, `rig_manifest`, `rig_set`, `rig_key`, `rig_apply_pose`,
  `rig_play_clip`, `rig_key_input`, `rig_save_pose`. Executors go through
  the same `Editor`; contract tests with an example call each on the
  mascot demo, validated against the schema.
- Fill the write-through latency measurement from E0.

## Phase E3 — Keyable geometry channel and geometry drivers

The HTML controller's `apply()` re-derives paths (lid arcs by trigonometry,
mouth curvature/open/teeth/corners, the body's 4 movable corners, bulges,
perspective and skew). Prompt 16 approximated them with transforms.

- ADR: a geometry channel on path nodes — path data (or a point list with
  fixed topology) that can be keyed and driven per frame, interpolated
  point-wise when topology matches, with a typed error otherwise.
  Evaluation stays in the pure evaluator; the bake (D-0052) writes geometry
  keys like it writes transform keys, or the ADR justifies evaluating
  geometry drivers at sample time instead (measure both; keep determinism).
- Driver IR: geometry-producing expressions (path builders: move/line/
  cubic/arc/close, `rounded_rect`, `ellipse_arc`, mirroring) with bounds and
  typed errors. Spec (§7.9/§10), printer/fmt, lint.
- Write-through for geometry keys (planner, corpus, undo).
- Golden: a test rig whose lid arc is computed exactly like the HTML's
  formula, compared point-for-point.

## Phase E4 — Native deformers and constraints on shapes (prompt 16 phase 5, T-RIG-0013)

As specified in prompt 16 phase 5, for vector shapes, each with an ADR
where it changes the model, goldens, spec, docs and write-through:

- lattice / free-form deform (2×2 with edge bends — the mascot's corners
  and bulges are the acceptance case);
- bones: hierarchy, bind pose, skinning of path points with weights
  (auto-weights by distance, editable), bone keys;
- blend shapes with a topology check and a helper that makes topology
  compatible;
- constraints: 2-bone IK with pole, follow path, copy/limit transform,
  distance, spring follow-through (reuse T-DSL-0011 springs if present;
  otherwise implement here and close that part). Aim with radius clamp
  already exists — route it through the same constraint stage.
- Evaluation order documented and tested: properties → drivers →
  deformers → constraints → draw. All of it lives in `motioner_rig` /
  `motioner_eval` (pure), one implementation shared by bake and runtime.
- Performance: the mascot with lattice + geometry drivers still evaluates
  under 0.3 ms/frame in release.

## Phase E5 — Textured mesh for images and video (engine + render)

- Generic FerrousEngine feature in the dev clone: textured triangle mesh
  drawing (UVs, per-vertex positions updated per frame, premultiplied
  alpha, works with the existing image/video texture paths and masks),
  documented and tested in the engine, with an example. Report the engine
  commit range for the reviewer to push and pin.
- Motioner: lattice, bones/skinning and blend shapes apply to `image()` and
  `video()` nodes through that mesh. Goldens for an image bent by a lattice
  and a video frame skinned by two bones. Existing goldens byte-identical.
- **Bone constraints (reviewer finding after E4).** D-0060 runs IK, copy
  and limit on part transforms after the deform stage, so a skinned limb
  cannot be bent by IK. Let IK/copy/limit/aim target `m.bone` joints too:
  order becomes properties → machine → drivers → bone constraints →
  skinning/lattice/blend → part constraints → draw (addendum to D-0060,
  one implementation, no second skeleton). Golden: an image arm skinned by
  two bones whose IK target is keyed, the elbow bends and the texture
  follows.

## Phase E6 — Exact mascot, goldens and comparison matrix (T-RIG-0016)

- Rebuild `examples/rig/mascot.motion` on the geometry channel, lattice and
  constraints so every value of the HTML's `apply()` is reproduced: exact
  lids, mouth (smile/open/teeth/corners), movable corners, bulges,
  perspective/skew, sparks/glow. Keep it shorter and clearer than the HTML.
- Reference rasters: a script (`scripts/mascot_reference.ps1` or `.py`)
  that drives the read-only HTML in headless Edge/Chrome, applies each of
  the 17 presets and clip frames, and saves PNGs to
  `out/reports/phase16b/reference/` (never modify the HTML; copy it to a
  temp dir if the script needs to inject code). Motioner renders the same
  frames; a comparison matrix (side by side + heatmap via the prompt 13b
  compare tool) with a documented per-pixel tolerance. Every preset and both
  clips within tolerance, or the report lists each remaining difference and
  its cause.
- Golden frames for a few frames of `mascot_demo.motion`;
  `motioner corpus check` on both rig examples.

## Phase E7 — Rig CLI, panel logic, measurements, docs, report

1. `motioner rig run <file> --input mood=sad --frames 60 --out dir`
   (prompt 16 phase 10 spike) on `motioner_rig`, plus `--events` to print
   fired events as JSON lines. Tests.
2. Rig panel **logic only** (T-RIG-0009 part 1): view-models and D-0024
   commands for property groups with left/right/both mirror, pose library
   (thumbnail requests through the render tools), clip list, machine graph
   model with live state, timeline rows. Headless tests. Drawing waits for
   prompt 15 (Motioner on GUIMaker); keep it out of this prompt.
3. **50-instance preview performance (reviewer finding after E0).** E0
   measured 1.5 fps at 1080p for 50 mascots (2 000 paths, 48 600 keys);
   the target is 60 fps. Profile first (`mascot_scene_bench.rs`), report
   where the time goes (evaluation, tessellation, uploads, draw calls),
   then fix it: e.g. cache tessellation keyed by path geometry + stroke
   (transform-only animation must not re-tessellate), batch draws per
   material, reuse GPU buffers across frames; generic engine parts go to
   the dev clone. Target ≥ 60 fps, or ≥ 30 fps with the remaining
   bottleneck named and backlogged. Existing goldens byte-identical.
4. Measurements (release): everything from E0.2 final values, mascot
   eval/frame before and after E3-E4, 50-instance preview fps, textured
   mesh cost, write-through latency per edit kind.
5. Docs: `docs/rigs.md` (comparison table updated: what is now at Rive
   parity or better), `docs/imports.md`, `docs/tools.md` rig tools, spec,
   README. Tasks closed with `pctl`, the rest backlogged with reasons,
   session log, CONTEXT updated.
6. Final report **in Spanish** at `out/reports/PHASE-16B-REPORT.md`: per
   phase, the comparison matrix with screenshots, ADRs, dependencies,
   numbers, gate results, free space on C:, engine commits to push, and what
   is left (T-RIG-0009 drawing after prompt 15; T-RIG-0015).
