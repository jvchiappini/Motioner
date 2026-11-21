# Claude session start — PCS session 013: polish the editor and full agent control

Speak Spanish with me. Write code, docs and commits in English.

## Goal

Two things, both done by you (no opencode):

1. **Polish** what prompt 08 delivered (editing, overrides, per-property
   timeline) until it is clean: no visual defects, no dead code, no
   duplication, docs up to date.
2. **Agent parity**: an LLM must be able to do *absolutely everything* a
   person can do with the app — open, inspect, select, scrub, play, edit
   any property, add/move/delete keys, change easing, hide/lock/rename,
   undo/redo, delete orphans, bake, take screenshots — without mouse
   coordinates, SendKeys or window focus. Motioner is "AI-first"
   (`.control/PROJECT.md`); today the only way to drive the editor is
   `scripts/ui_drive.ps1`, which posts clicks at pixel coordinates. That is
   not acceptable as the agent interface.

The changes are mostly short; prefer many small, verified steps.

## State at the start

- Motioner `feat/m1-foundations`, pushed: `e93d5f1` (prompt 08: overrides,
  inspector, gizmos, per-property timeline, undo, bake) and `fde5841`
  (engine pin bump to `c4ae1f2`, D-0023, golden regenerated: 14 AA pixels
  of the title, max diff 4).
- FerrousEngine clone `C:\Users\jvchi\CARPETAS\FerrousEngine`, branch
  `feat/pure2d-text-and-readback` at `c4ae1f2`, pushed. It has the in-app
  screenshot hook: `AppContext::request_screenshot()` and
  `FerrousApp::on_screenshot(&mut self, Screenshot { width, height, rgba })`.
  Motioner does **not** use it yet.
- Prompt 08 report is in the session log of `S-2026-11-01-001` (opened by
  opencode) and summarised in `.control/CONTEXT.md`. Tests: ~290, all green.

## Folders (work only here)

- Motioner: `C:\Users\jvchi\CARPETAS\Motioner`.
- FerrousEngine dev clone: `C:\Users\jvchi\CARPETAS\FerrousEngine`.
- Prompts: `Motioner\.control\prompts\{claude,opencode}\`.

## Rules

- Never commit or push without my permission.
- Never edit `Motioner\engine\ferrous` (pinned submodule, D-0018). Engine
  changes go to the clone on a branch, then a pin bump with an ADR
  (procedure in `engine/README.md`). Prefer solving things in Motioner.
- Build with `CARGO_INCREMENTAL=0` and `-j 2` (8 GB RAM, `os error 1455`).
  **C: had 6.7 GB free.** If it drops below 5 GB, stop and ask me. You may
  clean caches outside the three repos (Temp, npm cache, browser caches,
  shader caches). Avoid release builds unless measuring fps.
- `cargo fmt` (never `--all`), `cargo machete crates` (not at the root).
- Engine types only in `motioner_render` and `motioner_ui` (D-0020);
  `motioner_core`, `motioner_dsl`, `motioner_eval` stay engine-free.
- Rendered frames stay deterministic; the golden `hello_f120.png` stays
  byte-identical (the D-0023 version).
- No new dependencies without an ADR. D-0005 forbids JSON/RON as the
  *document* format; a JSON *output* for tools is a separate decision that
  needs its own ADR if you choose it.
- FerrousEngine keeps 25 CRLF files in the index; check
  `git ls-files --eol` before any engine commit.

## Startup steps

1. Read `.control/CONTEXT.md` and `D-0022`, `D-0023`. Run
   `python .control/scripts/pctl.py status`. Close session
   `S-2026-11-01-001` and open a new one with pctl. `T-UI-0002` is still
   `in_progress`: close it if prompt 08 finished its scope.
2. `git status` in Motioner and the engine clone; free space on C:.
3. Summarise the state in a few lines, then start Phase 1.

## Phase 1 — in-app screenshots (the iteration tool)

Replace `PrintWindow` (`scripts/ui_shot.ps1`, `capture_window.ps1`) with the
engine hook:

- `motioner preview <file> --frame N --paused --window-size WxH
  [--ui-scale F] [--select ROW] --screenshot <png> --exit-after-frames K`
  requests a screenshot after the last UI draw, writes it with
  `motioner_io::write_png` and exits with a clear error if the surface does
  not support `COPY_SRC`.
- Check that the PNG matches what is on screen (colours, sRGB, BGRA order,
  DPI scale) by comparing once against a `PrintWindow` capture.
- Update `scripts/ui_shot.ps1` to use it (keep `PrintWindow` only as a
  documented fallback, or delete it if unused).

Every polish iteration below: capture at 1600x900, look at it with Read,
critique in two lines, fix the worst problem first. Screenshots go to
`out/ui/p09-NN-<what>.png` (not committed).

## Phase 2 — polish (visible defects found in the prompt 08 screenshots)

Look at `out/ui/p08-01-selection-gizmo.png`, `p08-07-property-timeline.png`
and `p08-08b-collapsed-rect1.png` first.

1. **Visibility strips are not visible.** Circle 2 should read as absent on
   0–0.3 s and Group 1 on 0–1.0 s; the bars look uniform. Make invisible
   ranges clearly different (empty/hatched, or no bar) and visible ranges a
   solid bar. The pure function already exists in `timeline_model.rs`;
   this is drawing and theme (`theme.rs` `track_bar*`).
2. **Eased segments are almost invisible** (faint dotted line). Draw them
   as a clear line between keys whose shape follows the easing, readable
   on the expanded rows.
3. **Selected layer's sub-rows are flooded with selection blue.** Use the
   selection colour only for the layer row; sub-rows get a subtle,
   darker treatment. Stopwatches must look clearly *on* (animated) or
   *off*; today they all look on.
4. **Summary diamonds mix hollow and filled shapes without meaning.**
   Define one visual language (e.g. filled = key on this layer, hollow =
   inherited from parent, outlined accent = selected, a small mark when
   several channels share the frame) and use it everywhere: collapsed
   rows, sub-rows, inspector diamonds. Document it in the UI README.
5. **Timeline height.** Only ~6 property rows fit at 1600x900. Make the
   splitter between canvas and timeline draggable (if it is not), give a
   sensible default, and make the timeline scroll to the selected key/row.
6. **Inspector.** The key diamond overlaps the "Opacity" label; the panel
   needs scrolling and the "Ease" control at the bottom is clipped and is
   a value that cycles on click. Fix the label/diamond layout, add proper
   vertical scrolling to the inspector, and make Ease a real control
   (segmented or dropdown list of the existing `Ease` names, with a tiny
   curve preview if cheap).
7. Anything else you see while iterating: alignment to the 4 px grid,
   clipped text, contrast ≥ 4.5:1 for body text, 1280x720 and 125 % scale.

## Phase 3 — agent control (parity with a person)

Design it, write an ADR, then implement it.

**Principle.** There is one typed command layer. Mouse, keyboard and agent
commands all go through it; the agent interface is not a second
implementation. Today that layer is split between `input::Action`
(transport, selection, undo…) and `edit::EditCommand` (overrides). Unify or
wrap them in one `Command` enum with a stable text name per variant.
Anything a person can trigger (every button, shortcut, drag result,
inspector field, layer action, orphan delete, rename, expand/collapse,
selection, scrub, zoom, safe-area toggle) must map to a `Command`.

**Two surfaces:**

1. **Headless CLI** (no window, works in CI and for any LLM with a shell):
   - `motioner query <file> [--frame N] [--node ID]` prints the scene:
     nodes with identity key, kind, name, parent, visible/locked, every
     property's value at frame N, keys per channel (time, value, ease),
     overrides, orphans and diagnostics. Machine-readable and stable
     (sorted, deterministic); choose the format in the ADR (JSON or an
     MDF-like text — justify it against D-0005).
   - `motioner edit <file> <command>...` applies commands to the overrides
     file through the same `edit::apply` code the editor uses, then
     prints what changed. Commands use the same syntax as the overrides
     file where possible (`set("title", "pos.x", 900)`,
     `key("title", "opacity", 1.5, 0.5, ease="quad_out")`,
     `unkey(...)`, `move_key(...)`, `visible(...)`, `lock(...)`,
     `rename(...)`, `delete_orphan(...)`). Reuse the DSL parser; no new
     grammar if you can avoid it. `--dry-run` prints the resulting
     overrides text without writing.
   - `still`, `render`, `bake`, `info` already exist; make sure their
     output and errors are equally machine-friendly.
2. **Live control of a running editor** (so an agent can see and drive the
   same window a person uses):
   - `motioner preview <file> --control` opens a local control endpoint
     (127.0.0.1, ephemeral port, random token, both written to a small
     file next to the program or in a known temp path; never listen on
     other interfaces). Poll it from the UI loop without blocking a frame.
   - `motioner ctl <command>` connects to it and runs any `Command`, plus
     queries (`state`: frame, playing, selection, expanded rows, selected
     keys, undo depth, dirty flag; `query` as above but live) and
     `screenshot <png>` (the Phase 1 hook). Each call returns a result or a
     typed error.
   - Undo/redo, selection and every edit done through `ctl` are identical
     to the same action done by hand (same undo entries, same overrides
     text).

**Parity must be enforced, not promised:**
- A test that iterates every `Command` variant and checks it has a text
  name, round-trips through the parser, and is reachable from the CLI and
  `ctl` (exhaustive `match`, so adding a variant without a mapping fails
  to compile or fails the test).
- An integration test that performs the prompt 08 manual run (move the
  title, change its colour, add keys, retime, change ease, hide a circle,
  lock a spinner, undo/redo, delete an orphan, bake) **only through
  `motioner edit`/the command layer**, and compares the resulting
  overrides text and document with the same edits done by `edit::apply`.
- A live test (skips without a GPU, like `tests/live_painter.rs`) that
  starts `preview --control`, sends a few `ctl` commands, takes a
  screenshot and checks state.

**Docs for agents:** a section in the root `README.md` and a short
`docs/agents.md` (or `crates/motioner_app/README.md`) with every command,
its syntax, examples and the output format — written so an LLM can use the
app from that page alone. Replace `scripts/ui_drive.ps1` with a script that
uses `ctl` (or delete it if the integration test covers it).

## Phase 4 — clean up

"Super limpio" means:
- Review the whole prompt 08 diff (`git show e93d5f1 --stat`, then the
  files): duplicated drawing or edit code (inspector vs gizmo vs timeline
  writing the same commands), dead code, `#[allow]`s without a reason,
  untyped errors, `unwrap`/`expect` in the UI loop, allocations per frame
  in hot paths, missing rustdoc, files over ~800 lines that should be
  split.
- `crates/motioner_ui/README.md`, root `README.md`, `spec/mdf-v0.md`,
  `CONTEXT.md` reflect the final state; remove stale notes.
- Open points from prompt 08 to decide (write the decision down):
  - Provenance keys depend on byte offsets, so editing lines above a node
    orphans its overrides. Consider a key less sensitive to unrelated
    edits (e.g. line-relative or call-path based) or at least a tool that
    re-binds orphans (`motioner rebind`), and say which you chose.
  - `bake` refuses nodes created in loops or functions. We discussed a
    hybrid for later: edit the program directly when the value comes from
    a literal that only affects that node, overrides otherwise. Do not
    build it now; record it as a task in the backlog with the design.

## Phase 5 — tests, gates, report

- Gates: `cargo fmt -- --check`,
  `cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
  `cargo test --workspace -j 2`,
  `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps`,
  `python scripts/check_boundaries.py`, `cargo machete crates`,
  `pctl validate`.
- Tasks: create `T-UI-0004` (polish) and `T-APP-0001` (agent control) with
  `pctl task-new`, log events with `pctl session-log`, close what is done.
- Ask me before each commit; propose separate commits (polish, agent
  control, cleanup).

## Final report (in Spanish)

- Before/after screenshot paths for each polish item.
- The agent command list and one end-to-end example an LLM would run.
- Decisions taken (ADRs), what was deleted or simplified.
- Gate results, free space on C:, and what is left.
