---
id: T-RENDER-0013
titulo: Images (project-relative, content-hash cache, fit modes)
estado: backlog
prioridad: media
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-01
depende_de: [T-CORE-0001]
bloqueado_por: null
version_schema: 1
---


`image(source, width=None, height=None, fit="contain"|"cover"|"fill")` from
project-relative paths, cached on the GPU by content hash. PNG now; JPEG/WebP
only with an ADR.

## Design

- Bytes enter through `motioner_io`; core/eval stay file-system-free. Missing
  or undecodable assets are typed `RenderError::Asset` errors (D-0028).
- Opacity, transform, masks, effects and blend modes apply to images.
- Editor: image layers with a thumbnail icon and size fields.
- Tests: decode, fit modes, golden still.

## Criterios de aceptacion
- [ ] `image()` builds and queries
- [ ] Decode, fit modes and GPU cache
- [ ] Golden still and decode tests
- [ ] Editor rows

## Criterios de aceptacion
- [ ] 

## Notas del agente

