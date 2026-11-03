# opencode prompt 12 — Motioner: the UI writes code (write-through, smart loop and function edits, fmt/lint, edit traces, tool schema)

You are the orchestrator of a very long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`). Run this prompt **after**
`opencode-prompt-11-projects-assets-media.md`. FerrousEngine work is not
expected; if it appears, follow the engine rules of prompt 11 (dev clone at
`C:\Users\jvchi\CARPETAS\FerrousEngine`, generic features only, documented,
pin bumped by the reviewer).

**Read `.control/decisions/D-0036.md` first: it is the contract for this
prompt.** Also read D-0022 (overrides), D-0024 (command layer) and D-0027
(hybrid bake and value origins); this prompt builds on all three.

Why this matters: today every UI gesture becomes an entry in
`<name>.overrides.motion`. From now on **every UI edit writes MDF code
directly into the program**, as a good motion designer would have written
it, and the overrides file is only a fallback. That code, paired with the
action that produced it, is the training material for a very small model
specialised in motion design (a frontier model generates the base corpus;
the editor and future in-app agent produce real edits). Code quality is
therefore a product requirement: generated code must be indistinguishable
from careful hand-written MDF.

Example of the target behaviour. The user selects the fourth circle of a
loop and makes it red:

```python
for i in range(5):
    c = circle(40, fill="#ffffff")
    c.pos = (200 + i * 120, 540)
    if i == 3:
        c.fill = "#ff4d4d"
```

Not an override, not a provenance key, not a block at the end of the file.

Quality bar: top-tier studio. Zero duplication, rustdoc on public items,
typed errors, no panics on runtime paths, regression tests that fail without
the change, no warnings. Work phase by phase; run the gate after each phase.
If the job must be split across runs, stop cleanly at a phase boundary and
report.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the repository. Never edit `engine\ferrous`. Do not
  commit, stage, stash, reset, checkout, rebase or push. The reviewer
  commits.
- `CARGO_INCREMENTAL=0`, `-j 2` (8 GB RAM). Release builds only to measure
  performance in the last phase. Stop if free space on C: drops below 5 GB.
- `cargo fmt` (never `--all`), `cargo machete crates`. Clippy pedantic
  `-D warnings`; every `#[allow]` has a reason.
- **New dependencies need an ADR first** (`pctl decision-new`). None should
  be needed: the planner, printer, formatter and trace writer are ours.
- **Safety of the user's program is the top priority.** Every write is
  atomic (temp + rename); every code edit is verified by re-running the
  program and comparing with the expected document (D-0027 exactness, by
  position); when verification fails the planner falls back to an override
  and logs why, it never guesses. Undo restores the exact previous bytes.
- **Never reformat outside the edited range.** Comments, blank lines, order
  and the user's style are preserved byte for byte outside the replacement
  ranges. Inserted lines follow the surrounding indentation (detect it) and
  the canonical style of Phase 7.
- **Determinism.** The same program, document and command always produce the
  same code edit (no hash-map iteration, no clock). All goldens stay
  byte-identical: write-through changes the text, never the pixels.
- **Command layer is law (D-0024).** No command changes meaning; only the
  sink changes (code first, override as fallback). The parity tests must
  prove that `edit` with write-through and `edit --overrides` produce the
  same document for every command.
- **Boundaries**: planner, origins, printer, formatter and linter live in
  `motioner_dsl` (engine-free, file-system-free: they take and return
  strings); file writes in `motioner_io` / `motioner_app`; the editor wires
  it in `motioner_ui`.
- Shell gotchas: Bash heredocs containing `'''` break (write files with the
  editor tool); PowerShell 5.1 drops unescaped `"` in native arguments;
  `core.autocrlf=true` (the planner must handle CRLF files: compute offsets
  on the real bytes and keep the file's line endings in inserted lines).
- Screenshots: `scripts/ui_shot.ps1 -Out out/ui/p12-NN-<what>.png
  [-Commands ...]`.

Phase gate: `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`.

Stop conditions: C: below 5 GB; a gate you cannot make green after two
honest attempts; an accepted ADR that a phase would contradict (D-0036
already supersedes the relevant parts of D-0022).

Test strategy for the whole prompt: a **planner corpus** under
`crates/motioner_dsl/tests/write_through/`: for each case a `before.motion`,
a command line, an `after.motion` (exact expected text) and the expected
strategy. Every strategy, every refusal and every tricky layout (CRLF,
tabs, comments inside calls, nested loops, functions called from loops,
nodes inside groups) has a case. A property test runs random command
sequences on the examples and checks: (1) the program re-runs, (2) the
document equals the overrides path, (3) undo returns the original bytes.

---

## Phase 1 — Origins v2

Extend `motioner_dsl::origins` so the planner knows where a node was made
and how it was reached. For every recorded origin and for every node
creation:

- **Statement spans**: the creation statement of the node, the last
  statement that wrote each property, the enclosing block (span and
  indentation).
- **Binding**: the variable the node is bound to in that block, if any (and
  whether that variable is reassigned later in the block).
- **Loop stack**: for each enclosing loop, its span, loop variable(s), the
  iterable expression, the iteration index and the iteration value (as a
  literal when it is one), and the body span.
- **Call stack**: for each enclosing function call, the call-site span, the
  function definition span, its parameters and defaults, and whether the
  function returns the node.
- Whether the expression is a literal, a parameter reference (and which
  parameter), the loop variable, or computed.

Keep the cost small (record spans and indices, not strings; intern names)
and measure the interpreter on `showcase_pro.motion` before/after; record it.

## Phase 2 — Code edit model and planner skeleton

- `CodeEdit`: an ordered set of non-overlapping byte-range replacements plus
  a human summary and the strategy used. Apply, invert (for undo), re-key
  provenance offsets (reuse the D-0027 re-keying), and compose.
- `plan(program, origins, document, change) -> Plan` where `Plan` is
  `Code(CodeEdit)` or `Fallback(reason)`; `change` is the resolved
  `EditCommand`. Verification lives in one function used by the editor, the
  CLI and tests.
- Strategy 1, **private literal** (move it here from bake, do not duplicate
  it): numbers, strings, pairs, bools, `None`, key values and eases.
- Strategy 4, **local assignment**: a static value or key on a bound node
  whose value is computed or shared, written right after the node's last
  write in its block (never in a trailing generated block). An existing
  assignment of the same property on the same node in that block is edited
  instead of adding a second one.
- Rebuild `bake` on top of the planner so there is one implementation.

## Phase 3 — Loop specialisation (the smart part)

Implement D-0036 §2.2 completely:

- Find the node's loop stack; build the guard from loop variables and
  iteration values: `if i == 3:`; nested loops `if row == 2 and col == 5:`.
- Insert the guarded statement inside the innermost loop body that contains
  the node's creation, after the node's last write in that body.
- **Merge, never duplicate**: an existing `if` with the same condition for
  the same node gets the new statement appended; the same value on several
  iterations merges into `if i in (1, 3):` (keep the tuple sorted); when a
  later edit makes the value equal again for every iteration, remove the
  guard and edit the shared literal; when a guarded statement becomes equal
  to the unguarded value, remove it (and the `if` when it empties).
- "This one / all": when both the guard and the shared-literal edit are
  exact, the command takes `scope="this"|"all"` (default `"this"`); the
  inspector shows a small toggle near the field for nodes made in loops.
  Selecting several instances of the same loop and editing them produces one
  `in` guard.
- Inline nodes without a binding get one: `c = circle(...)` replaces the
  inline expression and the original use becomes `c`. Fresh names are
  readable (`circle`, `dot`, `card` from kind or from `node.name`), unique in
  scope, and never shadow a used name.
- `for _ in range(n)` is renamed to an unused readable name (`i`, then `j`,
  `k`...). A loop over non-integer values guards on the value when it is a
  unique literal (`if name == "b":`), otherwise it is rewritten to
  `for i, x in enumerate(xs):`.
- Keys, ease edits, key moves and key removal follow the same guards.
- Delete of one instance: `if i == 3: continue` as the first statement of the
  body when that iteration creates only that node; otherwise fallback with a
  reason.
- Add to the language whatever the guards need and is missing, in the spec
  and the interpreter with tests: membership `in`/`not in` on tuples and
  lists, chained `and`, tuple unpacking in `for` with `enumerate`.

## Phase 4 — Call-site specialisation

Implement D-0036 §2.3:

- The value is a parameter of the function → pass it as a keyword argument
  at that call site (edit it when it is already passed).
- The function returns the node → bind the call result (if not bound) and
  assign after the call.
- Otherwise **promote the literal to a keyword parameter**: add
  `name=<old literal>` to the definition (a readable parameter name from the
  property: `fill`, `size`, `color`), replace the literal with the
  parameter, pass the new value at the edited call site. Other call sites
  render identically by construction; the verification proves it.
- Functions called from a loop combine both strategies. Default: bind the
  call result and add a loop guard with an assignment after the call
  (`b = badge(i)` / `if i == 3: b.fill = "#f00"`). Only when the function
  already takes the parameter, and the call has a simple argument, may the
  planner pass a conditional value instead, if MDF has a readable form for
  it. Write the rule down in the ADR addendum with examples and test each.

## Phase 5 — Structural edits

- Add layer (rect, circle, text, image, audio, video, path, group): insert a
  constructor statement with a readable name after the selected node or at
  the end of the composition body; the new layer is selected.
- Delete, duplicate (copies the statements that build the node, renaming
  the binding), rename (`node.name = "..."`, edits an existing one), reorder
  (moves statements when the order is the creation order; otherwise uses the
  z-order API if one exists, else fallback), parent/unparent in groups.
- Keys: add, move (retime), remove, change ease, stopwatch on/off (stopwatch
  off keeps the value at the playhead as a static value) all as code.
- `visible` is code; `locked`, expanded rows and selection go to the
  project's `editor.motion` (prompt 11), not the program.
- Asset placement from the Assets panel inserts `image(...)`, `audio(...)`
  or `video(...)` statements.

## Phase 6 — Editor and CLI integration

- Write-through is the default sink of `Editor::execute`: gestures patch the
  document in memory; at gesture end plan → apply → write → re-run →
  verify; one gesture is one write and one undo entry (inverse text patch +
  editor state). Measure the commit latency on the biggest example and keep
  it under 50 ms at the reference machine, or report why.
- File watcher (from prompt 11): external edits re-run the program and keep
  selection by identity; a conflict with a pending in-memory gesture is
  resolved in favour of the disk and reported.
- Fallback visibility: a status chip "N edits as overrides" opens a list
  with the reason of each and a "convert to code" action that re-plans it.
  Existing overrides files are offered for conversion on open (bake).
- A code panel (read-only is acceptable in this prompt) shows the program
  with the last edit highlighted, so the user sees what each gesture wrote.
- CLI: `motioner edit` writes code; `--overrides` keeps the D-0022
  behaviour; `--dry-run` prints the diff. The live control endpoint returns
  the code diff and strategy in its result.
- Setting "Edit mode: code (default) / overrides" per project in
  `editor.motion`.

## Phase 7 — `motioner fmt` and `motioner lint`

- A canonical printer for MDF that is idempotent and comment-preserving:
  4-space indentation, spacing around operators and after commas, double
  quotes, numbers via `format_number`, one statement per line, at most one
  blank line inside blocks and two between top-level definitions, trailing
  newline. `motioner fmt <file|project> [--check]`.
- The planner's inserted code uses the same printer for its snippets.
- `motioner lint`: diagnostics with spans and fixes for generated-code
  smells: an edited node without a binding, empty or dead guards (a value
  that equals the unguarded one), duplicated assignments of the same
  property in one block, `for _` loops whose index is used by a guard,
  unused parameters introduced by promotion, magic numbers repeated more
  than N times (suggest a variable). Lint is advisory; it never blocks
  rendering.
- Every file under `examples/` is formatted and lint-clean at the end of the
  prompt (formatting changes must not change any golden).

## Phase 8 — Edit traces (training data)

Implement D-0036 §6:

- A trace record per committed edit: schema version, timestamp (only in the
  trace, never in the document), project-relative file, program-before hash
  and text (or a reference to the previous record's after), the D-0024
  command text, the planner strategy, the code edit as a unified diff, the
  program-after hash, the frame, and the selection. Agent sessions will add
  conversation and tool-call fields later; reserve them.
- Stored as one record per line under `<project>/traces/<date>.jsonl`
  (hand-written writer with correct escaping; a parser for tests). Document
  the schema in `docs/traces.md`.
- Recording: on by default in debug builds and when the environment variable
  `MOTIONER_TRACES=1` is set; off by default for end users, with a project
  setting and a clear explanation in the settings UI. Nothing is ever
  uploaded.
- `motioner trace export <project|dir> --out file.jsonl
  [--strategy ...] [--since ...]`: concatenates, deduplicates by
  (before, command) hash and drops records whose replay fails.
- `motioner trace replay <file.jsonl>`: re-applies each record to its
  before-program and checks that it produces the after-program; used in CI on
  a small committed fixture.

## Phase 9 — Corpus tooling and tool schema (foundations for the model and the agent panel)

- `motioner corpus check <dir>`: validates a directory of generated `.motion`
  programs (from a frontier model or anywhere): parses, runs without
  warnings or overrides, renders stills at N frames headlessly, rejects
  blank or near-blank frames, checks determinism (two renders equal), runs
  `fmt --check` and lint; writes a manifest with per-file status, duration,
  layer count, features used (from a feature detector over the AST) and
  render times. Exit code non-zero only for tool errors, not for rejected
  files.
- Seed set: for every file in `examples/` add a sibling `<name>.prompt.md`
  with a short natural-language brief of what it shows (in English), so
  examples become (prompt → program) pairs.
- `motioner tools --schema`: prints the complete tool list for agents as one
  JSON document: every D-0024 command (name, description, parameters with
  types, example) plus code tools (`read_program`, `apply_code_edit`,
  `run`, `query`, `render_still`). Generated from the command table, never
  hand-maintained; a test fails when a command is missing. This is the
  schema the future in-app agent harness and the training data will share.
- Do **not** call any LLM API in this prompt (no keys, no network).

## Phase 10 — Docs, measurements, report

1. Release build once: interpreter cost of origins v2, plan+verify latency
   per strategy, commit latency during a drag on `showcase_pro.motion`;
   record numbers in the report and in `.control/CONTEXT.md`.
2. Docs: `spec/mdf-v0.md` (new language features), `docs/agents.md`
   (write-through results, `scope`, `--overrides`), `docs/write-through.md`
   (the strategy ladder with before/after examples for each strategy and each
   refusal), `docs/traces.md`, `docs/corpus.md`, root `README.md`.
3. ADR addendum to D-0036 with the decisions taken during implementation
   (call-site rule from Phase 4, naming rules, style choices of `fmt`).
4. Tasks: one task per phase with `pctl task-new`, events with
   `pctl session-log`, close what is done; backlog what is left, including
   the agent panel (INITIATIVE-0014: harness with conversation, provider
   abstraction per D-0012, tools from `motioner tools --schema`, edits
   through write-through, traces with conversation fields) and the training
   pipeline (generation with Claude Fable 5, filtering with `corpus check`,
   distillation into a small model, evaluation).
5. Final report (in Spanish): what changed per phase, strategies with real
   before/after snippets, fallback cases that remain and why, ADRs,
   screenshots, performance numbers, gate results, free space on C:, and what
   is left.
