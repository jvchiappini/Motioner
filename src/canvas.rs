//! Módulo de gestión del canvas de renderizado.
//!
//! Este módulo maneja la visualización de la composición, la gestión de caches (RAM/VRAM),
//! el rasterizado (CPU/GPU) y el procesamiento en segundo plano de los frames.

pub mod tile_cache;
pub mod spatial_hash;
pub mod buffer_pool;
pub mod position_cache;
pub mod rasterizer;
pub mod gpu;
pub mod cache_management;
pub mod preview_worker;
pub mod ui;

// Re-exportar funciones clave para facilidad de uso
pub use ui::show;
pub use preview_worker::{poll_preview_results, request_preview_frames, PreviewJob, PreviewResult, PreviewMode, generate_preview_frames};
pub use position_cache::{build_position_cache_for, PositionCache, position_cache_bytes};

#[cfg(feature = "wgpu")]
pub use gpu::{GpuResources, detect_vram_size};

/// Estructura de compatibilidad para evitar errores de compilación mientras se migra el código.
/// En el futuro, más lógica de canvas.rs se moverá a sub-módulos específicos.
pub struct CanvasManager;

impl CanvasManager {
    pub fn new() -> Self {
        Self
    }
}
