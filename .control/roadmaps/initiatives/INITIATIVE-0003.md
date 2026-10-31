---
id: INITIATIVE-0003
nombre: "Project format and determinism spike"
phase: PHASE-0001
estado: backlog
prioridad: alta
tamano_estimado: medium
depende_de: ["INITIATIVE-0002"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Project format and determinism spike

## Description

Define the on-disk project and prove the determinism contract with a
minimal end-to-end spike: load a project file, evaluate one frame, render
it offscreen through FerrousEngine, hash the output, and repeat to prove
the result is stable.

## Scope

### Deliverables
- Project format v0 specification: project metadata, compositions,
  assets and cache layout.
- Loader/saver with validation and readable error messages.
- Content-addressed asset manifest.
- Deterministic still-frame spike with golden hash.
- Written policy for frame time, color space and numeric tolerance.

### Non-goals
- Animation evaluation and timeline.
- Editor UI.
- Video encoding.

### Technical approach

Prefer a folder-based project with text serialization for VCS
friendliness, plus a single-file archive for sharing. Time is expressed
as integer frames with an explicit rational fps. Rendering uses a fixed
viewport, fixed color transform and fixed sampling so two runs can be
compared byte-for-byte or within a documented tolerance.

## Dependencies

- INITIATIVE-0002: workspace and engine integration.
- D-0002 and D-0005 approved.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0002 | First deterministic frame from a project file | | backlog |

## Tasks

- T-FORMAT-0001 - Specify project format v0
- T-FORMAT-0002 - Implement load/save round-trip
- T-RENDER-0001 - Spike offscreen deterministic still frame

## Acceptance criteria

- [ ] A fixture project loads, validates and saves without loss.
- [ ] Two runs produce the same frame hash.
- [ ] Invalid projects fail with actionable errors.
- [ ] The format is documented and versioned.

## Notes

If two runs differ, the cause must be identified before any feature work
continues. Determinism is a contract, not an aspiration.
