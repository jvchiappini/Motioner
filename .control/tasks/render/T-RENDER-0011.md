---
id: T-RENDER-0011
titulo: Masks and track mattes (alpha and luma, invert)
estado: done
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-02
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
- [x] Masks parse, animate and query
- [x] Render applies alpha and luma, or typed `Unsupported`
- [x] Two golden stills
- [x] Inspector and commands


## Notas del agente

Engine pass lives in the clone (D-0033): `Mask2d`, `MaskMode2d`,
`push_layer_2d_masked` / `begin_mask_2d` / `end_mask_2d`, the alpha/luma mask
shader, one scratch pass per masked layer and the typed misuse errors; eight
exact-value engine tests including the overlap pixel, the feathered source,
nested masks and a shadow clipped by the mask. Motioner: `MaskSpec`/`MaskMode`
on the node, `Document::set_mask`/`clear_mask`/`can_mask`, DSL `mask`,
`PlanMask` ranges with sources hidden from the normal stream, the `mask`
override, `mask_set`/`mask_clear`, the inspector Mask section (source picker,
mode, invert, clear), layers badges, query and bake, two goldens and the
`engine-masks` feature. With the feature off the headless renderer answers
`Unsupported { feature: Mask, node }` and the preview hides the source and
draws the content unmasked. Verified with the clone via a temporary
`Cargo.toml` path patch (restored).
