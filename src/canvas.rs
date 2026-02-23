//! Módulo de gestión del canvas de renderizado.
//!
//! Este módulo maneja la visualización de la composición, la gestión de caches (RAM/VRAM),
//! el rasterizado (CPU/GPU) y el procesamiento en segundo plano de los frames.

pub mod cache_management;
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
pub use gpu::{detect_vram_size, GpuResources};

// The `CanvasManager` stub was originally used by older code, but the
// current architecture places all canvas-related functionality in the
// submodules above (`buffer_pool`, `gpu`, `preview_worker`, etc.).  It is
// no longer referenced anywhere, so keep the module clean by removing the
// placeholder struct entirely.  Any future compatibility helpers can be
// reintroduced when actually needed.
