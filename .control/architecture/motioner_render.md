# Domain: motioner_render

## Purpose

The Motioner-owned adapter over the pinned FerrousEngine: turns a render plan
into an off-screen frame (headless) or paints it into the editor window's
Pure2D pass (live). Engine types do not leak past this crate except for the
documented `LivePainter` exception.

## Key components

- `FrameRenderer` Ã¢â‚¬â€ the headless pipeline: harness, embedded Roboto, readback:
  `crates/motioner_render/src/lib.rs:137-280`.
- `paint_plan` Ã¢â‚¬â€ the single plan Ã¢â€ â€™ engine path shared by headless and live
  rendering: `crates/motioner_render/src/frame.rs:109-156`.
- `shapes` Ã¢â‚¬â€ resolved rects/circles to `ShapeInstance`; content sizes and
  paints arrive already sampled by the evaluator:
  `crates/motioner_render/src/shapes.rs:26-125`.
- `space` Ã¢â‚¬â€ the composition Ã¢â€ â€ world mapping, depth window and the sRGB EOTF
  (delegated to `motioner_core::Color`, D-0028):
  `crates/motioner_render/src/space.rs:24-115`.
- `paths` Ã¢â‚¬â€ tessellates fill and stroke pieces with `lyon_tessellation` into
  local triangles, classifies shared edges as hard, caches per node and
  exposes `path_bounds` (D-0029):
  `crates/motioner_render/src/paths.rs:30-320`.
- `text` Ã¢â‚¬â€ glyph models and missing-glyph counting:
  `crates/motioner_render/src/text.rs`.

## Diagram

Not needed: the plan is a flat list and the painter walks it twice.

## Relevant decisions

- D-0023 Ã¢â‚¬â€ the engine pin and its boundary.
- D-0028 Ã¢â‚¬â€ keyed colours reach the renderer already interpolated in linear
  light; the golden tolerance is shared.
- D-0029 Ã¢â‚¬â€ vector paths, the tessellation cache and the lyon dependency.
- D-0031 — blended nodes render inside an engine layer; overlay/darken/lighten need the backdrop composite and return Unsupported until the pin bump.
- D-0030 A?? gradients are a per-frame table (`gradients`), `RenderError::Unsupported` while the pin lacks the pass, and the `engine-gradients` feature wires it.

## Documentation status

partial Ã¢â‚¬â€ last verified: 2026-11-01
