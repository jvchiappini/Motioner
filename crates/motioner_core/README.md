# motioner_core

The compiled document model: node tree, animatable channels, keyframes,
colours, the rational time base and the validated mutation API. This crate is
the root of the dependency graph; it never depends on graphics, UI or I/O.

## Easing (D-0037)

`Ease` is `Copy + Eq + Hash`. A CSS cubic-bezier curve is a `BezierEase` with
its four control values quantised to one millionth and stored as `i32`, so
`Eq` and `Hash` agree with the displayed numbers. `x1`/`x2` are validated in
`[0, 1]`; `y1`/`y2` may overshoot.

Named presets: `linear`, `quad_*`, `cubic_*`, `back_*`, `hold`, `elastic_out`,
`bounce_out`, `expo_in_out`, `sine_in_out`. `Ease::bezier(...)` builds a
curve; `mdf_value()` / `mdf_literal()` render the canonical
`bezier(x1, y1, x2, y2)` form used by the overrides file, `bake` and `query`.

## Motion-blur settings

`Composition::with_motion_blur(motion_blur, shutter, samples)` validates
`shutter` in `(0, 360]` degrees and `samples` in `1..=64` and stores them on
the composition. `Composition::motion_blur()`, `shutter()` and `samples()` are
read by `motioner_render` and the editor.
