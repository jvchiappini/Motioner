# Renderer (component)

Descripción técnica del `renderer` crate: API pública, backends y decisiones de diseño.

## API pública (propuesta)
- `render_frame(project: &Project, time: f32, size: Size) -> FrameBuffer`
- `compile_project(code: &str) -> ProjectAST`

## Backends
- CPU raster: pruebas y compatibilidad
- GPU (`wgpu`): performance y efectos

## Testing
- Hash-based frame tests
- Deterministic rendering con `--seed` opcional
