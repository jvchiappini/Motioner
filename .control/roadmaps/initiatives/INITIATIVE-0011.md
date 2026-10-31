---
id: INITIATIVE-0011
nombre: "Compositing effects and color"
phase: PHASE-0004
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0010"]
creado: 2026-10-31
responsable: agente
---

# Initiative: Compositing effects and color

## Description

Replace ad-hoc draws with an explicit render graph of passes and
intermediate targets, then use it to ship a compositing effect stack and
color-managed output. This is what makes Motioner output look
professional rather than like a demo.

## Scope

### Deliverables
- Render graph with named passes, dependencies and pooled targets.
- Effect stack model per layer and per composition.
- Effects: gaussian blur, glow/bloom, color correction, curves, grain,
  chromatic aberration, vignette, sharpen.
- Blend modes for layer compositing.
- Adjustment layers.
- Linear workspace, exposure, tonemapping, display transform and LUTs.
- Quality presets for preview and final.

### Non-goals
- Third-party shader plugins.
- Frame interpolation and optical flow.
- Advanced simulation effects.

## Technical approach

Effects declare their inputs and outputs and are scheduled by the render
graph. Intermediates are RGBA16F, allocated from a pool and reused by
resolution and format. Effect parameters are animatable and evaluated
like any other property. Color transforms are explicit and tested with
reference charts.

Key considerations:
- Premultiplied alpha end to end.
- Deterministic sampling for blur and grain.
- Budget VRAM and degrade gracefully in preview.

## Dependencies

- INITIATIVE-0010: 3D integration and depth/alpha buffers.
- FerrousEngine render graph crate.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0006 | 3D layer composited over 2D with effects | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] At least eight effects work in preview and export.
- [ ] Blend modes and adjustment layers behave correctly.
- [ ] Output is color-managed and consistent with references.
- [ ] VRAM stays within budget at 4K.

## Notes

Quality presets should change sample counts and resolution, never the
final look at full quality.
