---
id: T-RENDER-0007
titulo: Vector paths and trim paths (path, polygon, star, arc, trim, dash)
estado: backlog
prioridad: alta
tipo: feature
creado_por: agente
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-01
depende_de: [T-CORE-0001]
bloqueado_por: null
version_schema: 1
---


`showcase.motion` draws line art as many rects because MDF has no path. Phase 2
of the professional-motion job adds `path(...)` with straight and cubic Bezier
segments, the `polygon`, `star` and `arc` helpers, and animatable
`trim_start`, `trim_end`, `trim_offset`, `dash` and `dash_offset` for draw-on
effects.

## Design

- The MDF shape is fixed by D-0028 addenda: a list of points with optional
  handles, plus builder methods; choose one and record it in the task notes.
- Render: tessellate fills and strokes (lyon via ADR, or own flattening +
  stroker + polygon fill), anti-aliased, cached per node and invalidated on
  change. The engine pin must be bumped before the render hook stops returning
  `RenderError::Unsupported`.
- Editor: path nodes in layers/inspector/timeline, bounds and picking; the
  gizmo works on the path transform. Canvas point editing is out of scope and
  tracked in T-UI-0006.
- Tests: flattening tolerance, trim lengths, golden stills of a star, an arc
  and a half-drawn path.

## Criterios de aceptacion
- [ ] `path`, `polygon`, `star` and `arc` build documents that `query` reports
- [ ] Trim and dash are animatable through `key`/`edit`/overrides/bake
- [ ] Render either draws tessellated paths or returns typed `Unsupported`
- [ ] Golden stills for star, arc and half-drawn path
- [ ] Editor rows, bounds and gizmo for path nodes

## Criterios de aceptacion
- [ ] 

## Notas del agente

