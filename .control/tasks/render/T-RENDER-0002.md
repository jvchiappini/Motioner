---
id: T-RENDER-0002
titulo: Render the first MDF-driven MP4 through FerrousEngine
estado: backlog
prioridad: critica
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-10-31
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

First visual milestone: execute an MDF program, render a 5-second
1920x1080 60 fps 2D composition headlessly through FerrousEngine, and
encode an H.264 MP4 with ffmpeg. Scope is deliberately small: shapes,
text, transforms, opacity and keyframes. No editor, NLE, audio, 3D, AI,
LaTeX or overrides.

Pin FerrousEngine to a recorded commit and keep it behind the
`motioner_render` adapter. Use an offscreen target and pipe raw RGBA
frames to ffmpeg. The result must be deterministic and playable.

## Criterios de aceptacion
- [ ] `cargo run -p motioner_app -- render examples/hello.motion --out out/hello.mp4` works.
- [ ] The MP4 plays and contains 300 frames at 1920x1080 and 60 fps.
- [ ] Text and shapes animate as described by the MDF program.
- [ ] The same frame rendered twice produces the same hash or a
      documented tolerance.
- [ ] Engine types do not leak outside `motioner_render`.
- [ ] No per-frame heap allocation in the render loop beyond documented
      buffers.
- [ ] A golden PNG and a short report with timings are committed.

## Notas del agente

