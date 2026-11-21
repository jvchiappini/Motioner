# opencode prompt 13b — Motioner: finish prompt 13 (std debts, design analysis, render tools, media analysis, agent tool runtime, std examples)

You are the orchestrator of a very long, multi-phase job in the Motioner
repository at `C:\Users\jvchi\CARPETAS\Motioner` (branch
`feat/m1-foundations`, HEAD `d13e7fa`). This prompt **finishes**
`opencode-prompt-13-motion-std-and-agent-tools.md`: its phases 1-6 (the
`std` library) are committed at `2d9db98` (code) and `d13e7fa` (control).
Read prompt 13 in full first: its contracts (D-0039, D-0040 §5-6, D-0041,
D-0043, D-0036, D-0024), ground rules (section 0), phase gate and stop
conditions apply unchanged, and its phases 7-11 are the specification this
prompt completes. Where this prompt and prompt 13 disagree, this prompt
wins. Read `out/reports/PHASE-13-REPORT.md` and the `.control/CONTEXT.md`
prompt 13 section for the state you start from. Backlog tasks to close:
T-DSL-0020 (analysis), T-DSL-0021 (render tools), T-DSL-0022 (media
analysis), T-DSL-0023 (examples/docs/report), T-DSL-0024 (library corpus,
`count_up`, theme × preset matrix), T-AI-0003 (tool runtime).

Prompt 15 (GUIMaker) may run at the same time in
`C:\Users\jvchi\CARPETAS\GUIMaker` and the FerrousEngine dev clone: do not
touch those folders, `engine\ferrous` or the engine pin; check free space on
C: before every phase (stop below 5 GB). If the pagefile runs out, build
with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

Do not commit, stage, stash, reset, checkout, rebase or push: the reviewer
commits. Quality bar: top-tier studio; zero duplication, rustdoc on public
items, typed errors, no panics on runtime paths, regression tests that fail
without the change, no warnings. Work phase by phase; run the gate after
each phase (prompt 13 gate plus `pctl doc-check-refs`). If the job must be
split, stop cleanly at a phase boundary and report.

Order and parallelism:

```
C0 ──► C1 (crate skeletons + ADRs) ──► ┬─ C2 design analysis (prompt 13 ph. 7)
                                       ├─ C3 render tools    (prompt 13 ph. 8)
                                       └─ C4 media analysis  (prompt 13 ph. 9)
                                                  ──► C5 tool runtime (ph. 10) ──► C6 examples, docs, report (ph. 11)
```

C2, C3 and C4 may run as three subagents: C2 and C4 live in different
modules of `motioner_analysis`, C3 in `motioner_render`. The orchestrator
alone edits shared files (`Cargo.toml`, crate `lib.rs` module lists,
`scripts/check_boundaries.py`, `main.rs` dispatch) and integrates.

---

## Phase C0 — Debts from phases 1-6

1. **Prelude cost.** Prompt 13 phase 1 required caching the parsed prelude;
   today it is parsed (and executed) on every run: parse 0.513 → 1.406 ms
   and execution 1.224 → 1.414 ms (+15.5 %) on `showcase_pro.motion` for
   *every* program, including ones that use nothing from `std`. Fix both:
   - parse the prelude once per process (`OnceLock`, spans already shifted
     into the library range) and share the AST;
   - avoid re-executing the prelude's definitions each run: evaluate its
     top-level once into an immutable library scope (functions, constants,
     themes) that each run reads through; user shadowing keeps working; if
     some prelude state must be per-run (current theme, tempo), make that
     explicit and small.
   Target: programs that do not use `std` within +7 % of the prompt 12
   baseline recorded in CONTEXT (execution ≈1.21 ms median); prelude cost
   under 0.2 ms per run. Measure in release with the same method as before
   and record before/after.
2. `count_up` preset (prompt 13 phase 5) with tests and a GIF.
3. Planner corpus cases under `crates/motioner_dsl/tests/write_through/` for
   every preset kind and every component (edit at the call site:
   `at=beat(2)` → `at=beat(3)`, `fill="accent"` token kept, refusal inside
   `std`) — T-DSL-0024.
4. Full theme × preset visual matrix (`out/std/matrix/<theme>.png`
   filmstrips), reviewed; fix the worst visible problems.
5. D-0039 addendum with the final preset defaults and the reasoning, theme
   choices, and whether components stay in the prelude.
6. Verify scenes appear in `outline`, `project_summary` and the timeline as
   labelled ranges (prompt 13 phase 4); finish what is missing.

## Phase C1 — Crates and decisions

ADRs (`pctl decision-new`) for the new crates `motioner_analysis`
(engine-free, file-system-free) and `motioner_tools` (catalogue +
executors), and for `serde_json` for tool I/O (short ADR; it is already in
the lock file through the engine). Create both crates with empty modules,
boundaries rules in `scripts/check_boundaries.py`, rustdoc crate pages and
`.control/architecture` entries. No algorithm code yet.

## Phase C2 — Design analysis (prompt 13 phase 7)

Exactly as specified in prompt 13 phase 7: `describe_frame`, `hit_test`,
`sample`, `code_for_node`/`node_for_code`, `motion_report`,
`layout_report`, `legibility_report` (contrast against what is really
behind each text, sampled from a render without the text — provided by
`motioner_render`; coordinate the interface with C3), `check_design` with
one rule table, `docs/design-rules.md` with a good/bad image per rule, and
defect fixtures (violating + fixed program per rule). Then apply prompt 13
phase 5's requirement: **every preset and component passes
`check_design` on its own** with default arguments on every theme; fix the
library where it does not.

## Phase C3 — Render tools (prompt 13 phase 8)

Exactly as specified in prompt 13 phase 8 (overlays in a separate pass,
filmstrip, contact sheet, zoom, compare with heatmap, view_asset,
render_variants on throwaway copies with a byte-identical test,
render_preview, image cache keyed by program hash/frame/size/overlay).
Also the "render without the text" sampling service used by C2.

## Phase C4 — Media analysis (prompt 13 phase 9)

Exactly as specified in prompt 13 phase 9, with generated media of known
truth (click tracks at 90/120/128 bpm: tempo ±1 bpm, beats ±20 ms; video
with cuts at known frames; images with a known empty half). Small,
written-here algorithms; no new crates without an ADR.

## Phase C5 — Tool runtime and CLI (prompt 13 phase 10)

Exactly as specified in prompt 13 phase 10. The catalogue **extends the
single generator** that prompt 12b created for `motioner tools --schema`
(one list of tool descriptors; commands and code tools from 12b stay, the
D-0043 tools are added), never a second generator. Executors go through the
same `Editor` (write-through, checkpoints, undo, traces from 12b record
tool-driven edits with the reserved agent fields left empty). User tools
return `RequiresHarness` from the CLI. `motioner tool ... --args` and
`--batch` (replayable). Contract tests: every tool has an example call on
`examples/demo.motioner` whose result validates against its schema.
Leave room in the descriptors for prompt 16's rig tools (properties,
poses, clips, inputs) without implementing them.

## Phase C6 — Examples, docs, measurements, report (prompt 13 phase 11)

Exactly as specified in prompt 13 phase 11: three `std` examples (kinetic
title, product teaser, data story) with `.prompt.md` briefs, scene plans
and zero `check_design` errors, one golden frame each, and `motioner corpus
check` passing on them; docs (`docs/std.md` updated, `docs/design-rules.md`,
`docs/tools.md` with how an external agent such as Claude Code uses
`motioner tool`, spec, README); release measurements (prelude cost before
and after C0, `check_design` on `showcase_pro.motion`, each render-tool
latency, media analysis times); tasks closed with `pctl`; CONTEXT updated;
final report **in Spanish** at `out/reports/PHASE-13B-REPORT.md` covering
C0-C6, ADRs, dependencies, numbers, gate results, free space on C: and what
is left (prompt 14 agent harness and prompt 16 rigs come next).
