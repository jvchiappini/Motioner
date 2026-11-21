# opencode prompt 09 — Motioner: canvas zoom/pan, orphan re-binding, hybrid bake, engine screenshot fix

You are the orchestrator of a long, multi-phase job in two repositories:

- **Motioner** at `C:\Users\jvchi\CARPETAS\Motioner` (branch
  `feat/m1-foundations`). Session 013 (Claude) left a large change set in the
  working tree (it may or may not be committed when you start — do not care,
  build on what is there): a typed command layer (`motioner_ui::command`), a
  window-free `Editor` that is the only executor (`motioner_ui::editor`),
  `motioner query|edit|ctl`, `preview --control`, in-app screenshots and a
  UI polish pass. Read D-0024, D-0025, `docs/agents.md` and
  `crates/motioner_ui/README.md` before writing code.
- **FerrousEngine dev clone** at `C:\Users\jvchi\CARPETAS\FerrousEngine`
  (branch `feat/pure2d-text-and-readback`, head `c4ae1f2`): one small fix
  (Part B). Motioner uses the pinned submodule `Motioner\engine\ferrous`,
  never the clone; the reviewer bumps the pin later.

Quality bar: top-tier studio. Zero duplication, rustdoc on public items,
typed errors, no panics on runtime paths, regression tests that fail without
the change, no warnings. Work through every phase in order and run the phase
gate before moving on. Do not stop early unless a stop condition applies.

---

## 0. Ground rules (every phase, every subagent)

- Work only inside the two folders above. Never edit
  `Motioner\engine\ferrous` (pinned submodule, D-0018).
- Do not commit, stage, stash, reset, checkout, rebase or push in either
  repository. The reviewer commits.
- Build with `CARGO_INCREMENTAL=0` and `-j 2` (8 GB RAM; `os error 1455` is
  memory: rerun with `-j 1`). No release builds.
- Free space on C: is about **6 GB**. If it drops below **5 GB**, stop and
  report. You may clean caches outside the repositories (Temp, npm cache,
  browser caches).
- Motioner: `cargo fmt` (never `--all`), `cargo machete crates` (never at the
  root). Clippy pedantic with `-D warnings`; every `#[allow]` has a reason in
  a comment.
- No new dependencies. No `thiserror` in libraries, `anyhow` only in
  `motioner_app`, no new `unsafe`.
- **The command layer is law (D-0024).** Every new user action is a new
  `Command` variant with a `CommandKind`, a name, a signature, an example, a
  canonical text and an `Editor::execute` path. The window maps the gesture
  to that command; it never mutates editor state on its own. The parity
  tests (`crates/motioner_ui/src/command/tests.rs`,
  `crates/motioner_app/tests/agent_parity.rs`) must keep covering every
  variant. Update `docs/agents.md` for every new command.
- **Determinism.** Pixels depend only on the document and the integer frame.
  The golden `crates/motioner_testkit/golden/hello_f120.png` (SHA in
  `tests/golden_hello.rs` and `crates/motioner_ui/tests/live_painter.rs`)
  stays byte-identical. Zoom and pan are view state: they never change a
  rendered frame.
- **Boundaries** (`scripts/check_boundaries.py`): engine types only in
  `motioner_render` and `motioner_ui`; `motioner_core`, `motioner_dsl`,
  `motioner_eval` stay engine-free and file-system-free; `motioner_ui` must
  not depend on `motioner_io`.
- Shell gotchas: Bash heredocs containing `'''` or mixed quotes break in this
  environment — write scripts and files with the editor tool. Windows
  PowerShell 5.1 drops unescaped `"` in arguments to native programs
  (escape as `\"`). `core.autocrlf=true`: CRLF in the working copy is normal.
- Screenshots: `scripts/ui_shot.ps1 -Out out/ui/p10-NN-<what>.png
  [-Commands 'select(\"@288\")', ...]` (in-app capture, no focus). Look at
  every screenshot you take and fix the worst visible problem first.

Phase gate (Motioner): `cargo fmt -- --check`,
`cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
`cargo test --workspace -j 2 --no-fail-fast`,
`RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
`python scripts/check_boundaries.py`, `cargo machete crates`,
`python .control/scripts/pctl.py validate`.

Stop conditions: C: below 5 GB; a gate you cannot make green after two
honest attempts; a design question that contradicts an accepted ADR. Report
what you did and why you stopped.

---

## Phase 1 — Canvas zoom and pan (view state, commands, gestures)

Today the composition is always fitted to the canvas; there is no zoom, so
D-0024 lists "no zoom command" as a limit.

1. Add canvas view state to `editor::ViewState`: `zoom: Option<f64>`
   (`None` = fit) and `pan: (f64, f64)` in composition pixels. Keep it pure
   and testable; `app/canvas.rs::comp_rect` must derive the composition
   rectangle from it (fit when `None`).
2. Commands (with names, signatures, examples, docs):
   - `zoom(factor)` — absolute zoom (1 = 100 %), clamped to 0.05..=32.
   - `zoom_fit()` — back to fit, pan reset.
   - `zoom_by(steps, x=None, y=None)` — the wheel: multiply by 1.25^steps,
     keeping the composition point under the pointer fixed when `x`/`y`
     (composition pixels) are given.
   - `pan(dx, dy)` — move the view in composition pixels.
3. Gestures: Ctrl+wheel over the canvas → `zoom_by` around the pointer;
   middle-button drag or Space+drag → `pan`; `F` (or the top-bar zoom pill on
   click) → `zoom_fit`; `1` → `zoom(1)`. Space must still toggle playback
   when no drag happens (decide and document the rule, e.g. Space+drag only
   while the pointer is over the canvas and the button moves).
4. Everything that maps window ↔ composition (gizmos, picks, snapping
   threshold, selection outlines, safe areas, the zoom pill text) must use
   the same mapping. Add tests for the mapping at fit, 100 %, 400 % and with
   pan, and for "point under the pointer stays fixed".
5. `state()` reports `zoom` (`null` = fit) and `pan`.
6. Screenshots: fit, 100 %, 400 % on the title with a selection and the
   gizmo, at 1600x900 and 1280x720 @ 125 %.

Gate.

## Phase 2 — Orphan re-binding suggestions (D-0025 "next step")

1. In `motioner_ui::editor`, compute **suggestions** for each orphan: the
   live nodes not targeted by any other override of the same property whose
   kind accepts the orphan's property, ranked by (same creation order among
   nodes of that kind, nearest provenance offset, same parent). Pure,
   deterministic, unit-tested with a program that is edited above a node so
   its key shifts.
2. `query` adds `"suggestions": ["@id", ...]` to each orphan (best first).
3. The orphans list in the layers panel gets a "Rebind" action next to the
   delete action when there is a suggestion; it runs
   `assign_id(orphan_id, best_suggestion)` (no new command needed if the
   existing one covers it; if you add `rebind_orphans()` for "accept every
   best suggestion", it is a full command with parity coverage).
4. Integration test through `motioner edit`: copy `hello.motion`, add
   overrides, insert a comment line at the top of the program (keys shift),
   check the orphans, re-bind them, and check the document equals the one
   before the program edit.
5. Update D-0025 (append a dated "Addendum" section; do not rewrite the
   decision) and `docs/agents.md`.

Gate.

## Phase 3 — Hybrid bake (T-DSL-0006)

Read `.control/tasks/dsl/T-DSL-0006.md`: it contains the design. Implement
it in `motioner_dsl` (value origins recorded by the interpreter, private
literal detection, in-place edits, fallback to today's behaviour) and wire
it into `editor/bake.rs` so `bake`, `motioner bake` and `ctl bake()` share
it.

- Bake must stay exact: `run(baked program) + remaining overrides ==
  run(program) + all overrides` (extend `agent_parity.rs` and the DSL bake
  tests). Baking twice changes nothing.
- After an in-place edit, provenance keys of later nodes shift: re-key the
  remaining overrides in the same atomic write (use the Phase 2 matching or
  the interpreter's knowledge of the edit offsets; explain which in the ADR).
- `hello.motion`'s circles (created in a `for` loop from shared literals)
  must still be refused for shared values; a literal used once (for example
  the title's `size=160`) must be edited in place.
- Write an ADR for the value-origin model and the private-literal rule
  (`pctl decision-new`), update spec §9.3, close T-DSL-0006 with
  `pctl task-move`.

Gate.

## Phase 4 — Engine (Part B): screenshot requested in `update`

In the FerrousEngine clone, `AppContext::request_screenshot()` is honoured
only when called from `draw_ui`: the flag set during `update` is dropped
because `runner/frame.rs` builds a fresh context for drawing. Its rustdoc
says both work. Carry the update-time request into the draw context (or the
capture decision), add a regression test if the runner allows one (a pure
helper is fine), keep the 25 CRLF files' endings, run the engine's own
tests and `cargo clippy` on the crates you touched, and **do not** change
Motioner's code for it (Motioner keeps requesting in `draw_ui`). Report the
new head; the reviewer pushes and bumps the pin.

## Phase 5 — Docs, manual run, report

1. `docs/agents.md`, root `README.md`, `crates/motioner_ui/README.md`,
   `spec/mdf-v0.md`, `.control/CONTEXT.md` describe the final state.
2. Manual run only through commands: `scripts/ui_shot.ps1 -Commands ...`
   covering zoom/pan, an orphan re-bind and a hybrid bake; screenshots in
   `out/ui/p10-*`.
3. Tasks: create one task per phase with `pctl task-new` (prefix UI, APP,
   DSL, ENGINE as fits), log events with `pctl session-log` in the active
   session, close what is done.
4. Final report (in Spanish): what changed per phase, new commands with one
   example each, ADRs, screenshot paths, gate results, free space on C:,
   engine clone head, and what is left.
