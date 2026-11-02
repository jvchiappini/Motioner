# Domain: motioner_render

## Purpose

The Motioner-owned adapter over the pinned FerrousEngine: turns a render plan
into an off-screen frame (headless) or paints it into the editor window's
Pure2D pass (live). Engine types do not leak past this crate except for the
documented `LivePainter` exception.

## Key components

- `FrameRenderer` — the headless pipeline: harness, embedded Roboto, readback:
  `crates/motioner_render/src/lib.rs:137-280`.
- `paint_plan` — the single plan → engine path shared by headless and live
- `paint_plan` opens one engine layer per `PlanLayer` range and closes it
  after the range; a node's blend and effects therefore apply to its whole
  subtree: `crates/motioner_render/src/frame.rs:120-210`.
- Effects render through `push_layer_2d_with_effects` since pin 7a9b7fd
  (D-0032) and masks through `push_layer_2d_masked` since pin e8b9ba7
  (D-0033). `Feature::Mask` / `RenderError::Unsupported` is no longer raised;
  styled text, block layout and per-glyph drawing render since pin 06a8adf
  (D-0034); `Feature::TextLayout` is no longer raised.
  rendering: `crates/motioner_render/src/frame.rs:150-320`.
- `paint_plan` also walks each `PlanLayer.mask` range: it increases the layer
  masked, draws the content items, begins the mask source at `mask.start`,
  draws its items (which may include their own layers and effects), ends it and
  pops, so a blurred source feathers the edge and a shadow is clipped -
  `crates/motioner_render/src/frame.rs:150-320`,
  `crates/motioner_render/src/lib.rs:88-290`.
- `shapes` — resolved rects/circles to `ShapeInstance`; content sizes and
  paints arrive already sampled by the evaluator:
  `crates/motioner_render/src/shapes.rs:26-125`.
- `space` — the composition ↔ world mapping, depth window and the sRGB EOTF
  (delegated to `motioner_core::Color`, D-0028):
  `crates/motioner_render/src/space.rs:24-115`.
- `paths` — tessellates fill and stroke pieces with `lyon_tessellation` into
  local triangles, classifies shared edges as hard, caches per node and
  exposes `path_bounds` (D-0029):
  `crates/motioner_render/src/paths.rs:30-320`.
- `text` — glyph models and missing-glyph counting:
  `crates/motioner_render/src/text.rs`.
- `fonts` — `FontSet` lookup (project faces first, bundled Inter/Roboto last),
  nearest-weight warnings, and `CpuTextMetrics`, the GPU-free
  `motioner_core::TextMetrics` implementation used by `text_width`/`text_bounds`
  (D-0034): `crates/motioner_render/src/fonts.rs`.
- Styled text (`tracking`, `line_height`, `\n`, per-glyph `GlyphDraw2d`)
  goes through `draw_text_2d_styled` / `draw_glyphs_2d`:
  `crates/motioner_render/src/frame.rs`.

## Diagram

Not needed: the plan is a flat list and the painter walks it twice.

## Relevant decisions

- D-0023 — the engine pin and its boundary.
- D-0028 — keyed colours reach the renderer already interpolated in linear
  light; the golden tolerance is shared.
- D-0029 — vector paths, the tessellation cache and the lyon dependency.
- D-0031 — blended nodes render inside an engine layer; overlay/darken/lighten need the backdrop composite and return Unsupported until the pin bump.
- D-0030 — gradients are a per-frame table (`gradients`), rendered since pin
  e9fb90d.
- D-0034 — Inter 400/600/700, project-first font lookup, CPU measurement via
  `TextMetrics`, and per-character animators (rendered since pin 06a8adf).

## Documentation status

partial — last verified: 2026-11-02
