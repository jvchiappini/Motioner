---
id: T-RENDER-0014
titulo: 3D layers: glTF, cameras, lights, per-object shading (unlit, PBR, path traced) composited with 2D
estado: backlog
prioridad: alta
tipo: feature
creado_por: usuario
asignado_a: agente
creado: 2026-11-03
actualizado: 2026-11-04
depende_de: []
bloqueado_por: null
version_schema: 1
---

## Contexto

The 1.0 V3 criterion: 3D layers with glTF models, cameras, lights, PBR,
skeletal animation and GPU particles composited with 2D layers. FerrousEngine
already ships the 3D passes (PBR, prepass, shadows, skinning); Motioner needs
the document/plan surface, the loader and the ordering with the 2D path.

## Criterios de aceptacion
- [ ] `camera`, `light` and `mesh(source)` document nodes with MDF syntax
- [ ] glTF import through `motioner_io`, content-hashed and budgeted
- [ ] PBR, lights and shadows through the engine 3D passes
- [ ] Skeletal animation sampled by the deterministic evaluator
- [ ] 2D and 3D layers interleave in one painter order and one export
- [ ] Per-object shading model, chosen per mesh: `unlit` (albedo only),
      `pbr` (raster, engine passes) and `path_traced` (progressive, sample
      count per object/layer, deterministic seed); objects with different
      models coexist in one 3D layer and composite correctly with depth
- [ ] Materials as MDF values (`material(albedo, metallic, roughness,
      emissive, opacity, ...)`), every parameter keyable
- [ ] Lights as nodes (`directional`, `point`, `spot`, `area`, environment
      HDRI), keyable, with light linking: a light lists the objects it
      affects (default all), and objects can opt out of casting/receiving
      shadows
- [ ] Cameras as nodes: several per composition, active camera switchable
      on the timeline (camera cuts), fov/focal length, depth of field
      (focus distance, aperture), keyable; 2.5D: 2D layers can live in the
      3D space of a camera (parallax)
- [ ] Rendering features are generic FerrousEngine features (no Motioner
      names), documented and tested in the engine; the path tracer must be
      deterministic for `render` and may be progressive in the preview

## Notas del agente

Backlogged at the end of prompt 10 (Phase 12). Design the plan boundary before
touching the engine.

2026-11-04: the user asked for per-object render settings (one object
albedo only, another ray/path traced), lights that affect chosen objects
and cameras. Needs an ADR before implementation (shading model contract,
path tracer determinism, preview vs render quality).
