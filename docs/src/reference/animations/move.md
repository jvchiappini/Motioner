# Move (DSL)

Description

Move animates an element's position from its current coordinates toward a specified target over a time interval.

Syntax

```
move(to_x = <0.0..1.0>, to_y = <0.0..1.0>, start = <seconds>, end = <seconds>, ease = <easing>)
```

Parameters


Notes


Example

```
rect(name = "Box", x = 0.1, y = 0.2, w = 0.2, h = 0.2) {
  animations {
  move {
    element = "Box",
    to = (0.800, 0.200),
    during = 0.000 -> 2.000,
    ease = linear
  }
  }
}
```
# Move (DSL)

Description

The `move` block declares a position animation. The DSL supports two valid placements:

- inside a shape's `animations { ... }` block (recommended for local animations), or
- as a top-level `move { ... }` block that references an element by name via `element = "..."`.

This is the exact DSL that the parser accepts (fields and names must match).

Syntax (block form)

```
move {
  element = "<optional-element-name>",   # optional when nested inside a shape
  to = (<x>, <y>),                         # normalized 0.0..1.0
  during = <start_seconds> -> <end_seconds>,
  ease = <easing-spec>                      # default: linear
}
```

Parameters

- `element` — string, optional when `move` is inside a shape's `animations` block; required for top-level `move` blocks.
- `to` — tuple (x, y) with normalized coordinates (0.0..1.0).
- `during` — time range written as `start -> end` (seconds, floats).
- `ease` — easing expression (e.g. `linear`, `sine`, `ease_in_out(power = 1.0)`, etc.). Default is `linear`.

Behavior notes

- Multiple `move` animations attached to the same element are applied in chronological order.
- When the playhead is inside a `during` interval the element is interpolated from its position at animation start toward the `to` target using the specified easing.
- The parser accepts both the nested (`animations { move { ... } }`) and top-level `move { ... }` forms; top-level moves will be attached to the named element if it exists.

Example — top-level move block (exact form accepted by parser)

```
size(1280, 720)
timeline(fps = 60, duration = 5.00)

circle "Circle" {
  x = 0.500,
  y = 0.500,
  radius = 0.100,
  fill = "#78c8ff",
  spawn = 0.00
}

move {
  element = "Circle",
  to = (0.700, 0.500),
  during = 0.000 -> 5.000,
  ease = ease_in_out(power = 1.000)
}
```

See also: the easing reference in the developer docs (Easing functions — `docs/src/advanced/custom-animations.md`).
