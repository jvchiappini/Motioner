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
- Native Motioner Document Format (MDF): a program-first, Python-like
  executable language owned by the project, shared by the on-disk files,
  the code panel and the AI. The compiled document carries provenance
  and overrides. LaTeX math typesetting is included in 1.0.
- ffmpeg for video/audio decode and encode.
- Audio engine and mixer, physics and particle systems (libraries to be
  decided by spike).
- AI providers: Anthropic, OpenRouter and Google over HTTPS.
- Manual export for YouTube; no YouTube API integration.
- Apache License 2.0.
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

Product scope expanded on 2026-10-31 and 1.0 approved: NLE video/audio
editing, full audio mixer (no advanced mastering), bidirectional node
graph, 3D with particles (rigid-body physics after 1.0), AI automatic
iteration with human approval, Python-like executable MDF code, public
free distribution under Apache-2.0 with a revenue path. Video upload is
manual; Motioner exports a YouTube-ready MP4.

Resolved on 2026-10-31: program-first custom Python-like DSL, LaTeX
math in 1.0, Apache-2.0, revenue from donations/sponsors plus paid cloud
rendering and AI credits, iteration budget 10/10/configurable, English
UI, MSI/NSIS plus portable ZIP, no auto-update, no YouTube upload.
Remaining items are implementation details listed in REF-0003 section 9.
The draft language spec is `spec/mdf-v0.md`.

FerrousEngine and GUIMaker are not vendored in this repository yet and
will be pinned when provided. The reference machine is a Ryzen 5 8645HS
with 8 GB RAM and an RTX 4050 Laptop (6 GB VRAM) on Windows 11; 1080p is
the primary target and 4K is deferred.
