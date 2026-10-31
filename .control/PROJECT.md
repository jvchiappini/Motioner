# Project: Motioner

## Description
A code-driven motion graphics and video editor that turns prompts and
code into professional 2D/3D videos and provides NLE editing for video
and audio, with an After Effects-style timeline, a node graph, and an
AI-first workflow built on the custom Rust FerrousEngine renderer.

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
- ffmpeg for video/audio decode and encode.
- Audio engine and mixer, physics and particle systems (libraries to be
  decided by spike).
- AI providers: Anthropic, OpenRouter and Google over HTTPS.
- YouTube Data API for direct upload.
- Windows first; the reference machine is documented in REF-0003.

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
`T-AI`, `T-NLE`, `T-AUDIO`, `T-PHYS`, `T-PUB`, `T-APP`, `T-PERF`.

Example: `T-CORE-0001`.

## General status
Design gate closed on 2026-10-31: D-0001..D-0009 accepted. The
repository is a brand-new Motioner project that replaces the legacy
Motioner repository. The Cargo workspace is scaffolded with six core
crates and green CI; no product features exist yet.

Product scope expanded on 2026-10-31: NLE video/audio editing, full
audio mixer, node graph, advanced 3D (particles/physics), AI automatic
iteration, YouTube upload, public free distribution with a revenue path.
See `docs/reference/REF-0003.md`. The 1.0 staging, license, MDF syntax,
AI privacy and asset policy are still open.

FerrousEngine and GUIMaker are not vendored in this repository yet and
will be pinned when provided. The reference machine is a Ryzen 5 8645HS
with 8 GB RAM and an RTX 4050 Laptop (6 GB VRAM) on Windows 11.
