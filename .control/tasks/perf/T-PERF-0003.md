---
id: T-PERF-0003
titulo: Phase 11 review fixes: radial gradient alpha, Showcase Pro art pass and engine batch reuse
estado: done
prioridad: alta
tipo: bug
creado_por: agente
asignado_a: agente
creado: 2026-11-03
actualizado: 2026-11-03
depende_de: []
bloqueado_por: null
version_schema: 1
---
The Phase 11 review found three classes of problem: a radial-gradient render
bug visible in the orbs, an art pass for the demo reel, and three engine nits
from the Phase 10 work.

## Contexto

- **A1, radial gradient alpha.** `gradients::geometry` transformed the centre
  and then transformed `centre + radius` again before measuring the radius, so
  an off-centre node folded its translation into the radius. The ramp did not
  reach its transparent stop at the silhouette; an additive orb read as a
  hard-edged, almost opaque disc.
- **A2, Showcase Pro.** The orbs cluttered every scene, the scene 1 monogram
  had a 40 px gap at the apex, captions were under 28 px and low contrast, and
  REVEAL was not optically centred in its card.
- **A3, engine nits.** `gui.wgsl` documented flag `0x100` while the code uses
  `0x200`; translucent polylines double the joint coverage; the runner built a
  fresh `GuiBatch` every frame, which showed as 9 allocations/frame in the
  editor counter.

## Criterios de aceptacion
- [x] The radial radius is `radius x scale`; a regression test renders an
      off-centre translucent radial gradient in all seven blend modes and
      asserts the backdrop is unchanged at the edge and `alpha x colour` at the
      centre
- [x] Golden SHAs that change are re-pinned and `hello_f120.png` stays
      byte-identical (`C2173BAD...`)
- [x] Showcase Pro: REVEAL optically centred from `text_bounds`, no text under
      28 px, brighter captions, soft and varied orbs, soft light leaks, clean
      monogram build
- [x] Engine: shader comment fixed, polyline limitation documented and pinned
      by a test, runner reuses the `GuiBatch`; editor counter reads 0/frame
- [x] Gate green and the engine clone commit is reported for the pin bump

## Notas del agente

- The engine clone commit is `e482b3c` on `feat/pure2d-text-and-readback`; the
  Motioner submodule working tree is checked out there and the reviewer must
  commit the pointer. It carries the `GuiBatch` recycle path
  (`FrameBuilder::take_ui_batch`, `FramePacket::remove`), the `0x200` comment
  fix and the `GuiBatch::polyline` limitation + test.
- Radial fix: `crates/motioner_render/src/gradients.rs`; tests in
  `crates/motioner_render/src/gradients.rs` (unit) and
  `crates/motioner_render/tests/render.rs` (GPU, all modes).
- Re-pinned goldens: `gradients_f30/f105` and all six
  `showcase_pro_f*.png`; `hello_f120` is unchanged.
- Alloc reading: `out/ui/p12-01-alloc.png` (debug, alloc-count) and
  `out/ui/p12-02-fps-1600.png` (release, 59 fps, Alloc 0).
