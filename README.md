# Motioner

A code-driven motion-graphics editor and NLE with an After Effects-style
timeline, a node graph, audio mixing and an AI agent, built on the
custom Rust **FerrousEngine** renderer. Motioner turns prompts and
Python-like code into professional 2D/3D videos, with a deterministic
pipeline shared by preview and export.

## Status

**Milestone 1: an editor.** The MDF program compiles and renders to MP4 or
PNG, and motioner preview opens an editor window that edits the composition
through **overrides** stored in <name>.overrides.motion (D-0022): inspector
fields, canvas gizmos, timeline keys, visibility/lock, undo/redo. The program
itself changes only through motioner bake. FerrousEngine is vendored as a
pinned submodule; GUIMaker is not.

## What it will do

- Author compositions with a timeline, viewport, inspector and node
  graph.
- Edit video and audio on an NLE timeline, with a stereo mixer and
  effects.
- Describe compositions as Python-like executable code (MDF, the
  Motioner Document Format) with loops, functions, conditionals and
  LaTeX math.
- Let an AI agent create and edit compositions through typed tools,
  render previews, critique them and iterate until the result is good.
- Mix 2D motion graphics and 3D scenes, including skeletal animation and
  GPU particles, in one ordered layer stack.
- Preview at 1080p 60 fps and export frame-accurate MP4 and PNG with
  ffmpeg.

## Architecture at a glance

- The structured **document is the source of truth**; code is a
  lossless projection of it.
- Preview and export call the same **deterministic evaluator**; time is
  integer frames plus a rational fps.
- The evaluator compiles a frame into a flat **render plan**; the
  renderer consumes it through a render graph on FerrousEngine.
- Every mutation is a **typed command** with validation, an inverse and
  a human-readable summary.
- The editor chrome uses engine-native **Ferrous UI plus GUIMaker**, not
  egui.
- Projects are folder-based, use a native text format and store assets
  by content hash.
- 1.0 targets Windows and 1080p; 4K, HDR, color grading, rigid-body
  physics and cloth/fluids are post-1.0.

Full details: [system blueprint](.control/docs/reference/REF-0001.md) and
the [MDF v0 specification](spec/mdf-v0.md).

## Repository layout

```
crates/
  motioner_core/      document model, IDs, commands, undo
  motioner_eval/      time, curves, flattening into render plans
  motioner_render/    FerrousEngine adapter, compositor, effects
  motioner_io/        MDF project format, assets, ffmpeg export
  motioner_app/       application binary and wiring
  motioner_testkit/   fixtures, golden frames, benchmarks
  motioner_ui/        editor window: layers, canvas, inspector, timeline, hot reload
  motioner_dsl/       grammar, parser, printer, sandbox (Phase 5)
  motioner_ai/        providers, tools, context builder (Phase 5)
.control/             project control system (source of truth for state)
.opencode/            agent configuration
engine/               FerrousEngine and GUIMaker, pinned later
scripts/              repository checks
spec/                 language and format specifications
```

## Usage

```
motioner render  examples/hello.motion --out out/hello.mp4
motioner still   examples/hello.motion --frame 120 --out out/hello.png
motioner info    examples/hello.motion
motioner bake    examples/hello.motion
motioner preview examples/hello.motion
```

`preview` opens the editor window: layers on the left, the composition
letterboxed on a canvas with selection gizmos, an editable properties inspector
on the right, and a timeline with transport, ruler, visibility strips and
expandable rows with one lane per parameter (Position X/Y, Scale X/Y, Anchor
X/Y, Rotation, Opacity, plus content fields). X and Y keyframes are independent
and are stored as `pos.x`/`pos.y` overrides. Selection is shared by the canvas,
the lists and the inspector; saving the `.motion` or the overrides file reloads
it, and MDF errors appear as a banner, a red file dot and a status-bar
diagnostic.

Edits do not touch the program: they are written 300 ms after the last change to
`<name>.overrides.motion` (`override`, `override_key`, `override_unkey`,
`override_visible` and `override_locked` statements, D-0022). `motioner bake
<file> [--id ID]...` folds the overrides of nodes bound to a top-level variable
into one generated block at the end of the program, removes them from the
overrides file, and refuses, with a message, nodes created inside loops or
unbound functions.

Shortcuts: Space play/pause, Left/Right step (Shift: 10), Up/Down select layer,
Home/End, L loop, G safe areas, R reload, K add keys, Delete delete keys, U
show only animated properties, Ctrl+Z undo, Ctrl+Shift+Z / Ctrl+Y redo, Esc
quit. Options: `--frame N`, `--paused`, `--exit-after-frames N` (smoke tests and
CI), `--window-size WxH`, `--ui-scale F` and `--select ROW` (screenshots, see
`crates/motioner_ui/README.md`).

## Project control

Motioner is controlled with the Project Control System in `.control/`.
Useful entry points:

- Roadmap: `.control/ROADMAP.md`
- Blueprint: `.control/docs/reference/REF-0001.md`
- Engineering conventions: `.control/docs/reference/REF-0002.md`
- Decisions: `.control/decisions/`
- Status: `python .control/scripts/pctl.py status`

Control panel:

```
python .control/frontend/server.py
```

Then open http://localhost:8420.

## Development

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cargo machete
python scripts/check_boundaries.py
```

The toolchain is pinned in `rust-toolchain.toml` (1.96.0). The same
checks run in `.github/workflows/ci.yml` on Windows and Linux.

## Conventions

All code, identifiers, comments, tests, messages and documentation are
in English. Duplication is avoided, helpers are documented, public APIs
carry rustdoc, and performance changes are measured. See
[engineering conventions](.control/docs/reference/REF-0002.md).

## License

Motioner is free and open source under the
[Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution.
