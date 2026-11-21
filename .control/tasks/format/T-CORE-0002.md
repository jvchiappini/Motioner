---
id: T-CORE-0002
titulo: Professional easing, motion blur and animation helpers
estado: done
prioridad: media
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-03
depende_de: [T-CORE-0001]
bloqueado_por: null
version_schema: 1
---
Timing is half of motion design. Phase 9 adds `ease=bezier(x1,y1,x2,y2)`
(CSS semantics, solved numerically and deterministically), `hold`,
`elastic_out`, `bounce_out`, `expo_in_out`, `sine_in_out`, motion blur and the
`stagger`/`wiggle` helpers.

## Design

- `Ease` grows into a value that can carry a cubic-bezier control pair; the
  overrides file, bake, `query`, the ease grid and the timeline lines support
  it. `hold` is a step.
- Motion blur: `comp(..., motion_blur=True, shutter=180, samples=8)` samples
  sub-frame times deterministically and averages in linear light; off in the
  editor preview by default (toggle command), on in `render`.
- Helpers produce ordinary keys, deterministic and bakeable.
- Tests: bezier solver accuracy, motion-blur golden still, helper outputs.

## Criterios de aceptacion
- [x] New eases parse, round-trip through overrides and bake, and show in the
      ease grid and timeline
- [x] Motion blur deterministic and switchable
- [x] Helpers emit keys
- [x] Tests and one motion-blur golden

## Notas del agente

D-0037 (the prompt called it D-0036, already taken by write-through) and
D-0038 (engine accumulation). `Ease` keeps `Copy + Eq + Hash` through a
quantised `BezierEase`. `Evaluator::evaluate_fractional` leaves the integer
path byte-identical. `render_frame` uses the engine accumulation API behind
the `engine-accumulation` feature (off until the pin bump). Goldens:
`timing_eases_f030.png`, `timing_motion_blur_f400.png`. The motion-blur
golden and the feature-on clippy were generated against the FerrousEngine
clone. The editor shows the new presets and a bezier key's numbers; the
four numbers are read-only in the inspector today (`set_ease(bezier(...))`
edits them).

