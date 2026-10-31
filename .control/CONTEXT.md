---
actualizado: 2026-10-31
actualizado_por: agente
version_schema: 1
---

# Context memory (maintained by the agent)

## What this project is, in practice

Motioner is starting from an empty directory as a fresh codebase. This
session installed the Project Control System (`.control/`) and produced
the pre-code design: PROJECT, GOALS, ROADMAP, the system blueprint
(`docs/reference/REF-0001.md`) and the engineering conventions
(`docs/reference/REF-0002.md`). There is no application code yet; that
is intentional.

The product is a code-driven motion-graphics editor with an After
Effects-style timeline, an AI chat that authors compositions, and a
deterministic offline renderer. Rendering and the application host are
built on the custom Rust FerrousEngine, which is NOT in this repository
yet and must not be downloaded until the user approves.

A legacy repository `jvchiappini/Motioner` exists (last push 2026-02):
an egui 0.26 / wgpu 0.19 prototype with a DSL, code panel and timeline.
It is prior art to review (T-ARCH-0001), not the new codebase.

## Tacit conventions detected

- The user writes in Spanish; all repository content must be English.
- Code quality is a hard requirement: no duplicated logic, well-named
  helpers, extensive rustdoc, measured optimization. See REF-0002.
- IDs are prefixed by domain (`T-ARCH`, `T-CORE`, `T-RENDER`, ...).
- Documentation is lazy: document the domain being worked, not
  everything in advance.
- The workspace is a multi-crate Cargo project; `core` must never depend
  on GPU, UI or AI.

## Things to watch out for (gotchas)

- The local folder is currently a clone of the PCS repository itself:
  `origin` points to `jvchiappini/ProjectControlSystem`. The git remote
  for Motioner still needs a decision; do not push to the PCS remote.
- FerrousEngine and legacy Motioner overlap but differ: the engine pins
  wgpu 23, winit 0.30 and ships its own UI stack, while legacy Motioner
  uses egui 0.26 and wgpu 0.19. Mixing versions is a real integration
  risk (D-0004).
- The engine API is young and moving; keep it behind
  `motioner_render`.
- `pctl` task/session section names and commands are fixed Spanish
  strings from the framework; that does not violate the English rule for
  project content.
- PCS templates and generated files contain some mojibake em dashes;
  prefer plain ASCII in new content.

## Perceived status

Design phase complete enough to review. Architecture is coherent but two
decisions block implementation: D-0003 (document vs code as truth) and
D-0004 (UI toolkit). The user also needs to decide the repository/remote
strategy.

## Last relevant session

S-2026-10-31-001 - installed PCS, wrote project identity, roadmap,
blueprint, ADR proposals, conventions and Phase 1 tasks.
