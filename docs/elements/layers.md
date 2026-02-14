# Layers

Las `layers` son contenedores que agrupan drawables y definen orden, blend-mode y transforms.

## Atributos
- `name` — identificador
- `visibility` — visible / hidden
- `blend` — normal / multiply / screen

## Ejemplo

```dsl
layer "foreground" {
  opacity = 0.9
  circle(x = 0.2..0.8, y = 0.5, radius = 40 + 20*sin(t*PI))
}
```

## Recomendaciones
- Usar layers para organizar escena (background / mid / ui).
- Evitar renderizar demasiadas layers con blending costoso en GPU.
