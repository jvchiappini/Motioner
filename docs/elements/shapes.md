# Shapes

**Descripción**: formas básicas disponibles en el renderer: `circle`, `rect`, `path`.

**Propósito**: construir elementos vectoriales reutilizables en la escena.

## Props comunes
- `x`, `y` — posición normalizada o en píxeles
- `radius` — para `circle`
- `width`, `height` — para `rect`
- `fill`, `stroke`, `stroke_width`

## Ejemplo

```dsl
layer "shapes" {
  rect(x=0.0, y=0.0, width=640, height=360, fill="#0f1113")
  circle(x=0.5, y=0.5, radius=80, fill="#78c8ff")
}
```

## Notas
- Las shapes soportan keyframes y animaciones (ver `animation` docs).
