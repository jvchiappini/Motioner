---
actualizado: 2026-11-11
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## Prompt 16 status (rigs, symbols, state machines)

Session `S-2026-11-11-001`. **No commit**: the reviewer commits. Report
`out/reports/PHASE-16-REPORT.md` (Spanish). Tasks T-RIG-0001..0016;
ADR D-0051..D-0055; **no new external dependency**.

Delivered:

- `use(path)` (D-0051): SVG symbol, `.motion` module namespace, raster /
  audio / video / font definitions; metadata; sandbox; cycles; injected
  `AssetResolver` (`run_with_assets`). `motioner_io::ProjectAssets` is the
  file-system resolver; the CLI render/still/info paths use it. Modules
  import top-level names into the shared global scope (v0, documented).
- SVG reader in `motioner_io/src/svg.rs` (zero deps, D-0054): groups,
  paths with arcs to cubics, shapes, transforms flattened, paint, opacity,
  gradients, defs/use, clipPath -> D-0033 mask, viewBox. Every id is a part.
- `motioner_core::symbol` (`Symbol`, `SymbolPart`, `SymbolShape`).
- `motioner_rig` crate (D-0055, core + eval only): typed properties,
  channels, mirrors, driver IR (`driver.rs`), poses/clips/blinks, machine
  (`machine.rs`), aims, `bake`/`evaluate` (`eval.rs`).
- DSL scene child module `crates/motioner_dsl/src/scene/rigs.rs`: symbol
  instantiation (anchors, clip masks), `rig()`, `prop`, `key`/`set`,
  `mirror`, `driver` compiler, `pose`, `clip`, `play`, `input`, `state`,
  `transition`, `fire`, `aim`, `blink`, and the **bake** in `into_document`.
- **Bake-on-build** is the model (D-0052): rig output is written as node
  keys per frame with hold-before-change keys. Opacity is always written
  on the first touched frame (otherwise the evaluator's hold-first-key
  rule shows a later value early). Pose tracks start from the value the
  previous tracks produced; clips do not override a property before its
  first key; `Pose.reset` clears omitted properties (the HTML `expand()`).
- Mascot: `examples/rig/mascot.motion` (21 props, 6 drivers, 17 presets,
  2 clips, mood machine, blink, aim) and `examples/rig/mascot_demo.motion`
  (15 s, 1080p60). Screenshots in `out/reports/phase16/`.
- `docs/rigs.md` (Rive/Lottie comparison), `docs/imports.md`, spec §7.9
  and §10, README pointer.

Measured (release): mascot.svg import 0.10 ms; demo build+bake 31 ms for
901 frames (0.035 ms/frame, 4081 transform keys); 121-frame 1080p60 render
1.7 s including process + GPU + encode.

## Left (tasks)

- T-RIG-0014: resolver + rig manifest in the editor session (preview,
  query, edit, corpus, bake cannot compile `use()` today; `motioner_ui`
  must not depend on `motioner_io`, so the host injects it). Also the
  write-through corpus for rig property/pose/clip/machine edits.
- T-RIG-0013: bones, skinning, mesh/lattice, blend shapes, IK, follow
  path, spring; **path-geometry drivers** need a keyable geometry channel.
- T-RIG-0009: editor rig panel, graph view, timeline rows.
- T-RIG-0016: demo goldens + corpus/visual comparison.
- T-RIG-0015: wasm player, binary rig format, Lottie import/export,
  auto-rigging.

Known fidelity gaps of the mascot vs the HTML `apply()`: lids slide instead
of re-deriving arcs, mouth scales an authored lens (no curvature), no
perspective/skew or movable corners, no back-facing turn.

## Write-through essentials (unchanged)

Strategy ladder: private literal -> loop guard -> call site (argument,
returned node, parameter promotion) -> local assignment -> fallback
override. Every edit is verified by re-running; undo stores source +
overrides. `motioner_dsl` owns planner/printer/lint (strings); writes are
in `motioner_ui`/`motioner_app`.

## Project essentials

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu), branch `feat/m1-foundations`. Program-first
MDF builds the composition; scope/decisions are REF-0001..0004 and
D-0010..D-0055. Code is written by opencode with DeepSeek V4.1 Flash;
Claude is architect/auditor; the user speaks Spanish.

- CLI: `render`, `still`, `info`, `bake`, `preview --control`, `ctl`,
  `new`, `import`, `backup`, `migrate`, `query`, `edit`, `fmt`, `lint`,
  `trace export|replay`, `corpus check`, `tools --schema`, `tool`.
- Editor: projects, assets, timeline, graph editor, effects/masks/
  gradients/blend modes, typography (Inter), images, video proxy/cache,
  audio mixer, autosave/backup/recovery, write-through gestures,
  overrides, code panel, traces.
- Std CLI surface: `theme`, `use_theme`, `token`, `beat`, `bar`, `scene`,
  `comp_size`, `row`, `column`, `grid`, `stack`; node methods `place`,
  `bounds`, `t`.

## Gotchas

- **Host memory**: GUIMaker/FerrousEngine may build in parallel; pagefile
  commit can run out (`E0786`, 1455). Use `-j 1..2`; free space ended at
  13.7 GB. Stop below 5 GB.
- `cargo` with `-j 2` sometimes reports "can't find crate" under memory
  pressure; retry with `-j 1`.
- PowerShell 5.1 drops unescaped `"` in native args; to pass JSON use
  `--args '{\"frame\":30}'`. `Set-Content` has no `-NoNewline`.
- **PowerShell 5.1 `Set-Content -Encoding utf8` writes a BOM and mangles
  UTF-8 read as ANSI.** Use `[System.IO.File]::WriteAllText` or Python for
  files with non-ASCII (module accents).
- MDF has no `is`, dicts, `**kwargs` or `while`. Use `== None`.
- `scene.t(offset)` takes one argument (scene-local time).
- Clippy pedantic: new numeric modules carry documented `#[allow]` for
  pixel/sample casts.
- Windows ffmpeg lacks `-pattern_type glob`; use `seq%03d.png`.
- GPU tests serialize through `motioner_testkit::gpu::lock()`.

## Engine

Pin `7e95367` on `feat/pure2d-text-and-readback`; dev clone `6a30e7d`,
`e482b3c`, `cab7035`. **No pin bump in this session.** Never edit
`engine/ferrous`. The `svgparser 0.8.1` future-incompat warning comes from
the engine and predates this work.

## Goldens

All previous goldens unchanged and green. Prompt 16 deliberately did not
touch `motioner_eval`, `motioner_render` or `motioner_core` rendering paths,
so goldens could not move. Mascot screenshots are report artefacts, not
goldens (T-RIG-0016).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.
Cargo needs `-j 1..2`; `CARGO_INCREMENTAL=0`.

## Last session

`S-2026-11-11-001` (prompt 16): imports, SVG symbols, rigs, drivers,
poses/clips, state machines, `motioner_rig`, mascot demo, docs, report.
Gate green (fmt, clippy workspace -D warnings, workspace tests, rustdoc
-D warnings, boundaries, machete, pctl validate, doc-check-refs); no
commit.

