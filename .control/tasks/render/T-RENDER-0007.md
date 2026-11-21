---
id: T-RENDER-0007
titulo: Vector paths and trim paths (path, polygon, star, arc, trim, dash)
estado: done
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
`showcase.motion` draws line art as many rects because MDF had no path. This
phase adds `path(...)` with straight and cubic Bezier segments, the
`polygon`, `star` and `arc` helpers, and animatable `trim_start`,
`trim_end`, `trim_offset`, `dash` and `dash_offset` for draw-on effects.

## Design

- The MDF shape is fixed by D-0029: a list of points with optional relative
  handles, not builder methods, so a path stays one immutable value.
- Geometry lives in `motioner_core::path` (flattening, trim with wrap, dash,
  bounds, containment); render tessellates the flattened pieces with
  `lyon_tessellation` into triangles and caches them per node.
- The pinned engine already has the triangle SDF, so this phase renders for
  real: no `RenderError::Unsupported`, no engine change and no pin bump.
- Editor: path layers in the outline/inspector/timeline, selection bounds and
  picking against the fill or the stroke; the gizmo uses the node transform.
  Canvas point editing stays in T-UI-0006.

## Criterios de aceptacion
- [x] `path`, `polygon`, `star` and `arc` build documents that `query` reports
- [x] Trim and dash are animatable through `key`/`edit`/overrides/bake
- [x] Render either draws tessellated paths or returns typed `Unsupported`
- [x] Golden stills for star, arc and half-drawn path
- [x] Editor rows, bounds and gizmo for path nodes

## Notas del agente

Implemented in session S-2026-11-01-006 (prompt 10, phase 2).

- ADR D-0029: authoring model, geometry ownership, lyon dependency (MIT OR
  Apache-2.0, already in the engine graph) and the no-pin-bump decision.
- `examples/paths.motion` animates a star's rotation, an arc's trim and
  width, a cubic ribbon's trim window and a dashed hexagon's dash offset.
- Goldens: `paths_f45.png` (SHA `d42669a9…`) and `paths_f80.png`
  (SHA `c646efcd…`), tolerance 2/0.1 %.
- Pending and out of scope: multi-subpath paths, fill-rule choice, dash
  patterns beyond the 50 % duty cycle and canvas point editing (T-UI-0006).
