---
actualizado: 2026-10-31
actualizado_por: agente
version_schema: 1
---

# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is a brand-new codebase started from an empty directory. It is
an AI-first motion-graphics editor and NLE: prompt-driven creation plus
an After Effects-style editor, node graph, audio mixer, 3D, program-first
Python-like code (MDF), LaTeX math and automatic AI iteration. It renders
on the custom Rust FerrousEngine and uses Ferrous UI plus GUIMaker for
editor chrome. Neither engine nor GUIMaker is in the repository yet; they
are pinned when the user provides them.

The architecture gate is closed, the workspace has six crates (core,
eval, render, io, app, testkit), and CI is green. No product features
exist yet.

## Confirmed decisions (2026-10-31)

- 1.0 is Windows-only and 1080p-first (D-0010, D-0014).
- 2D, AE-style timeline, masks, mattes, precomps, NLE, audio mixer,
  node graph, 3D glTF/skeletal/GPU particles (D-0010).
- Program-first MDF: a custom Python-like DSL with loops, functions,
  conditionals and LaTeX; the program is the truth; the document is
  compiled with provenance and overrides (D-0011, supersedes D-0003).
- LaTeX math typesetting in 1.0, self-contained and deterministic
  (D-0016).
- AI: Anthropic, OpenRouter and Google; cloud data allowed; previews and
  visual critique; 10 iterations / 10 minutes / configurable cost cap;
  human approval for the final render (D-0012, D-0017).
- No YouTube API and no upload from the app; the user downloads the MP4
  (D-0012, D-0017).
- Apache-2.0; public and free; bundled assets permissive; users
  responsible for imports (D-0013).
- Revenue: donations/sponsors, paid cloud rendering, paid AI credits
  (D-0015).
- Audio 1.0: volume, pan, fades, EQ, compressor, limiter; no reverb or
  delay (D-0017).
- UI English only; MSI/NSIS plus portable ZIP; no auto-update (D-0017).
- Physics staged: particles in 1.0, rigid bodies post-1.0, cloth/fluids
  later (D-0010, D-0017).
- Solo developer; CI has no GPU (REF-0002, D-0017).

## Reference machine

AMD Ryzen 5 8645HS (6C/12T), 8 GB RAM (7.3 usable), NVIDIA RTX 4050
Laptop 6 GB VRAM, AMD Radeon 760M iGPU, Windows 11 build 26200, 512 GB
NVMe, 1920x1080 display. Hybrid graphics: request the high-performance
NVIDIA adapter. RAM and VRAM are the main constraints.

## Tacit conventions detected

- The user writes Spanish; all repository content is English.
- Code quality is a hard requirement (REF-0002): no duplication, named
  helpers, rustdoc, measured optimization.
- Prefixed task IDs by domain (`T-ARCH`, `T-CORE`, `T-NLE`, ...).
- Determinism: integer frames, rational fps, pure evaluation.
- Every mutation goes through typed commands.
- Program-first: MDF is the source; UI edits are overrides or bakes.
- The user wants to surpass Manim with executable code and LaTeX.

## Things to watch out for (gotchas)

- D-0003 is superseded by D-0011. Do not reintroduce "document is the
  truth"; the program is the truth and the document is compiled with
  provenance.
- The override/bake model is the hardest part of the editor. Orphans must
  be reported, never dropped.
- `spec/mdf-v0.md` is a draft; the grammar and scene API can still
  change. Review before implementing the interpreter.
- `origin` is `jvchiappini/Motioner`, not PCS. PCS `update.py` expects the
  PCS repo in `origin`; temporarily point it at a `pcs` remote.
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

Product and licensing decisions are closed, the roadmap has 22
initiatives, and the first draft of the language spec exists at
`spec/mdf-v0.md`. The next critical design step is reviewing that spec
and resolving override/bake semantics. Implementation still waits for
FerrousEngine and GUIMaker; meanwhile MDF v0 and project/format specs can
advance without the engine.

## Last relevant session

S-2026-10-31-006 - confirmed program-first MDF, custom DSL, LaTeX,
revenue channels and 1.0 defaults; wrote the MDF v0 draft spec and
updated all references.
