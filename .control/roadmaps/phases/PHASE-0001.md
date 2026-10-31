---
id: PHASE-0001
nombre: "Foundations and contracts"
orden: 1
estado: in_progress
inicio: 2026-10-31
fin_estimado: ""
fin_real: null
depende_de: []
responsable: agente
prioridad: critica
---

# Phase: Foundations and contracts

## Summary

Freeze the architectural decisions that are expensive to reverse, then
stand up the skeleton that every later phase depends on: the Cargo
workspace, the pinned FerrousEngine integration, the project file format,
the deterministic frame contract, and CI. No product feature ships in
this phase; the deliverable is a set of approved decisions plus a
running, testable skeleton.

The phase ends when a minimal project file can be loaded and rendered
to a deterministic still frame through the same pipeline that will later
drive the editor viewport and the video exporter.

## Goals

- Approve the core ADRs (source of truth, UI toolkit, engine pinning,
  project format, determinism) before any feature work.
- Establish the crate boundaries and dependency direction so the
  document core never depends on GPU or UI code.
- Prove that the renderer can produce the same frame twice from the same
  project file.

## Scope

### In scope
- Workspace layout, crate skeleton, formatting/lint/CI.
- FerrousEngine pinning strategy and a hello-viewport spike.
- Project format v0 schema with a load/save round-trip.
- Determinism contract: frame-based time, fixed evaluation inputs,
  golden-frame harness.
- Decision records and the living system blueprint.

### Out of scope
- Editor panels, timeline UI and interaction.
- Animation evaluation beyond a constant static frame.
- 3D scenes, effects, AI and export encoding.

## Dependencies

- FerrousEngine must expose an offscreen render target and a stable
  pinned revision; it is not vendored yet.

## Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| FerrousEngine API churn breaks the host | High | High | Pin a commit; keep the engine boundary behind one adapter crate; prefer small upstreamable patches |
| UI toolkit choice locks the project | Medium | High | Decide in D-0004 before building panels; prototype the riskiest widgets first |
| Determinism claims fail on real GPUs | Medium | High | Golden frames plus a CPU-side eval oracle; document tolerance policy |
| Over-modularization slows momentum | Medium | Medium | Start with fewer crates; split only when a boundary is proven |

## Key decisions

- D-0001: FerrousEngine as the rendering foundation.
- D-0002: deterministic frame evaluation shared by preview and export.
- D-0011: program-first executable MDF (supersedes D-0003).
- D-0004: editor UI toolkit.
- D-0005: project format and asset storage.
- D-0006: document/runtime separation and caching model.

## Success criteria

- [ ] ADR set D-0001..D-0006 is approved or superseded.
- [ ] `cargo build`, `cargo fmt --check` and `cargo clippy` pass on an
      empty but wired workspace.
- [ ] A project file loads, saves without loss and validates.
- [ ] The same project file renders the same still frame on two runs.
- [ ] CI runs the workspace checks and the golden-frame test.

## Initiatives

- INITIATIVE-0001 - Architecture decisions and contracts
- INITIATIVE-0002 - Workspace, engine integration and CI
- INITIATIVE-0003 - Project format and determinism spike

## Notes

This phase is deliberately front-loaded. Reversing the source-of-truth
or UI-toolkit decisions later would cost months, so they are recorded as
ADRs and confirmed with the user before implementation starts.
