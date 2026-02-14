# Motioner — Project README

[!badge:PROJECT]  {color:#60a5fa}Rust • egui • renderer{/color}

Aplicación mínima en Rust + `egui` para crear/preview de animaciones y exportar video usando `ffmpeg` (CLI).

Características iniciales

- UI con `egui` (eframe)
- Preview animado simple (círculo en movimiento)
- Exportación via `ffmpeg` (CLI)

Requisitos
- Rust (stable)
- `ffmpeg` disponible en PATH (la app usa la herramienta CLI)

Cómo ejecutar
1. Compilar y ejecutar:
   ```bash
   cargo run --release
   ```
2. En la ventana: ajustar FPS / duración y usar `Exportar video`.

Servir la documentación localmente
--------------------------------

Si abres los archivos con `file://` el navegador bloqueará `fetch`. Sirve `docs/` por HTTP:

```cmd
serve-docs.cmd
```

Alternativas: `python -m http.server 8000 --directory docs` o `npx http-server docs -p 8000`.

Luego abre: http://localhost:8000/docs/

Markdown enhancements
---------------------
El viewer soporta algunas extensiones útiles de Markdown (shortcodes):

- Colores inline: `{color:#ff8800}texto{/color}` → muestra `texto` con color.  
- Fondo: `{bg:#222222}texto{/bg}` → pequeño badge con fondo.  
- Badge corto: `[!badge:alpha]` → pequeño distintivo en línea.

También hay un botón "Copiar" en cada bloque de código.
