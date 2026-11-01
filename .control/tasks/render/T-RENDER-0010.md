---
id: T-RENDER-0010
titulo: Effects: drop shadow, blur and glow with keyable parameters
estado: backlog
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-01
depende_de: [T-CORE-0001]
bloqueado_por: null
version_schema: 1
---


Soft shadows, blurs and glows are the difference between flat shapes and a
studio look. Phase 5 adds `node.effect(kind, **params)` with an ordered stack
that applies to the node and its subtree.

## Design

- Core: `Node.effects`, each effect a named parameter set using normal
  animatable channels (`drop_shadow.blur`, ...). D-0028 fixes the stack model.
- Engine Part C: offscreen layer targets, separable Gaussian blur with a fixed
  deterministic kernel, composited back with opacity and blend mode.
- Editor: Effects section in the inspector (add/remove/reorder commands,
  params as fields with stopwatches).
- Tests: blur kernel unit tests; golden stills of shadow, blur and glow.

## Criterios de aceptacion
- [ ] Effects parse, key, override, bake and query
- [ ] Render applies them with a deterministic kernel, or typed `Unsupported`
- [ ] Three golden stills and kernel tests
- [ ] Inspector add/remove/reorder commands

## Criterios de aceptacion
- [ ] 

## Notas del agente

