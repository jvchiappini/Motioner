---
id: PHASE-0004
nombre: "3D, effects and compositing"
orden: 4
estado: not_started
inicio: 2026-10-31
fin_estimado: ""
fin_real: null
depende_de: ["PHASE-0003"]
responsable: agente
prioridad: alta
---

# Phase: 3D, effects and compositing

## Summary

Extend Motioner from a 2D editor into a 2.5D/3D motion-graphics tool by
plugging FerrousEngine's scene, PBR renderer, cameras and lights into the
composition pipeline, and by adding a real compositing graph with layer
buffers, masks, mattes and effects.

The phase ends when a 3D layer with a camera and a light composites
correctly over 2D layers, with at least one GPU effect and color-managed
output.

## Goals

- Treat 3D layers as first-class composition citizens with their own
  cameras, lights and materials.
- Move from ad-hoc draws to an explicit render graph of passes and
  intermediate targets.
- Support the compositing features motion designers expect: masks,
  track mattes, blend modes and adjustment layers.

## Scope

### In scope
- 3D layer types, scene graph mapping, cameras, lights, PBR materials.
- Depth, alpha and HDR intermediate buffers.
- Render graph with 2D/3D passes, downsample/blur chains and post.
- Masks, track mattes, blend modes, adjustment layers.
- Color management: linear workspace, tonemapping, display transforms.

### Out of scope
- Volumetrics, global illumination and physics simulation.
- Plugin effects and third-party shaders.
- AI-assisted 3D scene generation.

## Dependencies

- PHASE-0003: viewport, timeline and compositor foundation.
- FerrousEngine renderer and 2D/3D crates pinned and adapted.

## Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Engine render graph not flexible enough | High | High | Keep an adapter layer; upstream general passes instead of forking deeply |
| HDR/alpha compositing bugs | High | High | Reference test scenes and golden frames per blend mode |
| 3D plus 2D ordering conflicts | Medium | High | One explicit layer ordering model for both domains |
| GPU memory pressure from intermediates | Medium | High | Pooled textures, resolution-aware allocation, budget tests |

## Key decisions

- D-0001: FerrousEngine as rendering foundation.
- D-0006: compiled render plan consumed by the renderer.
- D-0008: unified layer ordering across 2D and 3D.

## Success criteria

- [ ] A 3D object with camera and light renders over a 2D background
      with correct depth and alpha.
- [ ] At least five compositing effects run through the render graph
      with quality presets.
- [ ] Masks and track mattes work in preview and export identically.
- [ ] The compositor stays within the documented VRAM budget.

## Initiatives

- INITIATIVE-0010 - 3D scene integration
- INITIATIVE-0011 - Compositing effects and color
- INITIATIVE-0012 - Masks, mattes and precomps

## Notes

3D is an extension of the composition model, not a separate application
mode. The same timeline and evaluator must drive 2D and 3D layers.
