# opencode prompt 12b — Motioner: finish write-through (structural gestures, override visibility, code panel, planner corpus, edit traces, corpus tooling and tool schema, docs)

You are the orchestrator of a very long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`, HEAD `a82c8a2`). This prompt **finishes**
`opencode-prompt-12-code-write-through.md`: phases 0.5-7 of that prompt are
committed (`38d0086` code, `a82c8a2` control). Read prompt 12 in full first:
its ground rules (section 0), phase gate, stop conditions and test strategy
apply here unchanged, and its phases 6, 8, 9 and 10 are the specification
of what this prompt completes. Where this prompt and prompt 12 disagree, this
prompt wins.

**Contracts:** `.control/decisions/D-0036.md` (write-through, traces §6),
D-0022 (overrides), D-0024 (command layer), D-0027 (hybrid bake and
origins). Read `.control/CONTEXT.md` section "Prompt 12 status
(2026-11-05)" and `out/reports/PHASE-12-PHASES-5-7-REPORT.md` for the state
you start from.

**Running in parallel with another job.** Prompt 15 (GUIMaker) runs at the
same time in `C:\Users\jvchi\CARPETAS\GUIMaker` and the FerrousEngine dev
clone. Therefore:

- Do **not** touch `C:\Users\jvchi\CARPETAS\GUIMaker`, the FerrousEngine dev
  clone, `engine\ferrous` or the engine pin in `Cargo.toml`/`Cargo.lock`.
- The other job also builds; **check free space on C: before every phase**
  and stop if it is below 5 GB (do not delete anything outside this repo's
  `target/`; inside it you may remove debug `.pdb` and `incremental/`).
- Prompt 15 phase 10 will later port Motioner's UI to GUIMaker. Keep every
  new UI piece of this prompt (overrides chip and list, code panel, settings
  row) in **its own small module** under `crates/motioner_ui/src/` with the
  logic (model, list, diff highlight ranges) separated from the drawing, so
  the port only replaces the drawing code.

Quality bar: top-tier studio. Zero duplication, rustdoc on public items,
typed errors, no panics on runtime paths, regression tests that fail without
the change, no warnings. Work phase by phase; run the gate after each phase.
If the job must be split across runs, stop cleanly at a phase boundary and
report. Do not commit, stage, stash, reset, checkout, rebase or push: the
reviewer commits.

Phase gate (same as prompt 12): `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`, `pctl doc-check-refs`.

Order and parallelism:

```
B1 ──► B2 ──► B3 ──► ┬─ B4 (traces)        ─┬─► B6 (docs, measures, report)
                     └─ B5 (corpus, schema) ─┘
```

B4 and B5 may run as two subagents at the same time: they touch different
modules. Each adds its subcommand in its own file under
`crates/motioner_app/src/`; the orchestrator alone wires them into the
command-line dispatch in `main.rs` and resolves conflicts.

---

## Phase B1 — Structural gestures in the command layer (closes T-DSL-0012)

The planner (`motioner_dsl::structural::plan_structural`) and the sink
(`Editor::execute_structural`) exist; no gesture reaches them yet.

- Add D-0024 `Command` variants (text syntax, parser, printer, help,
  examples in `command/examples.rs`) for: add layer (every `LayerKind`, with
  optional name and parent), delete, duplicate, reorder (to front/back,
  forward/backward, to index), reparent (into group / out of group), group
  and ungroup a selection, and place asset (image/audio/video from the
  project's assets). Existing `Rename`, `AddKeys`, `DeleteKeys` and key
  commands keep their meaning; route them through the structural planner
  where it applies.
- The commands work identically from the window, `motioner edit` (with
  `--overrides` and `--dry-run`) and the live control endpoint. Parity tests:
  code sink and override sink produce the same document for every new
  command; where the override sink cannot express a structural change
  (adding a node), document why and make the command refuse with a typed
  error in `--overrides` mode instead of half-applying.
- Window: Add Layer menu (and the shortcuts already used by the editor for
  delete/duplicate, e.g. Delete, Ctrl+D, Ctrl+G / Ctrl+Shift+G,
  Ctrl+] / Ctrl+[), drag in the outline to reorder and reparent, drag from
  the Assets panel onto the canvas or the outline to place an asset. The new
  layer is selected after the write.
- Every gesture is one program write and one undo entry; undo restores the
  exact bytes. Fallback cases (loops, functions, non-contiguous blocks, no
  z-order API) stay fallbacks with a reason; do not invent language features
  to avoid them. List them for the ADR addendum in B6.
- Tests: command parse/print round-trip, CLI end-to-end on a temp copy of
  an example, live control end-to-end, undo exact bytes, outline drag model
  (reorder/reparent decisions) as pure functions.
- Screenshots: `out/ui/p12b-01-add-layer.png`,
  `out/ui/p12b-02-outline-reparent.png`.
- Close T-DSL-0012 with `pctl`.

## Phase B2 — Fallback visibility, code panel, live diff, edit-mode setting (rest of prompt 12 phase 6)

None of these exist today; build them as specified in prompt 12 phase 6.

- **Overrides chip.** A status chip "N edits as overrides" (hidden when
  N = 0) opens a list: one row per override with the command, the node, the
  fallback reason from `CommitSummary`, and a "Convert to code" action that
  re-plans it with the current program (success removes the override in the
  same undo entry; failure keeps it and shows the new reason). "Convert all"
  does the same for every row in one undo entry.
- **Bake on open.** When a project or file with a non-empty
  `<name>.overrides.motion` is opened, offer once (non-modal banner) to
  convert the overrides to code with the existing hybrid bake; show how many
  converted and how many remain, with reasons.
- **Code panel** (read-only is acceptable): shows the program text with line
  numbers, scrolls to and highlights the range written by the last gesture
  (insertions and replacements; deletions as a marker), toggled from the
  View menu and a shortcut. The highlight ranges come from the applied code
  edit, not from re-diffing text in the drawing code.
- **Live control**: the result of an edit request includes the code diff
  (unified, project-relative path), the strategy per command and any
  fallback reasons, as already printed by `motioner edit --dry-run`. Reuse
  the same formatter; one implementation.
- **Edit mode setting** per project in `editor.motion`
  (`edit_mode = "code" | "overrides"`, default code), read on open, written
  atomically on change, exposed as a settings row. CLI flags override it for
  one run. Old projects without the key read as code.
- **File watcher conflict**: verify (and if missing, implement and test)
  the prompt 12 rule: an external change while a gesture is in progress
  wins; the in-memory gesture is dropped, the program re-runs, the selection
  is kept by identity, and a notice explains what was discarded.
- Tests for every bullet (models as pure functions, the CLI and live
  results end to end). Screenshots: `p12b-03-overrides-chip.png`,
  `p12b-04-code-panel.png`, `p12b-05-bake-banner.png`.

## Phase B3 — Planner corpus and property test (prompt 12 test strategy)

The test strategy of prompt 12 was not built as specified. Build it now; it
is the safety net for B4 traces and for the future model.

- `crates/motioner_dsl/tests/write_through/<case>/` with `before.motion`,
  `command.txt` (D-0024 text), `after.motion` (exact expected text) and
  `strategy.txt`. One data-driven test runs every case. Cover every
  strategy, every refusal (expected fallback reason instead of `after`), and
  the tricky layouts: CRLF, tabs, comments inside calls and inside `def`
  headers, nested loops, functions called from loops, nodes inside groups,
  defaults, structural commands from B1. Move the existing ad-hoc planner
  cases only when it removes duplication; do not lose any assertion.
- A deterministic property test (seeded, no new dependency unless an ADR
  says so) runs random command sequences on every example and checks:
  (1) the program re-runs without warnings, (2) the document equals the
  overrides path, (3) undo of the whole sequence returns the original bytes,
  (4) `fmt --check` still passes on the result, (5) lint reports no new
  warning introduced by the planner. Keep its runtime under 20 s in debug.
- Fix every planner bug the corpus or the property test finds, each with
  its case in the corpus.

## Phase B4 — Edit traces (prompt 12 phase 8, D-0036 §6)

Implement prompt 12 phase 8 exactly: trace record schema (versioned,
reserved agent fields for conversation and tool calls), one JSONL file per
day under `<project>/traces/` (`motioner_io::TRACES_DIR` already exists),
hand-written writer with correct escaping and a parser for tests, recording
rules (on in debug builds and with `MOTIONER_TRACES=1`; off by default for
users, project setting with a clear explanation in the settings UI; nothing
is ever uploaded), `motioner trace export` (concatenate, deduplicate by
(before, command) hash, drop records whose replay fails, `--strategy`,
`--since`) and `motioner trace replay` with a small committed fixture used in
CI. Record structural commands from B1 and override conversions from B2
too. Recording must not add measurable latency to a commit (write after the
undo entry is pushed; measure). Document the schema in `docs/traces.md`.

## Phase B5 — Corpus tooling and tool schema (prompt 12 phase 9)

Implement prompt 12 phase 9 exactly: `motioner corpus check <dir>` with the
manifest (status, duration, layer count, features from an AST feature
detector, render times, rejection reasons), the `<name>.prompt.md` seed
briefs (English) for every example, and `motioner tools --schema` generated
from the command table (including every B1 command) plus the code tools
(`read_program`, `apply_code_edit`, `run`, `query`, `render_still`), with a
test that fails when a command is missing from the schema. Prompt 13 will
extend this schema with its tool runtime: keep the generator a single
function over a list of tool descriptors so prompt 13 adds descriptors, not
a second generator. **No LLM calls, no keys, no network.** Document in
`docs/corpus.md`.

## Phase B6 — Docs, measurements, ADR addendum, report (prompt 12 phase 10)

Do prompt 12 phase 10 in full, covering phases 0.5-7 and B1-B5:

1. One release build: interpreter cost of origins v2 vs `559affb` baseline
   numbers in CONTEXT, plan+verify latency per strategy, commit latency
   during a drag on `showcase_pro.motion` (with traces on and off), corpus
   check throughput.
2. Docs: `spec/mdf-v0.md`, `docs/agents.md`, `docs/write-through.md` (the
   strategy ladder with before/after examples taken from the B3 corpus for
   every strategy and every refusal), `docs/traces.md`, `docs/corpus.md`,
   root `README.md`.
3. ADR addendum to D-0036: call-site rule, naming rules, `fmt` style
   choices, structural fallbacks that remain, the watcher conflict rule,
   trace recording defaults.
4. Tasks: one task per phase with `pctl task-new`, `pctl session-log`
   events, close what is done; backlog what is left, including the agent
   panel (INITIATIVE-0014, prompt 14) and the training pipeline
   (generation with Claude Fable 5, filtering with `corpus check`,
   distillation, evaluation). Update `.control/CONTEXT.md` with a
   "Prompt 12 status (final)" section and trim the older prompt 12 sections
   to what is still useful.
5. Final report **in Spanish** at
   `out/reports/PHASE-12B-REPORT.md`: what changed per phase, strategies
   with real before/after snippets, fallback cases that remain and why,
   ADRs, screenshots, performance numbers, gate results, free space on C:,
   and what is left (prompt 13 is next).
