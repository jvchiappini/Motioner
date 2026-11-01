---
id: T-RENDER-0008
titulo: Gradients for fill and stroke (linear and radial, animatable stops)
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
- [ ] MDF gradients build, key, override, bake and query
- [ ] Render draws them or fails with typed `Unsupported`
- [ ] Golden stills plus a banding assertion

## Criterios de aceptacion
- [ ] 

## Notas del agente

