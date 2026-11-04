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
- `Ease` / `BezierEase` — named presets plus a CSS cubic-bezier curve whose
  four control values are quantised to 1e-6 as `i32`, keeping `Eq`/`Hash`
  honest; `mdf_value`/`mdf_literal` round-trip the overrides and bake forms
  (D-0037): `crates/motioner_core/src/animation.rs:9-260`.
- `Composition::with_motion_blur` — validates shutter `(0, 360]` and samples
  `1..=64`; `motion_blur`, `shutter`, `samples` are read by render and the
  editor (D-0037): `crates/motioner_core/src/document.rs:20-150`.
- `Color` — sRGB with straight alpha, the shared linear-light transfer
  function and the interpolation midpoint: `crates/motioner_core/src/color.rs:39-150`.
- `PathGeometry` — authored points with relative Bézier handles, adaptive
  flattening (0.1 px), arc-length trim with wrap, dash splitting, even-odd
  containment and local bounds; the one geometry implementation shared by
  render, bounds and tests (D-0029):
  `crates/motioner_core/src/path.rs:150-560`.
- `Gradient` / `PaintSpec` - linear and radial geometry plus stops whose
  offset and colour are `Animatable` channels; paint stores an optional
  gradient per fill/stroke and `ContentField` exposes one offset and one
  colour channel per stop (D-0030):
  `crates/motioner_core/src/gradient.rs:1-300` and
  `crates/motioner_core/src/node.rs:91-260`.
- `BlendMode` - seven node blend modes (normal, add, multiply, screen,
  overlay, darken, lighten); a discrete node property, overridable, not
  keyable (D-0031): `crates/motioner_core/src/node.rs:296-375`.
- `Effect` / `EffectChannel` - the ordered effect stack (drop shadow, blur,
  glow), one effect per kind, with every parameter an animatable channel
  named `<kind>.<param>` (D-0032): `crates/motioner_core/src/effect.rs:1-260`.
  - `MaskSpec` / `MaskMode` - a node's mask: source node, `alpha` or `luma`
  coverage and `invert` (D-0033). `Document::set_mask` validates the source
  (no self, no ancestor, no cycle; `can_mask` is the same rule) and
  `is_mask_source` tells evaluation which nodes are hidden from the normal
  stream. The `mask` override stores `"<source>:<mode>:<invert>"` or `"none"`:
  `crates/motioner_core/src/mask.rs:1-118`,
  `crates/motioner_core/src/document.rs:560-700`.
`Document::set_effect_stack` rebuilds the stack from an `effects` override
  while preserving parameters: `crates/motioner_core/src/document.rs:580-740`.
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
- D-0031 - the seven blend modes are a whole-node, non-animatable property.

## Documentation status

partial — last verified: 2026-11-01

## Images (D-0035)

- `NodeContent::Image(ImageContent)` stores a project-relative `source`, an
  optional resolved `ImageAsset { key, width, height }`, an animatable
  destination `width`/`height` (`0` = natural) and a discrete `fit`.
  `ImageLayout::compute` is the one contain/cover/fill arithmetic.
- `ContentField::Source` and `ContentField::Fit` are discrete; `Width` and
  `Height` are the usual scalar channels, so `set`/`edit`/`key` work by name.
- `Document::resolve_images(&dyn ImageResolver)` fills `asset` and reports
  `ImageIssue`s; core never reads a file. `ImageKey` is the SHA-256 digest and
  `DecodedImage` is the renderer-facing byte record.

## Audio (D-0048)

- `NodeContent::Audio(AudioContent)` (`crates/motioner_core/src/node.rs:1244`)
  stores `source`, `in_point`/`out_point` (seconds), `gain`/`pan` as
  animatable channels, `start` in composition frames and the fade lengths.
- `ContentField` gains `in_point`, `out_point`, `gain`, `pan`, `start`,
  `fade_in` and `fade_out` (`crates/motioner_core/src/node.rs:97`); only
  gain and pan are keyable. Audio nodes carry no draw content, so the
  evaluator skips them.
