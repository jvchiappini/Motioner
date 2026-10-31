---
id: INITIATIVE-0009
nombre: "2D compositor, typography and MP4 export"
phase: PHASE-0003
estado: backlog
prioridad: alta
tamano_estimado: large
depende_de: ["INITIATIVE-0008"]
creado: 2026-10-31
responsable: agente
---

# Initiative: 2D compositor, typography and MP4 export

## Description

Deliver the 2D motion-graphics feature set and the first real video
output. Layers include solids, shapes, paths, text, images and video
placeholders; the compositor handles alpha, blend modes and basic
effects; the exporter writes an MP4 through ffmpeg from the same frames
shown in the viewport.

## Scope

### Deliverables
- 2D layer types with transforms, opacity and blend modes.
- Shape and path rendering with fills, strokes and joins.
- Text layout, font loading, MSDF rendering and basic typography
  controls.
- Image and image-sequence layers with color and alpha handling.
- Transform, opacity, blur, glow and color-correction effects.
- Premultiplied-alpha compositing in a linear color space.
- Export to MP4, PNG sequence and a still frame.
- Export settings: resolution, fps, range, quality and color tags.

### Non-goals
- 3D layers and PBR.
- Masking and track mattes.
- Audio mixing and loudness.

## Technical approach

Reuse the engine's 2D and font crates where they fit, wrapped by the
render adapter. Composition is done in RGBA16F intermediates, then
tonemapped and encoded. Export runs the same evaluator frame by frame
and pipes raw frames to ffmpeg to avoid temporary image files.

Key considerations:
- Text quality needs a stable layout and glyph cache.
- Define alpha semantics once: premultiplied everywhere.
- Export must not depend on the current viewport zoom or proxy.

## Dependencies

- INITIATIVE-0008: viewport and playback.
- INITIATIVE-0006: golden-frame harness.

## Milestones

| Milestone | Description | Target date | Status |
|---|---|---|---|
| M-0005 | MP4 export matches preview | | backlog |

## Tasks

No tasks yet. Create them when this initiative starts.

## Acceptance criteria

- [ ] A 10-second 1080p 2D composition exports to MP4.
- [ ] Exported frames match the preview within tolerance.
- [ ] Text, shapes, images and blend modes render correctly.
- [ ] Alpha export works for supported formats.

## Notes

The MP4 milestone is the first end-to-end proof of the product. Protect
it from scope creep; 3D waits until this ships.
