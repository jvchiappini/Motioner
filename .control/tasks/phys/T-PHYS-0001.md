---
id: T-PHYS-0001
titulo: GPU particles and rigid-body physics (post-1.0 spike)
estado: backlog
prioridad: media
tipo: feature
creado_por: usuario
asignado_a: agente
creado: 2026-11-03
actualizado: 2026-11-03
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

GPU particles are part of the 1.0 scene surface; rigid-body physics, cloth and
fluids are explicitly post-1.0. The engine has compute particle passes; the
document needs a deterministic emitter (seeded, frame-addressed) that exports
the same simulation as the preview.

## Criterios de aceptacion
- [ ] `particles(count, seed, lifetime, ...)` node with MDF and overrides
- [ ] Deterministic, frame-addressed emission (no wall clock)
- [ ] GPU compute simulation matches the preview and the export
- [ ] Composite with 2D layers and respect masks/effects
- [ ] Spike the post-1.0 rigid-body backend and record the ADR

## Notas del agente

Backlogged at the end of prompt 10 (Phase 12). Rigid body is post-1.0; particles
are v1.
