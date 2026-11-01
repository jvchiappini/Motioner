---
id: T-RENDER-0012
titulo: Typography: real fonts, tracking, line height, text metrics, per-character animation
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


`showcase.motion` hard-codes Roboto advances and draws one line per size.
Phase 7 loads TTF/OTF families from `assets/fonts/` through `motioner_io`,
adds animatable `tracking` and `line_height`, the `text_width`/`text_bounds`
built-ins and one per-character animation model.

## Design

- Ship an open-licence family in `assets/fonts/` (licence file and ADR) next
  to Roboto; missing fonts are a load-time diagnostic.
- Choose `animate_chars(...)` or `split_chars(...)` in the ADR; deterministic,
  overridable and bakeable.
- Tests: shaping/advance tests, golden still of tracked, multi-line and
  per-character-animated text.

## Criterios de aceptacion
- [ ] Font loading with a clear missing-font error
- [ ] Tracking, line height and metrics
- [ ] Per-character animation keys, overrides and bake
- [ ] Golden stills

## Criterios de aceptacion
- [ ] 

## Notas del agente

