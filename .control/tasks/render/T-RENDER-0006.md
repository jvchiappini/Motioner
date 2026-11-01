---
id: T-RENDER-0006
titulo: FerrousEngine: honour a screenshot request made in update
estado: done
prioridad: media
tipo: bug
creado_por: usuario
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-01
depende_de: []
bloqueado_por: null
version_schema: 1
---
## Contexto

`AppContext::request_screenshot()` was only honoured from `draw_ui`: the draw
pass builds a fresh context, so a flag set in `update` was dropped. Prompt 09
phase 4 (Part B) fixes the FerrousEngine dev clone.

## Criterios de aceptacion
- [x] The update-time request is carried into the capture decision.
- [x] A pure helper (`screenshot_wanted`) has regression tests: update request, draw request, no request, no `COPY_SRC`.
- [x] The `request_screenshot` rustdoc states the update/draw contract.
- [x] The engine's tests and clippy on the touched crate run; CRLF endings kept.
- [x] Motioner code unchanged (it keeps requesting in `draw_ui`).

## Notas del agente

Working tree of `C:\Users\jvchi\CARPETAS\FerrousEngine` on top of
`c4ae1f2` (`feat/pure2d-text-and-readback`); not committed, per prompt 09.
The reviewer pushes and bumps the Motioner submodule pin.
