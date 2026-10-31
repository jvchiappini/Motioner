---
id: INITIATIVE-0005
nombre: "Timeline and animation evaluation"
phase: PHASE-0002
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0004"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Timeline and animation evaluation

## Description

Implement the generative half of the editor: a composition timeline and
a pure evaluator that resolves a frame into a flat render plan. Every
consumer (viewport, exporter, thumbnails, AI preview) calls the same
evaluator, which is what makes preview and final render agree.

## Scope

### Deliverables
- Timeline model: layer in/out, time stretch, offset, markers, parenting,
  local vs composition time.
- Animatable scalar/vector/color/transform properties and keyframes.
- Interpolation modes: hold, linear, bezier with editable handles.
- Easing presets and curve sampling with caching.
- Transform propagation and visibility resolution.
- Flat render-plan output for the renderer.
- Expression hook points with a deterministic, sandboxed runtime.

### Non-goals
- Editing UI for keyframes (INITIATIVE-0008).
- GPU effects.
- Full expression language design (INITIATIVE-0013).

## Technical approach

Time is an integer frame plus a rational fps. Evaluation is a pure
function of document revision, frame and render settings; no wall clock,
no accumulated floats. Curves are sampled once per frame and cached by
property revision. The evaluator flattens the hierarchy into a
cache-friendly array of render items with resolved transforms, opacity
and effect parameters.

Key considerations:
- Avoid per-frame heap allocation; reuse scratch buffers.
- Define semantics precisely for nested comps, time remap and parenting.
- Keep evaluation independent of the renderer and UI.

## Dependencies

- INITIATIVE-0004: document model and IDs.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0003 | Ten-second 2D composition animates headlessly | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] A 10-second nested composition evaluates correctly at every frame.
- [ ] Evaluation is deterministic and allocation-free per frame.
- [ ] Timeline semantics are documented and covered by tests.
- [ ] The render plan is stable across preview and export.

## Notes

The evaluator is the architectural contract between editing and
rendering. Treat its output schema as public within the project.
