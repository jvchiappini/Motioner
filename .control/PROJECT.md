# Project: Motioner

## Description
A code-driven motion graphics editor that turns prompts and code into
professional 2D/3D videos, with an After Effects-style timeline UI built
on the custom Rust FerrousEngine renderer.

## Type
Native desktop creative application: real-time editor plus deterministic
headless renderer.

## Main stack
- Rust, Cargo workspace, edition 2021.
- FerrousEngine (custom in-house engine, wgpu-based, modifiable) for
  rendering, scene/ECS and the editor host.
- Ferrous UI plus GUIMaker (separate project, engine-native) for editor
  chrome. No egui.
- Native Motioner Document Format (MDF): text-based, owned by the
  project, shared by the on-disk format and the code layer. No RON/JSON
  as canonical format.
- ffmpeg for video muxing/encoding; AI providers over HTTPS.

## Engineering conventions
All code, identifiers, comments, tests, messages and documentation are
in English, even though the user and the agent converse in Spanish.
Always optimize, measure before and after, avoid duplicated logic,
extract well-named helpers, document public contracts with rustdoc and
keep the codebase as clean as possible. Full standard in
`docs/reference/REF-0002.md`.

## ID convention
Prefixed task IDs by domain, because the codebase is a multi-crate
workspace from day one:
`T-ARCH`, `T-CORE`, `T-EVAL`, `T-RENDER`, `T-UI`, `T-IO`, `T-DSL`,
`T-AI`, `T-APP`, `T-PERF`.

Example: `T-CORE-0001`.

## General status
Design gate closed on 2026-10-31: D-0001..D-0009 accepted. The
repository is a brand-new Motioner project that replaces the legacy
Motioner repository. No application code exists yet. FerrousEngine and
GUIMaker are not vendored in this repository yet and will be pinned when
provided.
