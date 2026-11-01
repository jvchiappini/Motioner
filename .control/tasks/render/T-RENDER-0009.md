---
id: T-RENDER-0009
titulo: Blend modes (normal, add, multiply, screen, overlay, darken, lighten)
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


MDF accepts only `blend="normal"`. Phase 4 implements the seven modes with the
premultiplied-alpha formulas fixed in D-0028.

## Design

- Core validates the full list; the render plan carries it.
- Engine Part B as needed (pipeline variants or one compositing pass); until
  then non-normal modes return `RenderError::Unsupported`.
- Editor: blend mode in the inspector (segmented/dropdown), overridable.
- Tests: one golden swatch per mode over a known background.

## Criterios de aceptacion
- [ ] Every mode parses in MDF and is reported by `query`
- [ ] Render matches, or typed `Unsupported` pending the pin
- [ ] Seven golden swatches
- [ ] Inspector control and command coverage

## Criterios de aceptacion
- [ ] 

## Notas del agente

