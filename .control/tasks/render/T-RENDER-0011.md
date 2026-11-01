---
id: T-RENDER-0011
titulo: Masks and track mattes (alpha and luma, invert)
estado: backlog
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-01
depende_de: [T-RENDER-0007]
bloqueado_por: null
version_schema: 1
---


Masked text reveals need `node.mask(other, mode="alpha"|"luma", invert=False)`,
with the mask node not drawn itself and its transform animatable.

## Design

- Core: optional `MaskRef` per node.
- Engine Part D: stencil or offscreen alpha multiply.
- Editor: the relationship is visible in layers (icon/indent) and the
  inspector; commands set/clear a mask.
- Tests: golden stills of an alpha reveal and an inverted luma mask.

## Criterios de aceptacion
- [ ] Masks parse, animate and query
- [ ] Render applies alpha and luma, or typed `Unsupported`
- [ ] Two golden stills
- [ ] Inspector and commands

## Criterios de aceptacion
- [ ] 

## Notas del agente

