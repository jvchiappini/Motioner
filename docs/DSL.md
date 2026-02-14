# DSL — Especificación inicial

[!badge:DSL]  {color:#60a5fa}Lenguaje declarativo para escenas y animaciones{/color}

Este documento define la especificación inicial del **DSL** de Motioner: una forma legible y serializable de describir proyectos, timelines y animaciones. El objetivo es que la UI y el motor compartan la misma representación (WYSIWYG + código).

## Principios
- Sintaxis simple y declarativa
- Serializable a JSON/YAML
- Determinista y seguro para ejecución headless
- Soporta expresiones (`t`, `sin`, `ease`) y easing

## Estructura básica
- `project` — contenedor y metadatos
- `layer` — capas (shapes, images, text)
- `timeline` — fps, duración, markers
- `anim` — propiedades animadas (rangos / expresiones)

### Ejemplo mínimo

```dsl
project "Demo" {
  size(1280, 720)
  timeline { fps = 30; duration = 4.0 }

  layer "background" { rect(color = "#0f1113") }

  layer "ball" {
    circle(x = 0.1..0.9, y = 0.5, radius = 60 + 30 * sin(t * PI))
    style(fill = "#78c8ff")
  }
}
```

## Tipos y expresiones
- Números: `123`, `3.14`
- Strings: `"hello"`
- Ranges: `0.0..1.0`
- Identifiers: `t` (tiempo)
- Funciones: `sin()`, `cos()`, `ease("in-out", t)`

## API de ejecución (runtime)
- `parse(code: &str) -> Result<ProjectAST>`
- `render_frame_from_ast(ast, time, size) -> FrameBuffer`
- `serialize(ast) -> JSON`

## Seguridad y sandbox
- No permitir I/O arbitrario en el runtime del DSL
- En cloud, ejecutar en proceso aislado o WASM sandbox

## Versionado y compatibilidad
- Añadir `version` en `project` para migraciones automáticas.

## Siguientes pasos
1. Definir gramática EBNF
2. Implementar parser + AST en `crates/language`
3. Tests unitarios y snapshots
4. Integración UI ↔ código (live-sync)

---

Esta especificación es inicial — la ampliaremos con tipos avanzados (shaders, materiales, import de assets, efectos) y ejemplos. Si quieres, escribo la gramática EBNF ahora y creo el crate `crates/language` con parser POC.
