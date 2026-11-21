---
id: T-RENDER-0004
titulo: FerrousEngine polish pass: overlapping readback, hard-edge band, font id generations
estado: backlog
prioridad: alta
tipo: bug
creado_por: agente
asignado_a: agente
creado: 2026-10-31
actualizado: 2026-10-31
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

Review of FerrousEngine `32aea8a` (T-RENDER-0003) found defects that do not
block the first Motioner render. Fix them on the same engine branch before
merging it to engine main, then bump the Motioner pin again.

1. The two-slot readback ring does not overlap work:
   `Pure2dHarness::render_frame` and `take_readback` both block on
   `Maintain::Wait`, and `map_async` starts only in `take_readback`
   (`engine/ferrous/crates/ferrous_renderer/src/headless.rs:730`, `:857`).
   A failed map leaves its slot pending, so every later take repeats the
   error.
2. `edge_inside` in `engine/ferrous/assets/shaders/shape_2d.wgsl:316` treats
   a 1e-3 band around a hard edge as "on the line" and decides it by edge
   direction. For all-hard triangles the rasteriser already partitions
   pixels, so a pixel inside the band on the losing side is rejected by the
   only triangle that rasterised it: a one-pixel hole.
3. `Renderer::register_font_2d` recycles freed `FontId2d` values without a
   generation (`engine/ferrous/crates/ferrous_renderer/src/renderer_core.rs:3140`),
   so a stale id draws with another font.
4. With `default-features = false` (how Motioner builds it, D-0019),
   `ferrous_renderer` emits 8 warnings (unused imports, variables, a dead
   field and method); the engine CI only lints the default features.
5. `measure_text_2d` allocates a `Vec` per call.

## Criterios de aceptacion
- [ ] Readback mapping starts at submit; a frame's copy overlaps the next frame's CPU work; a failed map frees its slot; a test proves ordering and recovery.
- [ ] Hard-edge coverage has no holes and no double blend on randomized translucent triangulations (seeded test).
- [ ] Font ids carry a generation; a stale id is rejected with a warning and a test.
- [ ] Zero warnings with and without default features; CI lints both.
- [ ] Measuring text does not allocate.
- [ ] Engine gates green; branch pushed; Motioner pin bumped with an ADR.

## Notas del agente
