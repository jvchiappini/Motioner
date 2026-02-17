# Fade / Opacity (DSL)

Description

Fade animates a shape's opacity over a time range.

Syntax

```
fade(start = <seconds>, end = <seconds>, from = <0.0..1.0>, to = <0.0..1.0>, ease = <easing>)
```

Parameters

- `start`, `end` — start and end times in seconds
- `from`, `to` — opacity values (0.0 = transparent, 1.0 = opaque)
- `ease` (optional) — easing function to apply to interpolation (default: `linear`)

Example

```
circle(name = "Dot", x = 0.5, y = 0.5, radius = 0.05) {
  animations {
    fade(start = 0.0, end = 1.5, from = 0.0, to = 1.0, ease = expo)
  }
}
```
