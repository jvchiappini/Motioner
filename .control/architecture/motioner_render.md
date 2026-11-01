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
  rendering: `crates/motioner_render/src/frame.rs:109-156`.
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

## Diagram

Not needed: the plan is a flat list and the painter walks it twice.

## Relevant decisions

- D-0023 — the engine pin and its boundary.
- D-0028 — keyed colours reach the renderer already interpolated in linear
  light; the golden tolerance is shared.
- D-0029 — vector paths, the tessellation cache and the lyon dependency.

## Documentation status

partial — last verified: 2026-11-01
