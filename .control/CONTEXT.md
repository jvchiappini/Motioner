---
actualizado: 2026-10-31
actualizado_por: agente
version_schema: 1
---

# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is a brand-new codebase started from an empty directory. It is
an AI-first motion-graphics editor and NLE: prompt-driven creation plus
an After Effects-style editor, node graph, full audio mixer, advanced
3D, code (MDF) and automatic AI iteration. It renders on the custom Rust
FerrousEngine and uses Ferrous UI plus GUIMaker for editor chrome.
Neither engine nor GUIMaker is in the repository yet; they are pinned
when the user provides them.

The architecture gate is closed (D-0001..D-0009 accepted) and the Cargo
workspace is scaffolded with six crates (core, eval, render, io, app,
testkit). UI, DSL and AI crates are deferred to their phases. No product
features exist yet.

Product scope expanded on 2026-10-31 and is captured in REF-0003:
- Three audiences, AI-first and a fine-control editor.
- NLE: cut/trim/edit video and audio.
- Full audio mixer with effects.
- Full AE parity except importing AE projects; node graph included.
- 3D: models, skeletal animation, particles, physics.
- Import images/video/audio/SVG/glTF; export MP4 and PNG; upload to
  YouTube.
- User-visible and editable code.
- AI providers: Anthropic, OpenRouter, Google; automatic iteration is
  core.
- Windows first, GPU-optimized, 1080p 60 fps preview.
- Quality drops only while scrubbing.
- Public and free, with a revenue path.
- Solo developer; CI without a GPU.

The old Motioner repository is `Motioner-legacy`; the new one is
`jvchiappini/Motioner`. The legacy repo was not deleted because the gh
token lacks `delete_repo`.

## Reference machine

AMD Ryzen 5 8645HS (6C/12T), 8 GB RAM (7.3 usable), NVIDIA RTX 4050
Laptop 6 GB VRAM, AMD Radeon 760M iGPU, Windows 11 build 26200, 512 GB
NVMe, 1920x1080 display. Hybrid graphics: the app must request the
high-performance adapter. RAM and VRAM are the main constraints.

## Tacit conventions detected

- The user writes Spanish; all repository content is English.
- Code quality is a hard requirement (REF-0002): no duplication, named
  helpers, rustdoc, measured optimization.
- Prefixed task IDs by domain (`T-ARCH`, `T-CORE`, `T-NLE`, ...).
- Document is truth; MDF is the lossless text projection; no RON/JSON.
- Determinism: integer frames, rational fps, pure evaluation.
- Every mutation goes through typed commands.
- Full parity is the north star; scope staging is a planning device, not
  a permanent limitation.

## Things to watch out for (gotchas)

- `origin` is `jvchiappini/Motioner`, not PCS. PCS `update.py` expects
  the PCS repo in `origin`; temporarily point `origin` at a `pcs` remote
  to update the framework. Never push PCS changes to Motioner.
- The old repo remains public as `Motioner-legacy` until deleted
  manually or with `delete_repo` scope.
- `rust-toolchain.toml` pins 1.96.0; do not reintroduce nightly features.
- FerrousEngine and GUIMaker are absent; do not download them early.
- Workspace lints deny `unsafe_code`, warn on `missing_docs`, run clippy
  `all` and `pedantic`; CI denies warnings.
- `scripts/check_boundaries.py` must be updated for every new workspace
  crate, or CI fails by design.
- CI has no GPU: rendering must be testable via a CPU oracle and manual
  GPU golden frames on the reference machine.
- 8 GB RAM / 6 GB VRAM make 4K intermediates, caches and AI previews
  tight; budget and degrade gracefully.
- "Everything must be allowed" is not a legal policy for bundled assets
  or AI terms; this still needs a precise rule.
- `pctl` section names/commands are fixed Spanish framework strings;
  project content stays English.

## Perceived status

Architecture and workspace are solid, but the product scope is now much
larger than the original roadmap. The roadmap is explicitly under
revision, and several decisions are still open: 1.0 staging, license and
revenue, asset/AI licensing, MDF syntax, node graph role, AI privacy and
iteration budgets, NLE/audio depth, physics engine, YouTube flow, UI
language and packaging.

Phase 1 is still open: MDF v0 spec (T-FORMAT-0001) is unblocked; engine
pinning and deterministic frame (T-ARCH-0005, T-RENDER-0001) wait for
the FerrousEngine and GUIMaker repositories.

## Last relevant session

S-2026-10-31-004 - captured expanded product requirements in REF-0003,
updated goals and roadmap status, recorded reference hardware.
