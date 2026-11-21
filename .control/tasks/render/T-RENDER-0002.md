---
id: T-RENDER-0002
titulo: Render the first MDF-driven MP4 through FerrousEngine
estado: done
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
- [x] `cargo run -p motioner_app -- render examples/hello.motion --out out/hello.mp4` works.
- [x] The MP4 plays and contains 300 frames at 1920x1080 and 60 fps.
- [x] Text and shapes animate as described by the MDF program.
- [x] The same frame rendered twice produces the same hash or a
      documented tolerance.
- [x] Engine types do not leak outside `motioner_render`.
- [x] No per-frame heap allocation in the render loop beyond documented
      buffers.
- [x] A golden PNG and a short report with timings are committed.

## Notas del agente


2026-10-31 (S-2026-10-31-008): not started in code. Engine API investigated (headless Pure2D path exists, no 2D text: plan is CPU glyph coverage from ferrous_font into sprites). Pending: motioner_render adapter, ffmpeg export in motioner_io, CLI in motioner_app, CI and boundary-script updates. Implementation moves to opencode; Claude audits.

2026-10-31 (S-2026-10-31-010): done via opencode prompt 04 rev 2 on engine
`32aea8a`, reviewed and committed in `3fea9e3`. 300 frames 1920x1080 60/1
yuv420p in 2.75 s on an RTX 4050 (render+readback 5.5 ms/frame, encode
3.0 ms/frame). Golden `crates/motioner_testkit/golden/hello_f120.png`;
report in `crates/motioner_render/README.md`. Determinism: identical
SHA-256 for the same frame. Rendering 131 workspace tests green.
