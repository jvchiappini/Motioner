---
actualizado: 2026-11-07
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## Prompt 13b status (C0-C6 delivered, leftovers tracked)

Prompt 13 is finished through phase 11 with the 13b prompt. **No commit**:
the reviewer commits. Session `S-2026-11-07-002`. Report
`out/reports/PHASE-13B-REPORT.md` (Spanish).

### C0 — debts from phases 1-6

- **Prelude cost fixed.** The library AST is parsed once per process
  (`prelude::parse_library` + `OnceLock`) and the interpreter reads its
  definitions through `prelude::library_functions()` (a shared
  `HashMap<&'static str, Arc<FunctionDef>>`) instead of executing the prelude
  block or copying it into the global scope. The AST uses `Arc` (not `Rc`)
  so a parsed program can be shared; `Value` lists stayed `Rc`.
  Release, `showcase_pro`, 200 runs: median **1.275 ms with the prelude** vs
  **1.289 ms without** (parse 0.515 vs 0.476 ms). Prelude cost ≈ 0.04 ms.
- `ParsedProgram` + `execute_parsed_with_metrics` are public for tools and
  benchmarks (same-thread variant; `run` keeps the 64 MiB interpreter
  thread).
- `count_up(to, at, duration, from, steps, prefix, suffix)` added: one text
  layer per step, `hold` opacity keys, capped at 60 layers. GIF:
  `out/std/presets/count_up.gif`, `docs/std/presets/count_up.gif`.
- Planner corpus cases added for enter/exit/emphasis/reveal and two
  components; they resolve to a local assignment / call result and keep the
  `fill="accent"` token.
- D-0039 addendum with final defaults, theme choices, count_up deviation and
  scene note.

### C1 — crates

- `motioner_analysis` (pure; core + eval + dsl origins) and
  `motioner_tools` (catalogue + executors). D-0049 and D-0050 (serde_json).
  Boundary rules updated; architecture entries `parcial`.

### C2 — design analysis

- `describe_frame`, `hit_test`, `sample`, `code_for_node`/`node_for_code`,
  `motion_report`, `layout_report`, `legibility_report`, `check_design`.
- 14 rules in `rules.rs`, documented in `docs/design-rules.md`, with
  bad/good fixtures in `examples/design_defects/<rule>/`.
- Every preset/component passes `check_design` on all ten themes in tests;
  the visual filmstrip matrix remains (task T-DSL-0025).

### C3 — render tools

- `motioner_render::tools`: ImageFrame ops, overlays as a separate synthetic
  document pass, strip/sheet/zoom/compare/diff regions, bounded image cache,
  `TextlessFrame` for contrast. `render_variants` is harness-only.

### C4 — media analysis

- `analyze_image` (k-means palette, saliency, empty regions), `analyze_video`
  (cuts, motion, calm), `analyze_audio` (onsets, tempo refined by the beat
  grid, beats, downbeats, sections, loudness) with generated-truth tests.

### C5 — tool runtime

- `motioner_tools::catalogue::TOOLS` (~47) merged by
  `motioner_app::tools::schema` with the command/code tools (version 2).
- `motioner tool <project|file> <name> --args JSON [--out DIR]` and
  `--batch FILE.jsonl`; `AppHost` drives the same `Editor`.
- Contract tests: schema completeness, result shapes, typed `requires_harness`,
  GPU render tools.

### C6 — examples, docs, measurements

- `examples/kinetic_title.motion`, `product_teaser.motion`,
  `data_story.motion` with `.prompt.md`; zero `check_design` errors.
- `docs/std.md`, `docs/design-rules.md`, `docs/tools.md` updated.
- Measurements: `check_design` on `showcase_pro` 3.49 s (240 sampled frames,
  9 off-screen findings, intentional entrance moves); render tools 1.0-7.1 s
  including process + GPU start; media 0.4-0.9 s.

### Left (tasks)

- T-DSL-0025: theme × preset filmstrips, bounds parity, labelled scene
  ranges, example goldens.
- T-AI-0004: render_variants on throwaway copies and approved export wiring.
- `off_screen` currently flags intentional entrance moves from an edge;
  revisit before using it as a corpus gate.

## Write-through essentials (unchanged)

Strategy ladder: private literal → loop guard → call site (argument, returned
node, parameter promotion) → local assignment → fallback override. Every edit
is verified by re-running; undo stores source + overrides. `motioner_dsl`
owns planner/printer/lint (strings); writes are in `motioner_ui`/`motioner_app`.

## Project essentials

Motioner is an AI-first motion-graphics editor and NLE in Rust on the custom
FerrousEngine (wgpu), branch `feat/m1-foundations`. Program-first MDF builds
the composition; scope/decisions are REF-0001..0004 and D-0010..D-0050. Code
is written by opencode with DeepSeek V4.1 Flash; Claude is architect/auditor;
the user speaks Spanish.

- CLI: `render`, `still`, `info`, `bake`, `preview --control`, `ctl`, `new`,
  `import`, `backup`, `migrate`, `query`, `edit`, `fmt`, `lint`,
  `trace export|replay`, `corpus check`, `tools --schema`, `tool`.
- Editor: projects, assets, timeline, graph editor, effects/masks/gradients/
  blend modes, typography (Inter), images, video proxy/cache, audio mixer,
  autosave/backup/recovery, write-through gestures, overrides, code panel,
  traces.
- Std CLI surface: `theme`, `use_theme`, `token`, `beat`, `bar`, `scene`,
  `comp_size`, `row`, `column`, `grid`, `stack`; node methods `place`,
  `bounds`, `t`.

## Gotchas

- **Host memory**: GUIMaker/FerrousEngine may build in parallel; pagefile
  commit can run out (`E0786`, 1455). Use `-j 1..2`; free space ended at 5.3 GB.
  Stop below 5 GB.
- `cargo` with `-j 2` sometimes reports "can't find crate" under memory
  pressure; retry with `-j 1`.
- PowerShell 5.1 drops unescaped `"` in native args; to pass JSON use
  `--args '{\"frame\":30}'`. `Set-Content` has no `-NoNewline`.
- MDF has no `is`, dicts, `**kwargs` or `while`. Use `== None`.
- `scene.t(offset)` takes one argument (scene-local time).
- Clippy pedantic: new numeric modules carry documented `#[allow]` for
  pixel/sample casts.
- Windows ffmpeg lacks `-pattern_type glob`; use `seq%03d.png`.
- GPU tests serialize through `motioner_testkit::gpu::lock()`.

## Engine

Pin `7e95367` on `feat/pure2d-text-and-readback`; dev clone `6a30e7d`,
`e482b3c`, `cab7035`. **No pin bump in this session.** Never edit
`engine/ferrous`.

## Goldens

All previous goldens unchanged and green. New example renders (not tests):
`out/reports/phase13b/kinetic_title_f120.png`
(1733522C...F3), `product_teaser_f120.png` (225022C5...95),
`data_story_f240.png` (893E6949...C5).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.
Cargo needs `-j 1..2`; `CARGO_INCREMENTAL=0`.

## Last session

`S-2026-11-07-002` (prompt 13b C0-C6): prelude cache, count_up, analysis
crate, render tools, media analysis, tool runtime/CLI, three std examples,
docs, release measurements. Gate green (fmt, clippy workspace, 59 test
binaries, rustdoc -D warnings, boundaries, machete, pctl validate,
doc-check-refs); no commit.
