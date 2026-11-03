---
id: T-RENDER-0013
titulo: Images (project-relative, content-hash cache, fit modes)
estado: done
prioridad: media
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-02
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
- [x] `image()` builds and queries
- [x] Decode, fit modes and GPU cache
- [x] Golden still and decode tests
- [x] Editor rows

## Notas del agente

Phase 8 (2026-11-02): `ImageContent`/`ImageFit`/`ImageLayout` in core; DSL
`image()`; `ImageStore` in `motioner_io` (PNG, palette/grey/16-bit -> RGBA8,
SHA-256 keys, project-relative validation); `Document::resolve_images` with the
`ImageResolver` trait; plan image items with key/dest/`uv_rect`; renderer
`set_images`, mipmapped textures by key and a byte budget; placeholder in the
preview and `UnresolvedImage` in `still`/`render`; inspector Image rows, image
icon and timeline channels; `query` image block; `examples/images.motion` with
`scripts/gen_image_assets.py` and two goldens. Engine: `LayoutData` and
`register_texture_2d` at pin e0cc574; the reviewer removed the gap features
with the bump. See D-0035.

## Notas del agente

