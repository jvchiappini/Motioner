---
id: T-PERF-0001
titulo: Define performance budgets and benchmark plan
estado: done
prioridad: alta
tipo: investigacion
creado_por: agente
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-11-21
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

- 2026-11-21: closed as **superseded** by D-0063 (performance policy)
  and T-PERF-0005 (prompt 18). The numeric budgets, the benchmark suite
  and the regression thresholds now live in `scripts/perf_budgets.json`,
  `scripts/bench_baselines.json`, `scripts/perf_bench.py` and the engine
  `performance.md`; the reference hardware is REF-0003.
- The acceptance boxes stay unchecked on purpose: this task is not
  "implemented", it is replaced by a decision. The same note records why
  Motioner keeps retained per-panel/per-row batch ranges instead of moving
  its chrome to `UiSystem`: D-0063 section 7 has the measured reason
  (heavy_ui_list 0.154 ms collection, scrub 1.735 ms, 23 draws) and what
  would change the choice.

