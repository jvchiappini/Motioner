---
id: INITIATIVE-0012
nombre: "Masks, mattes and precomps"
phase: PHASE-0004
estado: backlog
prioridad: media
tamano_estimado: large
depende_de: ["INITIATIVE-0011"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Masks, mattes and precomps

## Description

Add the structural compositing features motion designers rely on:
masks with feathering, track mattes driven by another layer, and nested
compositions with independent time and resolution.

## Scope

### Deliverables
- Shape and bezier masks with modes, invert, feather and expansion.
- Track mattes: alpha and luma, precomposed or live.
- Precomps: create, nest, collapse transforms, proxy and time controls.
- Editing UI for masks and precomp navigation.
- Cache keys that include time, mask revision and resolution.

### Non-goals
- Rotoscoping assistance and motion tracking.
- 3D masks beyond camera projection.
- Expressions on mask paths (later).

## Technical approach

Masks render into a mask buffer consumed by the layer compositor.
Mattes sample the matte layer output at the same frame. Precomps are
evaluated recursively with a cycle guard; results are cached by
composition revision, frame and settings.

Key considerations:
- Prevent cycles and unbounded nesting.
- Invalidate caches conservatively on any upstream change.
- Keep mask data animatable and serializable.

## Dependencies

- INITIATIVE-0011: render graph and compositing.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0006 | 3D layer composited over 2D with effects | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] Masks and mattes render identically in preview and export.
- [ ] Nested precomps animate and cache correctly.
- [ ] Cycles are rejected with a clear error.
- [ ] No cache returns stale frames after an edit.

## Notes

Precomp cache bugs are subtle. Prefer correctness over aggressive reuse
until the revision model is proven.
