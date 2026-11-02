---
id: T-RENDER-0008
titulo: Gradients for fill and stroke (linear and radial, animatable stops)
estado: done
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
Professional cards and backgrounds need gradients. Phase 3 adds
`linear_gradient(start, end, stops)` and `radial_gradient(center, radius,
stops)` accepted by `fill` and `stroke` of rects, circles, paths and text, with
animatable stop colours and positions on top of the D-0028 channel model.

## Design

- Introduce a `Gradient` value type in core (stops as animatable colours and
  offsets); `PaintSpec`/`Paint` gain a third variant. Non-animated gradients
  render first; stop animation reuses the content channel machinery.
- Engine Part A only if the Pure2D shaders cannot do it: gradient fills in the
  Pure2D pass, dithered to avoid banding, exact sRGB stops; otherwise return
  `RenderError::Unsupported` until the pin moves.
- Tests: golden still with a linear and a radial gradient; banding check (max
  step between neighbouring pixels).

## Criterios de aceptacion
- [x] MDF gradients build, key, override, bake and query
- [x] Render draws them or fails with typed `Unsupported`
- [x] Golden stills plus a banding assertion

## Notas del agente

D-0030. `Gradient`/`GradientStop` in core with animatable offsets and colours;
`PaintSpec.fill_gradient`/`stroke_gradient`; dynamic `ContentField` stop
channels; DSL `linear_gradient`/`radial_gradient`; eval gradient arena;
`GradientTable` + `engine-gradients` feature (uncommitted engine Part A in the
FerrousEngine clone); `RenderError::Unsupported` on the pinned engine; stop
rows/stopwatches in inspector and timeline; `examples/gradients.motion`;
goldens `gradients_f30.png` (SHA 466dceb4...) and `gradients_f105.png` (SHA
d0c73e8...); text gradients remain unsupported until the text pass lands.

