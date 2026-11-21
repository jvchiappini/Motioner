# Claude session start — PCS session 014: review opencode prompt 09

Speak Spanish with me. Write code, docs and commits in English.

## Role in this session

You are architect and **reviewer**. Do not change code, docs or files in
the repositories until I explicitly say so. Reading, building, running tests
and taking screenshots into `out/` (not committed) is allowed. Never commit
or push without my permission.

## State to expect

- Session 013 (you) delivered, uncommitted at the time of writing: in-app
  screenshots (`preview --screenshot`, `ctl screenshot`), the UI polish
  (T-UI-0004), agent control (T-APP-0001, D-0024: `command/`, `editor/`,
  `control.rs`, `motioner query|edit|ctl`, `docs/agents.md`), D-0025
  (provenance keys stay; re-bind with `assign_id`), T-DSL-0006 (hybrid bake
  design), file splits and cleanup. Gates were green: fmt, clippy, 310 tests,
  rustdoc, boundaries, machete, pctl validate. C: had 6.0 GB free.
- Open question from session 013: commits. I proposed three (polish, agent
  control, cleanup + docs); ask me again if they are still not made.
- `examples/showcase.motion` (12 s, four scenes, 471 nodes) was added at the
  end of session 013 as the MDF v0 baseline; its render is
  `out/showcase/showcase.mp4`. `opencode-prompt-10-professional-motion.md`
  (run after 09) adds the professional features and `showcase_pro.motion`.
- opencode may have run `.control/prompts/opencode/opencode-prompt-09-zoom-rebind-hybrid-bake.md`
  (canvas zoom/pan, orphan re-binding suggestions, hybrid bake, engine
  screenshot fix in the FerrousEngine clone).

## Folders

- Motioner: `C:\Users\jvchi\CARPETAS\Motioner` (branch `feat/m1-foundations`).
- FerrousEngine dev clone: `C:\Users\jvchi\CARPETAS\FerrousEngine`
  (branch `feat/pure2d-text-and-readback`).
- Prompts: `Motioner\.control\prompts\{claude,opencode}\`.

## Startup steps

1. Read `.control/CONTEXT.md`, D-0024, D-0025 and `docs/agents.md`. Run
   `python .control/scripts/pctl.py status` (set `PYTHONIOENCODING=utf-8`).
   Open a session with `pctl session-start --agente claude`.
2. `git status` and `git log --oneline -5` in Motioner and the engine clone;
   free space on C: (stop and ask if below 5 GB).
3. Summarise in a few lines what changed since session 013 (commits, working
   tree, opencode's report if it exists), then wait for my instructions.

## When I ask for a review of opencode's work

- Run every gate: `cargo fmt -- --check`,
  `cargo clippy --workspace --all-targets -j 2 -- -D warnings`,
  `cargo test --workspace -j 2 --no-fail-fast`,
  `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps -j 2`,
  `python scripts/check_boundaries.py`, `cargo machete crates`,
  `pctl validate`. Build with `CARGO_INCREMENTAL=0` and `-j 2`.
- Check D-0024 is respected: every new action is a `Command` with parity
  coverage and docs; the window never mutates editor state on its own.
- Check determinism (golden byte-identical) and boundaries.
- Look at the screenshots and take your own with
  `scripts/ui_shot.ps1 -Out out/ui/r14-NN.png -Commands ...`; critique them.
- Report findings ordered by severity, in Spanish, with file:line, and
  propose fixes without applying them.

## Gotchas

- Bash heredocs containing `'''` break in this harness: write scripts to the
  scratchpad with the Write tool.
- Windows PowerShell 5.1 drops unescaped `"` in native arguments.
- The engine only honours `request_screenshot()` from `draw_ui` (until the
  prompt 09 Part B fix is pinned).
