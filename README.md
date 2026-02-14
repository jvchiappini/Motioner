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

![Rust](https://img.shields.io/badge/rust-stable-orange.svg)

# Motioner

Editor prototipo de animaciones construido en Rust. Motioner permite crear, previsualizar y exportar animaciones mediante un flujo de trabajo frame‑by‑frame pensado para integración con herramientas de post‑producción.

## Características

- Interfaz rápida basada en `egui` (eframe)
- Timeline y edición básica de escenas
- Previsualización en tiempo real
- Exportación por frames (PNG) y encodificación con `ffmpeg` (MP4)
- Estructura modular preparada para añadir render por GPU (`wgpu`)

## Requisitos

- Rust (stable) — instalado con `rustup`
- `ffmpeg` disponible en `PATH`

## Quickstart

1. Clona el repositorio:

```bash
git clone https://github.com/jvchiappini/Motioner.git
cd Motioner
```

2. Ejecuta en modo desarrollo:

```powershell
cargo run
```

3. Ejecuta en modo release (optimizado):

```powershell
cargo run --release
```

4. En la UI: ajustar FPS/duración/escena → `Exportar video`.

## Export manual (ejemplo)

Si la app genera frames en `out/frames`:

```powershell
ffmpeg -framerate 30 -i out/frames/frame_%05d.png -c:v libx264 -pix_fmt yuv420p out/movie.mp4
```

## Desarrollo

- Formatear: `cargo fmt`
- Linter: `cargo clippy`
- Compilar (release): `cargo build --release`

### Estructura relevante

- `src/` — código fuente
- `scripts/` — utilidades y herramientas de mantenimiento
- `target/` — artefactos de compilación

## Contribuir

- Abrir issues para bugs o propuestas
- Crear ramas `feat/xxx` o `fix/xxx` y enviar PRs
- Mantener commits claros y agregar pruebas cuando aplique

## Roadmap (prioritario)

- Integración `wgpu` para render por GPU
- Timeline avanzado con keyframes y easing
- Export por capas y soporte de audio

## Licencia

License: Not specified

## Contacto

- Mantenedor: `@jvchiappini`


