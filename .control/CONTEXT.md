---
actualizado: 2026-10-31
actualizado_por: agente
version_schema: 1
---

# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is a brand-new codebase started from an empty directory. It is
a code-driven 2D/3D motion-graphics editor with an After Effects-style
timeline, an AI agent and a deterministic renderer. Rendering and UI are
built on the custom Rust FerrousEngine, with the engine-native UI stack
plus GUIMaker for editor chrome. Neither engine nor GUIMaker is in the
repository yet; both are pinned later when the user provides them.

The architecture gate is closed: D-0001..D-0009 are accepted. The
Cargo workspace is scaffolded with six crates (core, eval, render, io,
app, testkit); UI, DSL and AI crates are deferred to their phases. No
product features exist yet.

The old Motioner GitHub repository was replaced. It was renamed to
`Motioner-legacy` because the current gh token has no `delete_repo`
scope; the new repository is `jvchiappini/Motioner` (public), pushed
from this folder. The legacy repo is prior art only and is not ported;
T-ARCH-0001 captured its reusable ideas (DSL structure, code panel).

## Tacit conventions detected

- The user writes in Spanish; all repository content must be English.
- Code quality is a hard requirement: no duplicated logic, well-named
  helpers, extensive rustdoc, measured optimization. See REF-0002.
- IDs are prefixed by domain (`T-ARCH`, `T-CORE`, `T-RENDER`, ...).
- Documentation is lazy: document the domain being worked, not
  everything in advance.
- The document is the truth; MDF is a lossless text projection shared by
  the project file and the code panel. There is no RON/JSON.
- Determinism is a contract: integer frames, rational fps, pure
  evaluation, golden frames.
- Every mutation goes through typed commands, for UI, code and AI.

## Things to watch out for (gotchas)

- `origin` now points to `jvchiappini/Motioner`, not PCS. PCS
  `update.py` expects the PCS repository in `origin`; to update PCS,
  temporarily add a `pcs` remote and point `origin` at it, or adapt the
  updater. Do not push PCS framework changes to Motioner.
- The old repo is still public as `Motioner-legacy`. Delete it manually
  or grant `delete_repo` scope if the user wants it gone.
- The local rustup override to a nightly toolchain was removed;
  `rust-toolchain.toml` pins 1.96.0. Do not reintroduce nightly-only
  features.
- FerrousEngine and GUIMaker are not present. Do not download them
  until the user provides the repositories.
- The engine API is young; keep it behind `motioner_render` and GUIMaker
  behind `motioner_ui`.
- Workspace lints deny `unsafe_code`, warn on `missing_docs`, and run
  clippy `all` and `pedantic`; CI denies warnings.
- `scripts/check_boundaries.py` must be updated when a new workspace
  crate is added, or CI fails by design.
- `pctl` task/session section names and commands are fixed Spanish
  strings from the framework; project content stays English.
- PCS templates may contain mojibake em dashes; prefer plain ASCII in
  new content.

## Perceived status

Solid foundation. The architecture is approved, the repository identity
is fixed, and the workspace builds cleanly. CI is green on Windows and
Linux with format, boundary, unused-dependency, clippy, test, build and
rustdoc gates (T-ARCH-0006 done). Remaining Phase 1 work: specify MDF v0
(T-FORMAT-0001), pin the engine and GUIMaker (T-ARCH-0005, blocked), and
prove deterministic still-frame rendering (T-RENDER-0001, blocked on the
engine).

## Last relevant session

S-2026-10-31-003 - finished CI gates, fixed unused scaffold dependencies,
verified green runs on both operating systems.
