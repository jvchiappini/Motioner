# Domain: motioner_eval

## Purpose

Turns a document plus an integer frame into a flat render plan. Pure: no
clock, no randomness, no I/O. It samples transform and content channels,
accumulates opacity down the hierarchy and resolves paints.

## Key components

- `Evaluator::evaluate` — depth-first traversal, accumulated transform and
  opacity: `crates/motioner_eval/src/evaluate.rs:44-110`.
- `item_shape` — samples every content channel (width, height, radius, size,
  colour) into the plan's resolved shape:
  `crates/motioner_eval/src/evaluate.rs:102-135`.
- `resolved_paint` — samples fill, stroke and stroke width; a stroke is drawn
  only when its colour is present and its width is positive:
  `crates/motioner_eval/src/evaluate.rs:132-150`.
- `sample` — holds outside the key range, interpolates with the arriving key's
  ease inside it: `crates/motioner_eval/src/curve.rs:95-125`.
- `Interpolate for Color` / `Option<Color>` — linear-light midpoint; an
  optional colour with a `none` endpoint holds (D-0028):
  `crates/motioner_eval/src/curve.rs:58-82`.
- `RenderPlan` / `ItemShape` — the engine-free contract consumed by
  `motioner_render`: `crates/motioner_eval/src/plan.rs:1-111`.

## Diagram

Not needed: one traversal, one output buffer.

## Relevant decisions

- D-0028 — content channels, colour space and the golden policy.

## Documentation status

partial — last verified: 2026-11-01
