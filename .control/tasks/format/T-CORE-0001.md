---
id: T-CORE-0001
titulo: Animatable content properties (width, height, radius, fill, stroke, size, color)
estado: done
prioridad: alta
tipo: feature
creado_por: usuario
asignado_a: agente
creado: 2026-11-01
actualizado: 2026-11-01
depende_de: []
bloqueado_por: null
version_schema: 1
---
## Contexto

MDF v0 animates only the five transform channels. `fill`, `stroke`,
`stroke_width`, `width`, `height`, `radius` and text `size`/`color` are static,
the biggest visible limit of `examples/showcase.motion`. D-0028 fixes the
shared professional-motion model; this task implements its first phase.

## Design

- `motioner_core`: the content fields become `Animatable<f64>`,
  `Animatable<Color>` and `Animatable<Option<Color>>` (`PaintSpec` for fills and
  strokes). `NodeContent::channel(field)` and `static_value(field)` are the one
  field-to-channel table; `Document::add_content_keyframe` and
  `remove_content_keyframe` are the validated write path.
- `motioner_eval`: sample every content channel into the render plan; colours
  interpolate in linear-light sRGB (`Color::lerp`), an optional colour holds
  the previous value when one endpoint is `none`. The sRGB transfer function
  moved to `motioner_core::Color` so the renderer and the interpolator share it.
- `motioner_dsl`: `node.key("fill", t, "#RRGGBB", ease=...)` and every other
  animatable field; `text` and `align` reject keys. Bake edits content-key
  literals in place and can emit `var.key("fill", ...)` in the generated block.
- Editor: one `Channel` type in `editor::channels` wraps transform channels and
  content fields, so `key`, `unkey`, `move_key`, `ease`, `stopwatch`,
  `toggle_key`, `add_keys`, `edit` and `query` work unchanged. Content rows in
  the timeline get stopwatches, diamonds and eased lines; the inspector keys
  them at the playhead; the layers panel counts their keys.

## Criterios de aceptacion
- [x] Core interpolation test for colour midpoints in linear light
- [x] DSL, eval, overrides round-trip and bake tests for content keys
- [x] Editor command and query tests for content channels
- [x] Timeline content rows carry keys, eases and summary diamonds
- [x] Golden still `animatable_f45.png` (960x540, tolerance of D-0028)
- [x] Gate green: fmt, clippy pedantic, tests, rustdoc, boundaries, machete,
      pctl validate

## Notas del agente

`examples/animatable.motion` is the feature demo; the colour-space comparison
used to justify linear light is rendered from `out/color_space_compare.motion`
to `out/color_space_compare.png` (see D-0028). Paths, gradients, effects,
masks, typography, images, easing and the showcase are later phases tracked
separately.
