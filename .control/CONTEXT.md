---
actualizado: 2026-11-01
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0027.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor (prompts, reviews, checks, small fixes). Session 013
  was done by Claude. Session 014 (opencode) ran prompt 09. Conversation
  with the user is in Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).

## Current state (branch feat/m1-foundations, sessions 013 + 014 uncommitted)

- `motioner render|still|info|bake|preview|query|edit|ctl`.
- The editor EDITS through overrides (D-0022); the program only changes with
  `bake`.
- **Agent control (D-0024)**: one `Command` enum (49 kinds, MDF call syntax,
  `motioner_ui::command`) executed only by the window-free `Editor`
  (`motioner_ui::editor`). `motioner query` (JSON scene), `motioner edit`
  (all-or-nothing, `--dry-run`), `preview --control` + `motioner ctl`.
  Docs: `docs/agents.md`.
- **Canvas view (D-0026)**: `ViewState.zoom` (`None` = fit), `pan`,
  `fit_zoom`; `viewport::Viewport` is the one window ↔ composition mapping.
  Commands `zoom(factor)`, `zoom_fit()`, `zoom_by(steps, x, y)`,
  `pan(dx, dy)`; gestures Ctrl+wheel, middle/Space+drag, F, zoom pill, `1`.
  Space toggles playback on release only when it did not drag.
- **Orphans (D-0025 + addendum)**: `Editor::orphan_suggestions()` ranks live
  nodes (property acceptance, inferred program shift, loop iteration,
  offset, parent); `query` adds `suggestions`; the layers panel has a Rebind
  action that runs `assign_id`.
- **Hybrid bake (D-0027, T-DSL-0006)**: the interpreter records `ValueOrigin`
  (span + literal shape + key/ease); bake edits private literals in place
  (count == 1), appends for the rest, re-keys remaining overrides by the
  exact edit offsets, and verifies by re-running; on mismatch it falls back.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...]`.

## Gotchas

- CI `RUSTFLAGS=-D warnings` will fail on engine path deps; use
  `submodules: true` and rely on `clippy -D warnings` instead.
- `scripts/check_boundaries.py`: `motioner_ui` may not depend on
  `motioner_io`, so the app passes a `PngWriter` fn to the UI.
- Disk: C: had ~9.5 GB free at session 014 end. CARGO_INCREMENTAL=0, `-j 2`.
- **Engine screenshot hook**: the runner now carries an `update`-time
  `request_screenshot` into the capture decision (session 014, uncommitted in
  the engine clone); Motioner still requests in `draw_ui`.
- Windows PowerShell 5.1 drops unescaped `"` in arguments to native programs:
  escape them (`'select(\"@288\")'`); a wrapper script with literal strings
  plus that escape is the reliable way to drive `ui_shot.ps1`/`motioner edit`.
- `core.autocrlf=true`: working copies are CRLF, commits are LF; normal.
- Bash heredocs containing `'''` break in this harness: write scripts to the
  scratchpad with the Write tool.
- PowerShell `Add-Type` fails while `LIB` lists a missing CUDA path.
- `cargo fmt` cannot split string literals: keep long strings < 100 columns.
- 8 GB RAM: cargo with default parallelism can fail linking (`os error 1455`).
- At 400 % zoom the title sits near the canvas centre only with an explicit
  `pan`; visible composition content is correct, but the selection box can
  extend outside the canvas.

## FerrousEngine state

- Pin `c4ae1f2` = head of `feat/pure2d-text-and-readback` (pushed, NOT
  merged), D-0023. Session 014 added an uncommitted fix on top of it: an
  update-time screenshot request is honoured (helper + 3 tests). The reviewer
  pushes and bumps the Motioner pin.
- Golden `hello_f120.png` SHA `c2173bad...` (still byte-identical).
- Pre-existing engine warnings when the engine is linted on its own.

## GUIMaker facts (read via GitHub API, not cloned, 2026-10-31)

- GUIMaker is a standalone visual designer BINARY that generates Rust UI
  code; it does not build against the current engine (`ferrous_app` removed).
- User direction: egui-class toolkit AND SceneBuilder-style designer; needs
  an architecture proposal and ADR before code (T-ARCH-0007).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Editor UI

- Chrome is an immediate painter on the engine `GuiBatch`; tokens in
  `theme.rs` are the only colour literals; layouts are pure and shared by
  drawing and hit testing. See `crates/motioner_ui/README.md`.
- One key-marker language (`widgets::key_marker`): filled gold = key, dark
  core = shared on a layer row, accent = selected, hollow = inherited/none.
- Visibility strips: solid where drawn, outline where not. Eased segments
  are solid lines following the ease. Inspector ease grid with curves.
- Orphans rows: check icon = Rebind to the suggested node, error icon =
  delete.

## Last relevant session

S-2026-11-01-003 (opencode, session 014): prompt 09 — canvas zoom/pan
(T-UI-0005, D-0026), orphan suggestions (T-APP-0002), hybrid bake
(T-DSL-0006, D-0027), engine screenshot fix (T-RENDER-0006). Gates green.
