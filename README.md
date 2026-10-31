# Motioner

A code-driven motion-graphics editor and NLE with an After Effects-style
timeline, a node graph, audio mixing and an AI agent, built on the
custom Rust **FerrousEngine** renderer. Motioner turns prompts and
Python-like code into professional 2D/3D videos, with a deterministic
pipeline shared by preview and export.

## Status

**Foundation phase.** The architecture gate is closed and the Cargo
workspace is scaffolded with six core crates. There are no editor
features yet. Next up: the MDF format specification and the first
deterministic frame spike. FerrousEngine and GUIMaker are not vendored
into this repository yet.

## What it will do

- Author compositions with a timeline, viewport, inspector and node
  graph.
- Edit video and audio on an NLE timeline, with a stereo mixer and
  effects.
- Describe compositions as Python-like executable code (MDF, the
  Motioner Document Format) with loops, functions and conditionals.
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

Full details: [system blueprint](.control/docs/reference/REF-0001.md).

## Repository layout

```
crates/
  motioner_core/      document model, IDs, commands, undo
  motioner_eval/      time, curves, flattening into render plans
  motioner_render/    FerrousEngine adapter, compositor, effects
  motioner_io/        MDF project format, assets, ffmpeg export
  motioner_app/       application binary and wiring
  motioner_testkit/   fixtures, golden frames, benchmarks
  motioner_ui/        editor shell and panels on GUIMaker (Phase 3)
  motioner_dsl/       grammar, parser, printer, sandbox (Phase 5)
  motioner_ai/        providers, tools, context builder (Phase 5)
.control/             project control system (source of truth for state)
.opencode/            agent configuration
engine/               FerrousEngine and GUIMaker, pinned later
scripts/              repository checks
```

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
