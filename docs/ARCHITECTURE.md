# Motioner — Arquitectura (Documento maestro)

[!badge:ARCHITECTURE]  {color:#4fd1c5}Visión general del sistema — Desktop · Web · Cloud{/color}

Este documento resume la arquitectura de Motioner y las decisiones técnicas clave. Está pensado como referencia viva para desarrolladores y contribuyentes: cada cambio arquitectónico debe documentarse aquí.

## Tabla de contenido
- [Visión general](#visión-general)
- [Componentes y estructura](#componentes-y-estructura-de-código-propuesta)
- [Flujo de datos](#flujo-de-datos-y-responsabilidades)
- [Render pipeline (CPU / GPU)](#render-pipeline-detalle-técnico)
- [Exportación y ffmpeg](#exportación-y-ffmpeg)
- [Multiplataforma y empaquetado](#multiplataforma-y-empaquetado)
- [Web (WASM) — estrategia](#web-wasm--estrategia)
- [Servicio en la nube (headless)](#servicio-en-la-nube-headless--diseño-propuesto)
- [CI / CD y calidad](#ci--cd-y-calidad)
- [Contribuciones y convenciones](#contribuciones-y-convenciones-oss)
- [Roadmap](#roadmap-priorizado)
- [Preguntas abiertas](#preguntas-abiertas--decisiones-que-necesito-que-confirmes)

---

## Visión general
Motioner separa claramente la **UI**, el **motor de render** y la **capa de export**. Objetivos clave:

- {color:#60a5fa}Desktop nativo{/color}: Windows / macOS / Linux con `egui` + `wgpu`.
- {color:#4fd1c5}Motor reusable{/color}: crate `renderer` que se usa en UI y en workers headless.
- {color:#f59e0b}Export robusto{/color}: uso de `ffmpeg` en desktop/cloud; export desde servidor para producción.
- Open‑source, modular y testeable (determinismo y snapshots).

---

## Componentes y estructura de código (propuesta)
Monorepo modular (sugerido):

- **crates/app-ui** — binary: `egui` + preview `wgpu` (editor desktop)
- **crates/renderer** — lib: API de render (CPU & GPU backends)
- **crates/cli-render** — binary: headless renderer para CI / workers
- **crates/formats** — serialización, import/export de proyectos
- **crates/services** — adaptadores para storage / auth (opcional)
- **docs/**, **examples/** — documentación y ejemplos

Razonamiento: mantener el renderer independiente permite reuse, testing y despliegue en servidores.

---

## Flujo de datos y responsabilidades

- **UI (app-ui)** → construye `Project`/`Scene` desde timeline, keyframes y propiedades.
- **Renderer (renderer crate)** → `render_frame(project, time, size) -> FrameBuffer` (CPU/GPU backends).
- **Export** → frames → `ffmpeg` → MP4/WebM (desktop / headless worker).
- **Cloud** → client encola job → worker procesa usando `cli-render` → sube artefactos a object storage.

---

## Render pipeline (detalle técnico)

- **Scene graph** ligero: capas, transforms, shapes, text, materiales y keyframes.
- **API** pública: `render_frame(project, time, size) -> FrameBuffer`.
- **Backends**:
  - CPU raster (portable, fallback)
  - GPU (`wgpu`) — Backend principal. Utiliza un pipeline de fragment shaders (WGSL) con Signed Distance Fields (SDF) para renderizado de alta fidelidad, permitiendo:
    - **Infinite Canvas**: Rejilla infinita estilo CAD con paneo y zoom.
    - **Low-res Preview**: Rasterización controlada con Anti-Aliasing (AA) basado en la resolución física de salida.
    - **Magnifier/Color Picker**: Herramienta de inspección por GPU (Lupa 8x) con rejilla de píxeles y muestreo de color exacto (paridad CPU/GPU).
- **Determinismo**: outputs reproducibles (hashable) para tests y caching.

---

## Lenguaje de programación (DSL) y sincronización UI → código

Motioner incluirá un **DSL** declarativo para describir escenas y animaciones. La UI y el renderer compartirán la misma representación textual/AST — esto permite edición WYSIWYG y programación simultánea.

### Objetivos del DSL
- Sintaxis simple y legible para diseñadores y programadores
- Representar escenas, capas, transforms, keyframes y expresiones
- Serializable a JSON/YAML y ejecutable por el renderer (AST)
- Runtime seguro y determinista (apto para cloud)

### Ejemplo de sintaxis (propuesto)
```dsl
project "Demo1" {
  size(1280, 720)
  timeline { fps = 30; duration = 4.0 }

  layer "background" { rect(color = "#101214") }

  layer "ball" {
    circle(x = 0.1..0.9, y = 0.5, radius = 60 + 30 * sin(t * PI))
    style(fill = "#78c8ff")
  }
}
```

La UI generará/sincronizará automáticamente código equivalente cuando el usuario arrastre/añada objetos en el canvas; a la inversa, editar el código actualizará la UI (live-sync).

### Runtime / interpretación
- El `renderer` expondrá una función `render_frame_from_code(code: &str, time: f32) -> Frame`.
- Implementar un parser ligero que genere un AST y validaciones (errores legibles para mostrar en UI).
- Ejecutar expresiones seguras (sin acceso a I/O arbitrario) — sandboxing y límites de CPU/mem para ejecución server-side.

### UI ↔ Code sync (Live editing)
- Cada objeto en el canvas tendrá un `source_id` que mapea al nodo AST / rango de texto.
- Ediciones GUI <-> modificaciones del código se sincronizan bidireccionalmente.
- Undo/redo y diffs deben funcionar sobre el AST para evitar inconsistencias.

### Integración con el flujo headless / cloud
- El API de render headless acepta tanto `Project` serializado (JSON) como código DSL; el worker compila/parsea y ejecuta con el mismo runtime.
- Para seguridad en cloud, ejecutar el DSL en un proceso sandbox o en WebAssembly (WASM) runtime con limites.

### Testing y reproducibilidad
- Tests unitarios del parser e intérprete (snapshots de AST).
- Tests de render que comparen hashes deterministas de frames.

---

---

## Exportación y `ffmpeg`

- **Desktop / Headless**: preferir `ffmpeg` CLI (libx264, yuv420p) por compatibilidad.
- **Web**: `ffmpeg.wasm` es aceptable para demos, pero no recomendado en producción.

> Nota: documentar `ffmpeg` como prerequisito y validar su presencia antes de exportar.

---

## Multiplataforma y empaquetado

`eframe` + `wgpu` permiten builds nativos para Windows, macOS y Linux. Empaquetado recomendado:

- Windows: NSIS / `cargo-bundle`
- macOS: `.app` (firmado + notarized para releases)
- Linux: AppImage / Flatpak / distro packages

Usar GitHub Actions para matrix builds y artefactos.

---

## Web (WASM) — estrategia
- Objetivo: portar la UI a WASM para edición ligera (preview interactiva).
- Limitaciones:
  - Acceso a sistemas de archivos y herramientas nativas (ffmpeg) no disponible
  - WebGPU/ WebGL dependencia del navegador
- Estrategia recomendada:
  - Soporte WASM solo para UI/preview/editor
  - Export final → job al servidor (headless)
  - Opcional: `ffmpeg.wasm` para pruebas/mini-exports (no recomendado en prod)

---

## Servicio en la nube (headless) — diseño propuesto

Arquitectura básica:

- **API**: recibir jobs (POST /render-job) + auth
- **Queue**: SQS / RabbitMQ / Redis streams
- **Workers**: contenedores que ejecutan `cli-render` + `ffmpeg`
- **Storage**: S3 / Blob para frames y vídeos

Job lifecycle: client -> queue -> worker -> storage -> callback/status

Escalado: workers stateless; provisionar GPU nodes para render acelerado.

---

## CI / CD y calidad

Usar GitHub Actions con matrix builds. Recomendado:

- `cargo fmt` + `clippy` en PRs
- Unit + integration tests (renderer deterministic hashes)
- Publish release artifacts y Docker images

---

## Contribuciones y convenciones (OSS)

- License: elegir MIT o Apache-2.0 (indica preferencia)
- Código: `rustfmt`, `clippy`, tests en CI
- PRs: una feature por PR, incluir tests y docs
- Issues: usar templates y etiquetas claras

---

## Roadmap (priorizado)
1. Desktop: `wgpu` preview y export nativo
2. Extraer `renderer` crate + `cli-render`
3. Dockerize workers + REST job API
4. CI: cross-platform builds y artefactos
5. WASM UI (editor ligero) + export server-side
6. Servicio multi-usuario (largo plazo)

---

## Ejemplo de API (headless) — POST /render-job
Request body (JSON):
```json
{
  "project": { /* Project JSON or DSL */ },
  "width": 1280,
  "height": 720,
  "fps": 30,
  "format": "mp4"
}
```
Response: `{"job_id":"...","status_url":"/jobs/:id"}`

---

## Testing / reproducibilidad
- Exportar `Project` JSON y almacenar junto a `out.mp4` para reproducibilidad.
- Añadir `--deterministic` flag al CLI que fija semillas y desactiva nondet features.

---

## Archivos que actualizar cuando cambie la arquitectura
- `docs/ARCHITECTURE.md` (este archivo)
- `docs/ROADMAP.md`
- `crates/renderer/README.md` (API pública)
- `CONTRIBUTING.md` (guías de contribución)

---

## Preguntas abiertas / decisiones que necesito que confirmes
- ¿Licencia preferida? (MIT / Apache-2.0)
- ¿CI por defecto: GitHub Actions? (recomendado)
- ¿Cloud objetivo (AWS / Azure / GCP / otro)?
- ¿Agregar plantillas de issues/PR en `.github/`? 

---

## Siguientes acciones recomendadas
- Crear `crates/renderer` y mover la lógica de render actual dentro de él.
- Implementar backend `wgpu` para `app-ui` y un `cli-render` para headless.
- Añadir CI con matrix multiplataforma.

---

> Si quieres, puedo generar ahora los archivos iniciales para `crates/renderer`, `cli-render` y las plantillas de CI/PR — dime qué decisiones (licencia/CI/cloud) prefieres y procedo.  
