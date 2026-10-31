---
id: INITIATIVE-0002
nombre: Workspace, engine integration and CI
phase: PHASE-0001
estado: in_progress
prioridad: alta
tamano_estimado: medium
depende_de: [INITIATIVE-0001]
creado: 2026-10-31
responsable: agente
---
# Initiative: Workspace, engine integration and CI

## Description

Create the Cargo workspace, the crate skeleton and the integration with
FerrousEngine, plus the automated checks that keep the tree healthy.
The goal is a clean, reproducible build on a fresh machine with a
dependency direction that protects the architecture.

## Scope

### Deliverables
- Workspace `Cargo.toml`, toolchain pin, shared lint/format config.
- Crates: core document, evaluator, render adapter, UI, AI, IO, app
  binary, testkit. Start with only what Phase 1 needs.
- FerrousEngine pin: submodule or fork branch with a documented update
  procedure.
- Engine adapter crate isolating engine types from Motioner types.
- CI running fmt, clippy, tests and build on Windows and Linux.
- License and contribution notes.

### Non-goals
- Feature implementation.
- Publishing crates.
- Multi-platform packaging and installers.

## Technical approach

Dependencies flow in one direction: app -> ui/ai/io -> render -> eval ->
core. The core crate must not depend on wgpu, FerrousEngine, UI or AI.
Engine types are converted at the adapter boundary so the rest of the
codebase can be tested without a GPU.

Key considerations:
- Pin the engine to a commit and record how to update it.
- Keep patches to the engine small and upstreamable.
- CI must not require a real GPU for unit tests; golden render tests are
  separate and may be manual or use a software adapter.

## Dependencies

- INITIATIVE-0001: crate boundaries and engine pinning decision.
- FerrousEngine repository access.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0002 | First deterministic frame from a project file | | backlog |

## Tasks

- T-ARCH-0004 - Scaffold workspace and crate skeleton
- T-ARCH-0005 - Pin FerrousEngine and document update flow
- T-ARCH-0006 - Set up CI, lint and format gates

## Acceptance criteria

- [ ] A fresh clone builds with one documented command.
- [ ] CI passes on Windows and Linux.
- [ ] The core crate has no GPU, UI or AI dependency.
- [ ] The engine revision is pinned and documented.

## Notes

The engine API is expected to move. Treat the adapter as the blast
radius and avoid leaking engine types into the document model.
