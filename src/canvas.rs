//! Módulo de gestión del canvas de renderizado.
//!
//! Este módulo maneja la visualización de la composición, la gestión de caches (RAM/VRAM),
//! el rasterizado (CPU/GPU) y el procesamiento en segundo plano de los frames.

pub mod buffer_pool;
pub mod cache_management;
pub mod gpu;
pub mod position_cache;
pub mod preview_worker;
pub mod rasterizer;
pub mod spatial_hash;
pub mod tile_cache;
pub mod ui;

// Re-exportar funciones clave para facilidad de uso
pub use position_cache::{build_position_cache_for, position_cache_bytes, PositionCache};
pub use preview_worker::{
    generate_preview_frames, poll_preview_results, request_preview_frames, PreviewJob, PreviewMode,
    PreviewResult,
};
pub use ui::show;

#[cfg(feature = "wgpu")]
pub use gpu::{detect_vram_size, GpuResources};

/// Estructura de compatibilidad para evitar errores de compilación mientras se migra el código.
/// En el futuro, más lógica de canvas.rs se moverá a sub-módulos específicos.
#[allow(dead_code)]
pub struct CanvasManager;

impl CanvasManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }
}
