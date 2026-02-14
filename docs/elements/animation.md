# Animation

Cómo definir animaciones y keyframes en el DSL.

## Easing y funciones
- `linear`, `ease-in`, `ease-out`, `cubic`, `bounce`
- `ease("in-out", t)` — usar en expresiones matemáticas

## Keyframe ejemplo

```dsl
layer "ball" {
  circle(x = 0.1..0.9, y = 0.5, radius = 60)
  anim {
    at 0.0 { circle.x = 0.1 }
    at 1.0 { circle.x = 0.9 }
  }
}
```

## Timeline
- `timeline { fps = 30; duration = 4.0 }`

