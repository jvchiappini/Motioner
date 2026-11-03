---
id: T-RENDER-0012
titulo: Typography: real fonts, tracking, line height, text metrics, per-character animation
estado: done
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-02
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
- [x] Font loading with a clear missing-font error
- [x] Tracking, line height and metrics
- [x] Per-character animation keys, overrides and bake
- [x] Golden stills

## Notas del agente

Phase 7 (2026-11-02): Inter 400/600/700 shipped with OFL; `FontSet` lookup
(project first, nearest-weight warning, named missing-family error);
`TextMetrics` trait + `CpuTextMetrics`; `tracking`/`line_height` channels and
`weight`; `text_width`/`text_bounds`; `animate_chars` in the document, DSL and
renderer (behind `engine-text-layout`); inspector Family/Weight/Tracking/Line
height and `\n` escaping; `examples/typography.motion` and three goldens.

Done in phase 8: `text_anim_add`/`text_anim_remove` commands, the canonical
`text_animators` override (`TextAnimator::encode_list`), `node.clear_chars()`,
bake emitting `clear_chars()` + `animate_chars(...)`, the inspector Animators
section, `query` reporting the list, and a bake round-trip test.
`CpuTextMetrics` now caches `ferrous_font::LayoutData` per face and calls
`measure_block` (pin e0cc574); the engine adds `LayoutData` and a bit-for-bit FontAtlas comparison. See D-0035.

