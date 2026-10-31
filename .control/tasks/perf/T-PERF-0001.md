---
id: T-PERF-0001
titulo: Define performance budgets and benchmark plan
estado: backlog
prioridad: alta
tipo: investigacion
creado_por: agente
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-10-31
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

Performance is a design constraint, not a polish step. Define numeric
budgets before implementation: editor UI frame time, viewport preview fps
at proxy and full resolution, evaluation time per frame, frame-cache
memory, and VRAM ceiling for common resolutions.

Define the benchmark suite and the CI thresholds that fail on
regressions. Reference hardware must be recorded so numbers are
comparable.

## Criterios de aceptacion
- [ ] Budgets are documented with numbers and reference hardware.
- [ ] A benchmark suite and regression threshold policy exist.
- [ ] Budgets are linked from the roadmap and the blueprint.

## Notas del agente

