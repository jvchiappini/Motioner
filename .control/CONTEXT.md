---
actualizado: 2026-10-31
actualizado_por: agente
version_schema: 1
---
# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is an AI-first motion-graphics editor and NLE in Rust on the
custom FerrousEngine (wgpu). Program-first MDF (Python-like DSL) builds
the composition. Scope and decisions: REF-0001..0004, D-0010..D-0017.

## Working model (from 2026-10-31, session 008)

- Code is written by opencode with DeepSeek V4.1 Flash.
- Claude acts as architect and auditor: writes self-contained English
  prompts for opencode, reviews diffs, runs checks, and only makes small
  fixes. Conversation with the user is in Spanish.
- Goals include improving FerrousEngine and GUIMaker upstream with small
  patches, then bumping the pin in Motioner with an ADR.

## Current state (branch feat/m1-foundations, not pushed)

- Committed on `feat/m1-foundations` (5 commits after 757c0b5): engine pin
  (61da7f9, D-0018, `engine/README.md`), `motioner_core`, `motioner_eval`,
  `motioner_dsl`, `examples/hello.motion`. `main` is untouched; fast-forward
  when the user approves.
- Uncommitted on purpose: `crates/motioner_render/Cargo.toml` engine deps,
  `anyhow`/`png`/`sha2` workspace deps and the matching `Cargo.lock`. They
  land with the adapter/CLI so `cargo machete crates` stays green.
- All gates green: fmt (no `--all`), clippy -D warnings, 52 tests, doc,
  boundaries, machete scoped to `crates`.
- T-ARCH-0005 done. T-DSL-0003/0004 stay in_progress until the hardening
  pass fixes the review blockers/majors (clamp NaN panic, unbounded flat
  expression depth, value cycles/depth, string iteration limit, i64 frame
  overflow, non-finite values). Minor findings live in T-DSL-0005.
- T-RENDER-0002 in_progress: adapter, ffmpeg export (motioner_io) and CLI
  (motioner_app) not started.

## FerrousEngine improvement candidates (upstream patches)

Verified at 02e6d48:
- `OffscreenTarget::read_rgba8` (`engine/ferrous/crates/ferrous_renderer/src/headless.rs:107`)
  allocates a GPU buffer and a Vec per call and panics on map failure.
  Proposed: `read_rgba8_into(&mut Vec<u8>)` with a persistent staging
  buffer, typed errors, and double-buffered async readback.
- Text: `ferrous_font` already has a font parser and MSDF atlas
  (`engine/ferrous/crates/ferrous_font/src/msdf_gen.rs:227`). Pure2D text
  should reuse it upstream instead of a Motioner-side CPU rasterizer
  (supersedes the earlier CPU-coverage plan; avoids duplicate font code).
- Blending is straight `ALPHA_BLENDING` only (`engine/ferrous/crates/ferrous_2d/src/render/pipeline.rs:102`);
  Motioner needs premultiplied alpha, blend modes and offscreen layers for
  group opacity.
- `render_to_view` requires the default `gui` feature; headless 2D pulls
  GUI/winit. `Pure2dHarness::new` returns `Result<_, String>`.
- `static mut` counters in `engine/ferrous/crates/ferrous_ui_core/src/render_collector.rs:13`
  (unsound), SSAO uses `thread_rng` (non-deterministic 3D), many warnings,
  fmt not clean, no test/clippy CI in the engine repo.

## Gotchas

- CI `RUSTFLAGS=-D warnings` will fail on engine path deps; use
  `submodules: true` and rely on `clippy -D warnings` instead.
- `scripts/check_boundaries.py` bans ferrous/wgpu prefixes everywhere:
  exempt motioner_render and add a motioner_dsl rule.
- Git Bash rewrites '/x' args: use `MSYS_NO_PATHCONV=1` for
  `git sparse-checkout`.
- Bash heredocs with mixed quotes broke; write files with the editor.
- Disk: ~20 GB free on C: (2026-10-31). Stop below 8 GB. Use CARGO_INCREMENTAL=0.
- D-0003 superseded by D-0011; MDF v0 subset frozen; `while`, `use`,
  dicts, overrides and LaTeX deferred to v0.1.
- `origin` is jvchiappini/Motioner; do not touch Motioner-legacy; no
  push without permission.

## Reference machine

Ryzen 5 8645HS, 8 GB RAM, RTX 4050 Laptop 6 GB + Radeon 760M, Windows 11.

## Last relevant session

S-2026-10-31-009 (open) - audited opencode's green-CI pass, committed the
foundations on feat/m1-foundations, closed T-ARCH-0005, created T-DSL-0005,
reviewed FerrousEngine and issued the hardening prompt.
