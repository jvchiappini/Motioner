---
actualizado: 2026-11-01
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0022.

## Working model

- Code is usually written by opencode with DeepSeek V4.1 Flash; Claude is
  architect and auditor (prompts, reviews, checks, small fixes).
  Conversation with the user is in Spanish.
- Prompts live in `.control/prompts/opencode/opencode-prompt-NN-*.md`
  (08 executed 2026-11-01). Work only inside Motioner, FerrousEngine and
  GUIMaker.
- The user wants LONG multi-phase prompts and a visible result (PNG/MP4).

## Current state (branch feat/m1-foundations, uncommitted prompt 08 work)

- FIRST RENDER WORKS: `motioner render|still|info` + `motioner preview`.
- The editor now EDITS (D-0022): every UI gesture becomes an entry in
  `<name>.overrides.motion`; the program only changes through
  `motioner bake <file> [--id ID]...`.
- **Keyframes are per parameter.** Core `Transform` stores
  `AnimatableVec2` (X and Y are independent `Animatable<f64>` channels) for
  position, scale and anchor; rotation/opacity stay scalar. Override/key
  names accept an axis suffix: `pos.x`, `pos.y`, `scale.x`, …;
  `key("pos", …)` keys both axes. A 3D transform will add `z`.
- Crates: core (identities, visible, content setters, OverrideSet),
  dsl (provenance, `.id()`, severities, run_with_overrides, axis keys,
  bake), eval (sample_vec2), ui (session debounced atomic writes + undo,
  per-parameter inspector, gizmos/snapping, timeline rows per axis with
  visibility strips and inherited keys, layers actions, orphans, clipboard),
  app (bake CLI). 215+ tests, gates green, golden byte-identical.
- Manual run: `scripts/ui_drive.ps1` drives the release window with posted
  mouse/keys and captures `out/ui/p08-*.png`. Bake canvas is byte-identical
  before/after (verified by a pixel diff).

## Gotchas

- CI `RUSTFLAGS=-D warnings` will fail on engine path deps; use
  `submodules: true` and rely on `clippy -D warnings` instead.
- `scripts/check_boundaries.py` bans ferrous/wgpu prefixes everywhere:
  exempt motioner_render and add a motioner_dsl rule.
- Disk: 10.6 GB free on C: after this session. CARGO_INCREMENTAL=0, `-j 2`.
- PowerShell `Add-Type` fails while `LIB` lists a missing CUDA path: clear
  `$env:LIB` first (done in the capture/drive scripts).
- PowerShell scripts: function names `LP`/`MV` collide with aliases
  (`Out-Printer`, `Move-Item`); avoid two-letter function names.
- `cargo fmt` cannot split string literals: keep long user-facing strings
  under 100 columns by hand.
- `scripts/ui_shot.ps1` uses the DEBUG binary unless `-Release` is passed.
- winit needs real focus for SendKeys; the driver calls SetForegroundWindow
  and moves the real cursor with SetCursorPos so hover matches posted clicks.
- 8 GB RAM: cargo with default parallelism can fail linking with
  `os error 1455`. Use `-j 2`.

## FerrousEngine state

- Pin `32aea8a` = head of `feat/pure2d-text-and-readback` (pushed, NOT merged).
- Dev clone `C:\Users\jvchi\CARPETAS\FerrousEngine` at `0ef6fe6` has the
  Part A prompt-08 changes **uncommitted**: exact sRGB constructors,
  border/shadow packing, `draw_text_baseline` with kerning, screenshot hook
  (COPY_SRC + `Screenshot`/`on_screenshot`). The reviewer commits and bumps
  the pin.
- Pre-existing engine clippy warnings block the 4B clippy gate (listed in the
  prompt-08 report); do not fix unrelated engine code.

## GUIMaker facts (read via GitHub API, not cloned, 2026-10-31)

- GUIMaker (github.com/jvchiappini/GUIMaker, head 9dcbc28, ~1 MB, Rust)
  is a standalone visual designer BINARY that generates Rust UI code.
- It does not build against the current engine: it imports `ferrous_app`
  (removed in engine main 02e6d48); path deps `../crates/...`.
- User direction: GUIMaker must become an egui-class toolkit AND a
  SceneBuilder-style visual designer. Needs an architecture proposal and ADR
  before code (T-ARCH-0007).

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Editor UI

- `motioner preview` opens the editor window (D-0020/D-0021). Chrome is an
  immediate painter on the engine `GuiBatch`; tokens in `theme.rs` are the
  only colour literals. Layouts are pure and shared by drawing and hit
  testing. See `crates/motioner_ui/README.md` for the shortcuts and the
  per-parameter timeline.
- Editing: inspector rows per parameter (Pos X/Y, Scale X/Y, Rotation,
  Anchor X/Y, Opacity + content), stopwatch/diamond per channel, canvas
  gizmos (move/scale/rotate, snapping), timeline sub-rows per parameter with
  ◀ ◆ ▶, key marquee + retime + ease, layers eye/lock/rename, orphans.
- Engine pitfalls found: `Color::hex` does not linearise; `draw_text` takes
  the em-box top; `GuiBatch::border/shadow` pack fields the shader ignores;
  surface lacked `COPY_SRC`.

## Last relevant session

S-2026-11-01-001 (open, to close after the prompt-08 report review): prompt 08
executed end to end in Motioner + FerrousEngine clone, gates green, manual
run and screenshots done.
