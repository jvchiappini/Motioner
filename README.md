<!-- Badges (replace/enable as you add CI, crates, license, etc.) -->
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)

# Motioner — editor / prototipo de animaciones

Motioner es un prototipo ligero escrito en **Rust** para diseñar, previsualizar y exportar animaciones. Está pensado como punto de partida para experimentar con timelines, render por capas y export frame‑by‑frame para generar vídeos mediante `ffmpeg`.

> Presentación breve: interfaz rápida con `egui`, exportación por frames y un flujo de trabajo pensado para extensiones (GPU rendering, plugins, export avanzados).

---

🎯 **Qué hace Motioner**

- Interfaz de usuario ligera con `egui` (eframe)
- Previsualización interactiva (timeline + escenas)
- Exportación frame‑by‑frame → `ffmpeg` (genera MP4 desde PNG)
- Código modular y fácil de extender para integrar `wgpu`/GPU rendering


✨ **Para quién**

- Desarrolladores y creadores que necesitan un prototipo rápido para generar animaciones programáticas
- Proyectos que requieren exportar renders como secuencia de imágenes para post‑procesado o encoding


---


Características principales

- Timeline y edición básica de escenas
- Export por frames (PNG) con encuadre a vídeo vía `ffmpeg`
- Código en Rust pensado para experimentar (facilidad para añadir render por GPU)
- Herramientas de desarrollo y scripts auxiliares en `scripts/`


---


Requisitos

- Rust (stable) — instalado con `rustup`
- `ffmpeg` disponible en `PATH` (se invoca desde la app para generar MP4)
- (Opcional) Drivers/SDK para GPU si integras `wgpu` en el futuro

---

## Quickstart

Windows / PowerShell:

```powershell
# Ejecutar en modo desarrollo
cargo run

# Modo release (optimizado)
cargo run --release
```

Uso: abre la ventana, ajusta FPS/duración/escena y pulsa `Exportar video` para crear la secuencia y encodificarla con `ffmpeg`.

---

## Ejemplo de export manual (si prefieres reproducir el flujo)

1. Ejecuta la app y usa la opción Exportar → genera una carpeta `out/frames` con PNGs.
2. En terminal, encadena con ffmpeg:

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png -c:v libx264 -pix_fmt yuv420p out/movie.mp4
```

---

## Desarrollo

- Formatea: `cargo fmt`
- Linter: `cargo clippy`
- Compilar release: `cargo build --release`

Estructura relevante:

- `src/` — código fuente principal
- `scripts/` — utilidades (p. ej. `rewrite_git_author.py`)
- `target/` — artefactos de compilación

---

## Contribuir

- Abre un issue si encuentras bugs o propones features.
- Crea una rama descriptiva `feat/xxx` o `fix/xxx` y envía un PR.
- Sigue mensajes de commit claros (pref. Conventional Commits).

¿Quieres que añada un `CONTRIBUTING.md` y plantillas de PR? Puedo generarlas.

---

## Roadmap (ideas)

- Integración `wgpu` para render por GPU
- Timeline avanzado (clips, keyframes, easing)
- Export por capas y soporte de audio
- Formato de proyecto + import/export de escenas

---

## Licencia

Actualmente no hay un `LICENSE` en el repo; ¿prefieres **MIT** o **Apache-2.0**? Dime cuál y lo agrego.

---

## Contacto

- Mantenedor: `@jvchiappini` — abre issues o PRs en GitHub.

---

_Nota_: la carpeta `docs/` fue eliminada; si quieres que publique documentación pública (GitHub Pages) puedo recrearla y configurar el workflow.



Quickstart (Windows — PowerShell)

1) Compilar y ejecutar (modo desarrollo):

```powershell
cargo run
```

2) Ejecutar release (optimizado):

```powershell
cargo run --release
```

3) Exportar video desde la UI: ajustar FPS/duración → botón `Exportar video`.

Servir la documentación local

Para abrir `docs/index.html` correctamente (evitar problemas CORS) puedes servir la carpeta `docs` localmente:

```powershell
# script incluido (Windows)
serve-docs.cmd

# alternativa con Python
python -m http.server 8000 --directory docs
```

Abrir en el navegador: `http://localhost:8000/docs/`

---

Desarrollo y contribución

- Clona el repo, crea una rama por feature y abre un PR.
- Sigue mensajes de commit descriptivos (conventional style recomendado).
- Tests / formateo: usa `cargo fmt` y `cargo clippy` cuando apliquen.

Si quieres colaborar, abre un _issue_ o un _pull request_ en GitHub.

---

Archivo de proyecto y estructura rápida

- Código fuente: `src/`
- Documentación y guías: `docs/`
- Scripts de ayuda: `serve-docs.cmd`, `serve-docs.ps1`
- Utilidades: `scripts/` (herramientas de mantenimiento)

---

Contacto & próximos pasos

- Mantenedor: `@jvchiappini`
- Próximas mejoras sugeridas: GPU rendering (`wgpu`), timeline avanzado, export por capas.

---

¿Añadimos una demo GIF o un `LICENSE`? Puedo preparar ambos (dime qué licencia prefieres).


