# Basic Animation Example

This example shows how to create a simple animation using the DSL (the
user-facing format). Do not put Rust in user docs — use the DSL below.

## DSL example — simple move + fade

Copy this into the Code panel (`Show DSL`) or into a project file to run it in
the UI.

```
size(1280, 720)
timeline(fps = 60, duration = 5.00)

rect "Rect" {
  x = 0.500,
  y = 0.500,
  width = 0.100,
  height = 0.100,
  fill = "#78c8ff",
  spawn = 0.00,
  animations {
    fade(start = 0.0, end = 1.5, from = 0.0, to = 1.0, ease = expo)
  }
}

move {
  element = "Rect",
  to = (0.700, 0.500),
  during = 0.000 -> 5.000,
  ease = ease_in_out(power = 1.000)
}
```

Notes

- Use `move { ... }` exactly as shown — the parser expects `to = (x, y)` and
  `during = start -> end` (see `docs/reference/animations/move.md`).
- Prefer nesting animations inside a shape's `animations { ... }` whenever the
  animation is specific to that element; top-level `move {}` blocks may be
  used to reference elements by name.

## Next steps

- [Creating Animations guide](../user-guide/creating-animations.md)
- [Move animation reference](../reference/animations/move.md)
