---
id: T-RENDER-0015
titulo: Video: frame-exact decode, proxies and streamed textures (prompt 11 phase 6)
estado: done
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-04
actualizado: 2026-11-04
depende_de: []
bloqueado_por: null
version_schema: 1
---
## Contexto

Prompt 11 phase 6: `video(source, in_point, out_point)` extended with
`start`, `speed`, `width`, `height`, `fit` and `gain`. Video behaves like
an image for geometry/effects/masks/blend, selects frames by exact
timestamp math, plays through proxies built in `cache/`, streams frame
textures in place and mixes its audio through the Phase 5 mixer.

## Criterios de aceptacion
- [x] MDF `video(...)` parses every reserved parameter and draws an image
      item.
- [x] Frame selection is exact for 24/25/30/60 fps sources into a 60 fps
      composition, with ffmpeg testsrc integration tests.
- [x] Preview uses bounded proxies generated in a background job with a
      progress indicator; render/still decode the original.
- [x] Texture updates reuse one texture per video layer
      (`Renderer::update_sprite_texture`) and the frame cache is bounded.
- [x] Timeline shows a thumbnail strip, trim/move and Alt-drag slip.
- [x] Video audio is mixed and muxed by `render`.
- [x] Gate green and screenshots `p11-07-*.png` taken.

## Notas del agente

Core: `VideoContent`/`VideoAsset`, `VideoResolver`,
`Document::resolve_videos`, `ImageKey::tagged`. IO: `video.rs`
(probe/frame decode/proxy) and `ProjectImages` video resolution.
Eval: frame-exact clip check and image item. Render: image revisions and
in-place texture updates with a resize fallback. UI: video inspector rows,
timeline thumbnails/trim/slip, proxy/memory readouts. App: bounded
`VideoCache`, proxy job and full-res still/render decode.
Measurements (debug, 1600x900, 1080p60 proxy playing): ~198 MB working
set, ~769 MB private, 59 UI fps, 120 dropped frames over ~5 s,
video cache ~42 MB. Output `out/p11-video/render.mp4` has 3.000 s video
and 3.000 s AAC audio.

