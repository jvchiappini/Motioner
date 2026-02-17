# Animations DSL (Reference)

This section documents Motioner's animation DSL — the recommended, stable surface for creating animations in project files and code-panel scripts. The runtime and UI operate on the same DSL structures used by editors and examples.

Quick links

- [Move animation](move.md)
- [Opacity / Fade](fade.md)
- [Scale / Rotate](transform.md)
- [Easing options & parameters](easing.md)

Overview

- Motioner uses a small, human-friendly DSL (no Rust required) to declare scene elements and attach animations.
- Files and code-panel DSL examples should be used by end users and contributors to author animations.
- The UI (Scene Graph, Animations modal, Easing editor) reads/writes the same DSL.

Top-level timeline settings

Use `timeline(...)` to set project timing:

Example:

timeline(fps = 30, duration = 5.0)

- `fps` — frames per second (integer)
- `duration` — seconds (float)

Scene / element declaration

- `circle { ... }`, `rect { ... }`, and `group { ... }` declare scene shapes.
- Shapes accept properties (position, size, color, spawn_time) and an `animations { ... }` block.

Example (full DSL):

```
timeline(fps = 30, duration = 5.0)

circle(name = "Circle", x = 0.1, y = 0.5, radius = 0.1, color = [255,255,255,255]) {
  animations {
    move(to_x = 0.8, to_y = 0.5, start = 0.0, end = 2.0, ease = sine)
    fade(start = 0.0, end = 1.0, from = 0.0, to = 1.0)
  }
}
```

See the individual pages for each animation type to learn parameters and examples.
