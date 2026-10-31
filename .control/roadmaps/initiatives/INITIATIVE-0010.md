---
id: INITIATIVE-0010
nombre: "3D scene integration"
phase: PHASE-0004
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0009"]
creado: 2026-10-31
responsable: agente
---

# Initiative: 3D scene integration

## Description

Bring FerrousEngine's 3D world, cameras, lights and PBR materials into
the composition model. A 3D layer becomes another compositable layer
with its own camera and light setup, evaluated and ordered alongside 2D
layers.

## Scope

### Deliverables
- 3D layer type with camera, lights, environment and render settings.
- Mapping from document nodes to the engine world and back.
- PBR materials, textures and environment lighting.
- glTF model import as an asset.
- 3D transforms, parenting and animated cameras.
- ID-buffer picking and 3D gizmos in the viewport.
- Depth and alpha output for compositing over 2D.

### Non-goals
- Physics, destruction and simulation.
- Global illumination and volumetrics.
- A full modelling tool.

## Technical approach

The evaluator emits a 3D render item referencing a camera and a light
set; the render adapter builds or incrementally updates the engine
world. Documents remain the truth; engine entities are runtime handles
derived from persistent IDs. Units and up-axis are fixed and documented
to avoid import surprises.

Key considerations:
- Minimize per-frame world rebuilds with dirty tracking.
- Define camera projection and depth semantics explicitly.
- Keep engine-specific types out of the document.

## Dependencies

- INITIATIVE-0009: 2D compositor and export.
- FerrousEngine 3D crates.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0006 | 3D layer composited over 2D with effects | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] A glTF model renders with a camera and light over 2D layers.
- [ ] Animated cameras and transforms evaluate deterministically.
- [ ] Picking selects the correct 3D object.
- [ ] Depth and alpha composite correctly.

## Notes

3D support must not fork the timeline or document into a separate mode.
