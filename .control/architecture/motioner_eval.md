# Domain: motioner_eval

## Purpose

Turns a document plus an integer frame into a flat render plan. Pure: no
clock, no randomness, no I/O. It samples transform and content channels,
accumulates opacity down the hierarchy and resolves paints.

## Key components

- `Evaluator::evaluate` â€” depth-first traversal, accumulated transform and
  opacity: `crates/motioner_eval/src/evaluate.rs:44-110`.
- `item_shape` â€” samples every content channel (width, height, radius, size,
  colour) into the plan's resolved shape:
  `crates/motioner_eval/src/evaluate.rs:102-135`.
- `resolved_paint` â€” samples fill, stroke and stroke width; a stroke is drawn
  only when its colour is present and its width is positive:
  `crates/motioner_eval/src/evaluate.rs:132-150`.
- `sample` â€” holds outside the key range, interpolates with the arriving key's
  ease inside it: `crates/motioner_eval/src/curve.rs:95-125`.
- `Interpolate for Color` / `Option<Color>` â€” linear-light midpoint; an
  optional colour with a `none` endpoint holds (D-0028):
  `crates/motioner_eval/src/curve.rs:58-82`.
- `RenderPlan` / `ItemShape` â€” the engine-free contract consumed by
  `motioner_render`: `crates/motioner_eval/src/plan.rs:1-125`.
- `ResolvedPath` / the path arena â€” a path item stores an index and the plan
  owns the sampled geometry, trim, dash and paint; items stay `Copy`
  (D-0029): `crates/motioner_eval/src/plan.rs:41-62` and
  `crates/motioner_eval/src/evaluate.rs:145-180`.

## Diagram

Not needed: one traversal, one output buffer.

## Relevant decisions

- D-0028 â€” content channels, colour space and the golden policy.
- D-0029 â€” path geometry travels in a plan arena, not inside `ItemShape`.
- `EffectValue` / `PlanLayer` - a node with effects (or a non-normal blend)
  wraps its whole drawn subtree in an item range; the plan carries the
  sampled chain, the blend and the range, so nested effects nest:
  `crates/motioner_eval/src/plan.rs:130-200` and
  `crates/motioner_eval/src/evaluate.rs:60-200`.
- `ResolvedGradient` / the gradient arena - `ResolvedPaint` is
  `fill: Option<ResolvedFill>` (`Solid` or a gradient index) plus a copied
  stroke, so items stay `Copy` while gradient stops live in the plan
  (D-0030): `crates/motioner_eval/src/plan.rs:14-52`.

## Documentation status

partial â€” last verified: 2026-11-01
