---
id: T-ARCH-0005
titulo: Pin FerrousEngine and document the update flow
estado: backlog
prioridad: alta
tipo: chore
creado_por: agente
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-10-31
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

FerrousEngine is not in this repository yet and must not be downloaded
until the user approves. When it is added, pin an exact revision and
document how to update it.

Choose and record the mechanism: git submodule with a Motioner branch, a
fork referenced by git revision, or a local path during development.
The adapter crate (`motioner_render`) must be the only place that imports
engine types.

## Criterios de aceptacion
- [ ] The engine revision is recorded in the repository.
- [ ] A documented update procedure exists, including how to carry local
      patches.
- [ ] Engine types do not leak outside the adapter crate.

## Notas del agente

