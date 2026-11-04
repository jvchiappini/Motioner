---
id: T-RENDER-0014
titulo: 3D layers: glTF, cameras, lights, PBR and skeletal animation composited with 2D
estado: backlog
prioridad: alta
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

The 1.0 V3 criterion: 3D layers with glTF models, cameras, lights, PBR,
skeletal animation and GPU particles composited with 2D layers. FerrousEngine
already ships the 3D passes (PBR, prepass, shadows, skinning); Motioner needs
the document/plan surface, the loader and the ordering with the 2D path.

## Criterios de aceptacion
- [ ] `camera`, `light` and `mesh(source)` document nodes with MDF syntax
- [ ] glTF import through `motioner_io`, content-hashed and budgeted
- [ ] PBR, lights and shadows through the engine 3D passes
- [ ] Skeletal animation sampled by the deterministic evaluator
- [ ] 2D and 3D layers interleave in one painter order and one export

## Notas del agente

Backlogged at the end of prompt 10 (Phase 12). Design the plan boundary before
touching the engine.
