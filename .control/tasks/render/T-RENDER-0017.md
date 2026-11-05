---
id: T-RENDER-0017
titulo: Finishing effects and colour: grading, LUT, grain, vignette, chromatic aberration, adjustment layers
estado: backlog
prioridad: alta
tipo: feature
creado_por: usuario
asignado_a: agente
creado: 2026-11-04
actualizado: 2026-11-04
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

Today `node.effect` has three kinds (`drop_shadow`, `blur`, `glow`; D-0032).
The "film" look of professional motion design comes from finishing passes.
Asked by the user on 2026-11-04 ("super profesional").

## Criterios de aceptacion
- [ ] Colour: `levels`, `curves` (per channel), `hue_saturation`,
      `exposure`, `white_balance` and `lut` (a .cube asset), all keyable
- [ ] Texture: `grain` (deterministic per frame, size/amount), `noise`
- [ ] Lens: `vignette`, `chromatic_aberration`, `directional_blur`,
      `radial_blur`, `displacement` (map from another node)
- [ ] Adjustment layers: an effect stack that applies to everything below it
- [ ] Every effect is a generic FerrousEngine pass (documented, tested) and
      a D-0032 effect kind in MDF with `<kind>.<param>` channels
- [ ] Goldens per effect; render cost of each pass in the phase report

## Notas del agente

Pairs with T-NLE-0002 (transitions) for a "pro finish" prompt.
