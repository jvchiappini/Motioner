---
id: T-FORMAT-0002
titulo: Implement project load and save round-trip
estado: done
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-11-11
depende_de: []
bloqueado_por: null
version_schema: 1
---
## Contexto

Implement the loader and saver for project format v0 against the spec
from T-FORMAT-0001. The round-trip must be lossless, deterministic and
produce actionable errors for invalid projects.

Include fixture projects covering a minimal composition, assets with
relative paths, and an older schema version for migration.

## Criterios de aceptacion
- [ ] Fixtures load, save and reload without loss.
- [ ] Serialization order is deterministic across runs.
- [ ] Invalid files fail with precise, actionable errors.
- [ ] Round-trip tests run in CI.

## Notas del agente

