---
actualizado: 2026-11-01
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0028.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor (prompts, reviews, checks, small fixes). Session 015
  (opencode) ran prompt 10, phase 1 only. Conversation with the user is in
  Spanish.
- Prompts live in `.control/prompts/{opencode,claude}/`. Work only inside
  Motioner, FerrousEngine and GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).
- Prompt 10 is a 12-phase job. Only the ADR and Phase 1 are done; every later
  phase has a backlog task (T-RENDER-0007..0013, T-CORE-0002, T-UI-0006/0007,
  T-APP-0003, T-PERF-0002) with its design in the body.

## Current state (branch feat/m1-foundations, session 015 uncommitted)

- `motioner render|still|info|bake|preview|query|edit|ctl`.
- The editor EDITS through overrides (D-0022); the program only changes with
  `bake`.
- **Agent control (D-0024)**: one `Command` enum, MDF call syntax,
  `motioner_ui::command`, executed only by the window-free `Editor`.
  `motioner query` (JSON scene), `motioner edit` (all-or-nothing,
  `--dry-run`), `preview --control` + `motioner ctl`. Docs: `docs/agents.md`.
- **Canvas view (D-0026)**: `ViewState.zoom/pan/fit_zoom`;
  `viewport::Viewport` is the one window ↔ composition mapping.
- **Orphans (D-0025)**: `Editor::orphan_suggestions()` ranks live nodes.
- **Hybrid bake (D-0027)**: value origins + in-place literal edits + re-key +
  verify.
- **Professional motion model (D-0028, session 015)**: content fields are
  animatable channels. `NodeContent` stores `Animatable<f64>` /
  `Animatable<Color>` / `Animatable<Option<Color>>` (`PaintSpec` for
  fill/stroke/stroke_width). `NodeContent::channel(field)` is the one
  field-to-channel table. `Document::add_content_keyframe` /
  `remove_content_keyframe` are the validated writes.
  - Colour interpolation is **linear-light sRGB** (`Color::lerp`; the sRGB
    transfer function moved to `motioner_core::Color` and the renderer
    delegates to it). A keyed red→green midpoint is `#BCBC00`, not `#808000`.
    An optional colour interpolates when both endpoints exist and holds when
    one is `none`.
  - `motioner_eval` samples channels into the plan (`resolved_paint`); a
    stroke is drawn only when its colour is present and width > 0.
  - MDF `key("fill"|"width"|"size"|...)`, overrides `override_key`, bake (in
    place and generated block) and `query` all work on content channels.
  - Editor: `editor::channels::Channel` = `Transform(property, axis)` |
    `Content(field)`. Content timeline rows have stopwatches, diamonds and
    eased lines; the inspector keys them at the playhead; `add_keys` keys all
    animated channels; content channels appear in `query.channels`.
  - `examples/animatable.motion` is the feature demo. Golden
    `crates/motioner_testkit/golden/animatable_f45.png` (SHA `de69d526…`,
    tolerance 2/0.1 %). `out/color_space_compare.png` justifies linear light.
- Keyframes per axis; animated vectors are always keyed per axis (`pos.x`).
- In-app screenshots: `preview --screenshot <png>` and `ctl screenshot(...)`;
  `scripts/ui_shot.ps1 [-Commands ...]`.

## Gotchas

- CI `RUSTFLAGS=-D warnings` will fail on engine path deps; use
  `submodules: true` and rely on `clippy -D warnings` instead.
- `scripts/check_boundaries.py`: `motioner_ui` may not depend on
  `motioner_io`, so the app passes a `PngWriter` fn to the UI.
- Disk: C: had ~9.0 GB free at session 015 end. CARGO_INCREMENTAL=0, `-j 2`.
- **Engine screenshot hook**: the runner carries an `update`-time
  `request_screenshot` into the capture decision; Motioner still requests in
  `draw_ui`.
- Windows PowerShell 5.1 drops unescaped `"` in arguments to native programs:
  escape them (`'select(\"@288\")'`); a wrapper script with literal strings
  plus that escape is the reliable way to drive `ui_shot.ps1`/`motioner edit`.
- `core.autocrlf=true`: working copies are CRLF, commits are LF; normal.
  Mixing `Set-Content` with existing LF files makes `cargo fmt --check` fail
  with "Incorrect newline style"; run `cargo fmt` after.
- Bash heredocs containing `'''` break in this harness: write scripts to the
  scratchpad with the Write tool.
- PowerShell `Add-Type` fails while `LIB` lists a missing CUDA path.
- `cargo fmt` cannot split string literals: keep long strings < 100 columns.
- 8 GB RAM: cargo with default parallelism can fail linking (`os error 1455`).
- Clippy enables `option_option`: use a named enum instead of
  `Option<Option<T>>` (see `editor::channels::SampledColour`).

## FerrousEngine state

- Pin `82f7210` on `feat/pure2d-text-and-readback` (pushed, not merged):
  update-time screenshot request honoured (D-0023).
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
- Orphans rows: check icon = Rebind, error icon = delete.
- Content rows (`fill`, `width`, `size`, ...) now draw the same controls as
  transform rows; `text`/`align` rows stay static.

## Last relevant session

S-2026-11-01-005 (opencode, session 015): prompt 10, ADR D-0028 and Phase 1
(animatable content properties) complete. Gates green. Next: Phase 2
(T-RENDER-0007, vector paths) unless the engine pin constrains the render
hook.
