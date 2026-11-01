# Domain: motioner_core

## Purpose

The compiled document model: node tree, stable identities, animatable
channels (keyframes and eases) and the validated mutation API that the DSL,
the overrides file and the editor all go through. It depends on nothing else
in the workspace and never touches the file system or a GPU.

## Key components

- `Document` — compositions, nodes, roots and the identity index. Every write
  is validated here: `crates/motioner_core/src/document.rs:122-620`.
- `NodeContent` — what a node draws, with every content property stored as an
  animatable channel (`Animatable<f64>`, `Animatable<Color>`,
  `Animatable<Option<Color>>`): `crates/motioner_core/src/node.rs:314-520`.
- `NodeContent::channel` / `static_value` — the one table that maps a
  `ContentField` to its channel or its static value (D-0028):
  `crates/motioner_core/src/node.rs:492-565`.
- `Document::add_content_keyframe` / `remove_content_keyframe` — the validated
  path for keyed content fields: `crates/motioner_core/src/document.rs:301-360`.
- `Color` — sRGB with straight alpha, the shared linear-light transfer
  function and the interpolation midpoint: `crates/motioner_core/src/color.rs:39-150`.
- `PathGeometry` — authored points with relative Bézier handles, adaptive
  flattening (0.1 px), arc-length trim with wrap, dash splitting, even-odd
  containment and local bounds; the one geometry implementation shared by
  render, bounds and tests (D-0029):
  `crates/motioner_core/src/path.rs:150-560`.
- `OverrideSet` — the editor's edit layer and its canonical text (D-0022):
  `crates/motioner_core/src/overrides.rs:114-266`.

## Diagram

Not needed: the crate is a leaf and its public types are the diagram.

## Relevant decisions

- D-0022 — overrides are keyed by node identity and never edit the program.
- D-0028 — animatable content properties and linear-light colour
  interpolation.
- D-0027 — value origins and the hybrid bake that consumes this model.
- D-0029 — path authoring model and the lyon tessellation backend.

## Documentation status

partial — last verified: 2026-11-01
