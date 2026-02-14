# Motioner UI — prototipo

Aplicación mínima en Rust + `egui` para crear/preview de animaciones y exportar video usando `ffmpeg` (CLI).

Características iniciales
- UI con `egui` (eframe)
- Preview de una animación simple (círculo en movimiento)
- Botón **Exportar video**: renderiza frames a PNG y llama a `ffmpeg` para generar MP4

Requisitos
- Rust (stable)
- `ffmpeg` disponible en PATH (la app usa la herramienta CLI)

Cómo ejecutar
1. Compilar y ejecutar:
   ```bash
   cargo run --release
   ```
2. En la ventana: ajustar FPS / duración y usar `Exportar video`.

Notas
- Esta es la base inicial: la renderización de exportación se hace por CPU (fácil de extender).
- Próximos pasos sugeridos: integrar `wgpu` para render en GPU, añadir timeline, escenas y render por capas.

Servir la documentación localmente
--------------------------------

Si abres los archivos con `file://` el navegador bloqueará peticiones `fetch` (CORS). Para ver `docs/index.html` correctamente sirve la carpeta `docs` por HTTP:

En cmd.EXE (desde la raíz del repo):

```
serve-docs.cmd
```

Alternativas:
- `python -m http.server 8000 --directory docs` (requiere Python 3)
- `npx http-server docs -p 8000` (requiere Node.js)
- Usar la extensión **Live Server** en VS Code y abrir `docs/`

Luego abre: http://localhost:8000/docs/

He incluido un script de ayuda `serve-docs.cmd` y `serve-docs.ps1` en la raíz del repo.
