//! Módulo de gestión del canvas de renderizado.
//
// Este módulo maneja la visualización de la composición, el rasterizado
// (ahora exclusivamente GPU) y el procesamiento en segundo plano de los
// frames.  El antiguo módulo `cache_management` y las políticas de RAM/VRAM
// fueron eliminadas junto con la lógica de previsualización en CPU.
pub mod gpu;
pub mod preview_worker;
pub mod rasterizer;
pub mod text_rasterizer;
pub mod ui;

// Re-exportar funciones clave para facilidad de uso
// `position_cache` removed — caching logic simplified; keep canvas submodules here.
pub use preview_worker::{
    // generate_preview_frames,
    poll_preview_results,
    request_preview_frames,
    PreviewJob,
    PreviewResult,
};
pub use ui::show;

#[cfg(feature = "wgpu")]
pub use gpu::GpuResources;

// The `CanvasManager` stub was originally used by older code, but the
// current architecture places all canvas-related functionality in the
// submodules above (`buffer_pool`, `gpu`, `preview_worker`, etc.).  It is
// no longer referenced anywhere, so keep the module clean by removing the
// placeholder struct entirely.  Any future compatibility helpers can be
// reintroduced when actually needed.
