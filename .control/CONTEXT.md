---
actualizado: 2026-10-31
actualizado_por: agente
version_schema: 1
---

# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is a brand-new codebase started from an empty directory. It is
an AI-first motion-graphics editor and NLE: prompt-driven creation plus
an After Effects-style editor, node graph, audio mixer, 3D, Python-like
code (MDF) and automatic AI iteration. It renders on the custom Rust
FerrousEngine and uses Ferrous UI plus GUIMaker for editor chrome.
Neither engine nor GUIMaker is in the repository yet; they are pinned
when the user provides them.

The architecture gate is closed (D-0001..D-0009 accepted). The Cargo
workspace is scaffolded with six crates (core, eval, render, io, app,
testkit) and green CI. No product features exist yet.

## Confirmed product decisions (2026-10-31)

1.0 is Windows-only and 1080p-first, approved in D-0010 and REF-0003:

- 2D motion graphics, AE-style timeline, masks, mattes, precomps.
- NLE: cut, trim, arrange, transitions and sync for video and audio.
- Audio: stereo mixer with volume, pan, fades, EQ, compressor and
  limiter; no advanced mastering.
- 3D: glTF, cameras, lights, PBR, skeletal animation, GPU particles.
- Physics staged: rigid bodies after 1.0; cloth and fluids later.
- Node graph as an alternate view over the same document.
- Python-like executable MDF with loops and control flow; the exact
  semantics are still open (D-0011).
- AI: Anthropic, OpenRouter, Google; cloud data allowed; previews and
  visual critique; automatic iteration; human approval for final render.
- No YouTube API and no upload from the app: the user downloads the
  MP4 and uploads manually.
- MP4 and PNG output; 1080p primary; 4K/HDR/color grading later.
- Apache-2.0, public and free; versioned LICENSE and NOTICE added.
- Revenue model still open (D-0015): donations/sponsors/paid services.
- Solo developer; CI has no GPU.
- Bundled assets must be permissive; users are responsible for imports.

Engine and GUIMaker are absent. Do not download them early.

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
- Determinism: integer frames, rational fps, pure evaluation.
- Every mutation goes through typed commands.
- Full parity is the north star; the 1.0 scope is a staged subset.
- The user wants to surpass Manim with executable code, not only
  declarative descriptions.

## Things to watch out for (gotchas)

- D-0003 says the document is the truth and code is a projection, but
  D-0011 requires executable loops and logic. This is an unresolved
  architectural conflict; resolve it before implementing MDF.
- `origin` is `jvchiappini/Motioner`, not PCS. PCS `update.py` expects
  the PCS repo in `origin`; temporarily point it at a `pcs` remote.
- The old repo remains public as `Motioner-legacy` until deleted manually
  or with `delete_repo` scope.
- `rust-toolchain.toml` pins 1.96.0; do not reintroduce nightly features.
- Workspace lints deny `unsafe_code`, warn on `missing_docs`, run clippy
  `all` and `pedantic`; CI denies warnings.
- `scripts/check_boundaries.py` must be updated for every new workspace
  crate, or CI fails by design.
- CI has no GPU: rendering needs a CPU oracle plus manual GPU golden
  frames on the reference machine.
- 8 GB RAM / 6 GB VRAM: 1080p-first, proxies, pooling and budgets.
- Python bytecode (`__pycache__`) is ignored; do not commit it.
- `pctl` section names/commands are fixed Spanish framework strings;
  project content stays English.

## Perceived status

Architecture, licensing and 1.0 scope are solid. The roadmap now has 21
initiatives covering NLE, audio, node graph, particles and 1.0 delivery.
The critical unresolved item is MDF semantics: program-first executable
code versus document-with-scripts, and custom DSL versus embedded
Python. That decision blocks the language spec and part of the core
model.

Phase 1 is still open: MDF v0 spec (T-FORMAT-0001) is unblocked; engine
pinning and deterministic frame (T-ARCH-0005, T-RENDER-0001) wait for
the FerrousEngine and GUIMaker repositories.

## Last relevant session

S-2026-10-31-005 - confirmed 1.0 scope, added Apache-2.0 license,
recorded AI/no-upload policy, revised roadmap with NLE/audio/node
graph/particles/AI iteration/1.0 delivery initiatives.
