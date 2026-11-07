---
actualizado: 2026-11-06
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## Prompt 12 status (final, 2026-11-06)

**Write-through is finished (prompt 12b, phases B1-B6).** The UI, `motioner
edit`, the live control endpoint and the future agent all write MDF code
through one planner and one sink. The overrides file is the fallback, with a
visible list and one-click conversion. Traces, the corpus filter and the tool
schema are in place. **No commit in this session** (the reviewer commits).

### B1 — Structural gestures in the command layer

- `Command` gained `add_layer`, `delete_layer`, `duplicate_layer`,
  `reorder_layer`, `reparent_layer`, `group_layers`, `ungroup_layer`
  (`crates/motioner_ui/src/command/`). Each parses, prints and round-trips.
- `motioner_dsl::structural` gained `Group`/`Ungroup`, planned as one combined
  edit; `plan_structural_with_metrics` threads text metrics through every
  verification (programs calling `text_width` no longer fall back).
- `CodeEdit::compose` fixed for insertions (offset map sign and boundary
  coverage); regression test `composition_after_an_insertion_maps_offsets_back`.
- Editor sink: one write, one undo entry, undo restores exact bytes; adding a
  layer/asset selects it. In overrides mode structural gestures are refused
  with the typed `refused` error.
- Window: Add Layer menu (rect, circle, text, path, group), Delete/Ctrl+D/
  Ctrl+G/Ctrl+Shift+G/Ctrl+Down/Ctrl+Up, outline drag (pure model in
  `crates/motioner_ui/src/arrange.rs`), asset drag onto the canvas or outline.
- Tests: 19-case corpus, editor structural tests, CLI structural tests,
  arrange unit tests. T-DSL-0012 closed.

### B2 — Fallback visibility, code panel, live diff, edit mode

- Status chip "N edits as overrides" (top bar) opens the list
  (`panels::overrides`, model in `overrides_panel.rs`); each row shows the
  command and the last known reason; Convert re-plans one entry through the
  write-through sink in one undo entry; Convert all uses `bake`.
- Open banner offers to convert a non-empty `<name>.overrides.motion`.
- Read-only code panel (View menu / Ctrl+B, `show_code(on)`), line numbers and
  the exact ranges of the applied `CodeEdit` (deletions marked); model in
  `code_panel.rs`, drawing in `panels::code`.
- `motioner edit --dry-run` and the live control reply share
  `motioner_ui::unified_diff`; an editing request replies with
  `{summary, code, strategies, overrides_kept, fallback_reasons, path, diff}`.
- `editor.motion` stores `edit_mode` (code by default) and `traces`; read on
  open, written atomically on change; `--code`/`--overrides` override it for
  one CLI run.
- Watcher: an external change during a gesture wins; the gesture is dropped,
  the program re-runs and the selection is kept by identity (test
  `an_external_change_discards_a_pending_gesture_and_keeps_the_selection`).

### B3 — Planner corpus and property test

- `crates/motioner_dsl/tests/write_through/<case>/` with 19 cases
  (`before.motion`, `command.txt`, `after.motion`, `strategy.txt`), driven by
  `crates/motioner_ui/tests/write_through.rs`. Covers private literal, local
  assignment, loop guard/key, nested loops, call argument/promotion/result,
  structural add/delete/duplicate/rename/group/loop-continue, two refusals,
  CRLF and comments. Regenerate intentionally with `UPDATE_CORPUS=1`.
- Seeded property test over nine examples: re-runs without warnings, code and
  overrides sinks agree, undo restores the original bytes, the result is
  canonical and lint did not grow (0.65 s debug).
- `examples/effects, gradients, images, masks, showcase, showcase_pro` were not
  canonical (phase 7 missed them); they are formatted now. Goldens unchanged.

### B4 — Edit traces

- `motioner_io::traces`: schema v1, hand-written JSONL writer/parser, one file
  per day under `<project>/traces/`, reserved `conversation`/`tool_calls`.
- Recording: debug on by default, release only when the project sets
  `traces=True`, `MOTIONER_TRACES=1` always wins. Window, `motioner edit` and
  the control endpoint record; writes happen after the transaction commits.
- `motioner trace export` (dedupe by before+command, replay filter, `--strategy`,
  `--since`) and `motioner trace replay`; committed fixture
  `crates/motioner_app/tests/fixtures/traces.jsonl` runs in CI.
- `docs/traces.md`.

### B5 — Corpus tooling and tool schema

- `motioner corpus check <dir> [--frames N]`: parse, run without warnings,
  fmt/lint, headless stills, reject only when every still is blank, render
  frame 0 twice for determinism, AST feature detector, `manifest.json` with
  timings/layers/reasons. Exit non-zero only for tool errors.
- `<name>.prompt.md` seed briefs for all 12 examples.
- `motioner tools --schema` generated from `CommandKind::ALL` plus
  `read_program`, `apply_code_edit`, `run`, `query`, `render_still`; a test
  fails when a command is missing. One descriptor list for prompt 13.
- `docs/corpus.md`.

### B6 — Docs, measurements, ADR, report

- Docs: `docs/write-through.md`, `docs/traces.md`, `docs/corpus.md`,
  `docs/agents.md`, `README.md`; ADR addendum in `D-0036.md`.
- Measurements (release, RTX 4050, host under a concurrent GUIMaker build):
  interpreter **1.298 ms** median / 1.665 ms p90 on `showcase_pro` (3 runs
  1.298/1.389/1.296; baseline 1.2105, +7.2 %, but `interpreter.rs`,
  `origins.rs`, `scene.rs` and `value.rs` are byte-identical to `559affb`, so
  the delta is host contention; phase 5-7 measured 1.2055, -0.4 %). Commit
  (plan+apply+write+re-run+verify) **median 11.72 ms, p90 13.37 ms** (budget
  50 ms). Corpus check: 5 programs × 3 stills in **5.65 s** whole run
  (0.61-2.17 s per file, includes GPU init).
- Gate green: fmt, clippy pedantic `-D warnings`, workspace tests (58 test
  binaries), rustdoc `-D warnings`, boundaries, machete, `pctl validate`,
  `doc-check-refs`. The debug build had to run with
  `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` because this host's
  pagefile is exhausted by a concurrent job (error 1455/STATUS 0x c0000409
  when linking test binaries with debug info); tests themselves are identical.
- Screenshots: `out/ui/p12b-01-add-layer.png`,
  `p12b-02-outline-reparent.png`, `p12b-03-overrides-chip.png`,
  `p12b-04-code-panel.png`, `p12b-05-bake-banner.png`.
- Report (Spanish): `out/reports/PHASE-12B-REPORT.md`.

### Write-through essentials (unchanged)

Strategy ladder: private literal → loop guard (`if i == 3:`, merges into
`in (1, 3)`, enumerate rewrite) → call site (argument, returned-node binding,
parameter promotion with default) → local assignment → fallback override with
a reason. Every code edit is verified by re-running; undo stores source +
overrides. `motioner_dsl` owns planner/printer/lint (strings only); writes are
in `motioner_ui`/`motioner_app`.

## Project essentials

Motioner is an AI-first motion-graphics editor and NLE in Rust on the custom
FerrousEngine (wgpu), branch `feat/m1-foundations`. Program-first MDF (Python-
like DSL) builds the composition; scope and decisions are REF-0001..0004 and
D-0010..D-0048. Code is written by opencode with DeepSeek V4.1 Flash; Claude
is architect/auditor; the user speaks Spanish.

- CLI: `render`, `still`, `info`, `bake`, `preview --control`, `ctl`, `new`,
  `import`, `backup`, `migrate`, `query`, `edit [--overrides|--code]
  [--dry-run]`, `fmt [--check]`, `lint`, `trace export|replay`, `corpus check`,
  `tools --schema`.
- Editor: projects (`<name>.motioner/`), assets registry, timeline with
  waveforms/thumbnails, graph editor, effects/masks/gradients/blend modes,
  typography (Inter), images, video proxy/cache, audio mixer, autosave/backup/
  recovery, write-through gestures, overrides list, code panel, traces.
- Keys: Space, arrows, Delete (layer when no key selected), Ctrl+D duplicate,
  Ctrl+G/Ctrl+Shift+G group/ungroup, Ctrl+Up/Down reorder, Ctrl+B code panel,
  Ctrl+E edit mode, Ctrl+T traces.

## Gotchas

- **Host memory**: another job builds GUIMaker/FerrousEngine in parallel.
  Pagefile commit can be exhausted; rustc fails with `E0786`/“paging file too
  small” (1455) or `STATUS_STACK_BUFFER_OVERRUN`. Free space: deleting
  `target/debug/**/*.pdb` frees ~9 GB and lets the pagefile grow; running the
  gate with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` avoids the
  largest link allocations. Check C: before phases; stop below 5 GB.
- PowerShell 5.1 drops unescaped `"` in native args; use `\"` in `-Commands`,
  `--select ROW` instead of `ctl select("@id")`. `ui_shot.ps1` is occasionally
  flaky about the control endpoint; starting `preview` with `Start-Process`
  and sending `ctl` manually is reliable.
- Editing `examples/*.motion` via `ctl` writes the real file; use copies under
  `out/ui/` for screenshots and remove them.
- `cargo fmt` normalises LF; files written by PowerShell need a
  `-replace "`r`n","`n"` pass (`.gitattributes eol=lf`).
- Clippy pedantic hits: `too_many_lines`, `too_many_arguments`,
  `assigning_clones`, `single_match_else`, `write_with_newline`,
  `enum_variant_names`, `cast_precision_loss`, `items_after_statements`.
- GPU tests serialize through `motioner_testkit::gpu::lock()`; the first
  parallel run after a build can fail on device memory.

## Engine

Pin `7e95367` on `feat/pure2d-text-and-readback`; dev clone `6a30e7d` (line/
polyline, in-place texture update), `e482b3c` (GuiBatch recycle), `cab7035`
(file drops). **No pin bump in this session.** Never edit `engine/ferrous`.

## Goldens

`hello_f120.png` byte-identical (SHA-256
`C2173BADD7F23933461473C5D682E40FC9FC1DCBD4EBBE10A1C7C1C7DE5CF15C`);
`showcase_pro_f180/f330/f600/f810/f1086/f1320.png`; `demo_f090/f330.png`; plus
blend/gradients/effect/mask/path/typography/images/timing stills. Formatting
the six examples in B3 did not change any golden.

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.
Cargo needs `-j 1..2`; `CARGO_INCREMENTAL=0`.

## Last session

`S-2026-11-06-001` (prompt 12b, B1-B6): structural command layer + UI, overrides
chip/list/convert, code panel, unified diff, edit mode, traces, corpus check,
tool schema, docs and measurements. Gate green; no commit. T-APP-0007..0010 and
T-DSL-0013 done; T-APP-0011 (this report) done; T-AI-0001/T-AI-0002 backlog.
