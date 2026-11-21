---
id: T-RENDER-0009
titulo: Blend modes (normal, add, multiply, screen, overlay, darken, lighten)
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
MDF accepts only `blend="normal"`. Phase 4 implements the seven modes with the
premultiplied-alpha formulas fixed in D-0028.

## Design

- Core validates the full list; the render plan carries it.
- Engine Part B as needed (pipeline variants or one compositing pass); until
  then non-normal modes return `RenderError::Unsupported`.
- Editor: blend mode in the inspector (segmented/dropdown), overridable.
- Tests: one golden swatch per mode over a known background.

- [x] Every mode parses in MDF and is reported by `query`
- [x] Render matches, or typed `Unsupported` pending the pin
- [x] Seven golden swatches
- [x] Inspector control and command coverage


## Notas del agente

D-0031. Core `BlendMode` with seven modes; DSL/overrides/query/`set`/`edit` carry it as a discrete node property. The renderer wraps a non-normal node in an engine layer: add/multiply/screen use the fixed-function composite today, overlay/darken/lighten need the engine backdrop pass and return `RenderError::Unsupported { feature: BlendMode, node }` until the pin bump (temporary `engine-composite` feature). Engine work in the clone: `CompositeMode2d`, `Layer2d`, `BackdropCompositePipeline`, destination snapshots, text gradients and `GradientTable2d`. `examples/blend.motion`, golden `blend_f30.png` (SHA 052dd593...), captures `out/ui/p4-blend-inspector.png` and `out/ui/p4-gradient-text.png`.
