/// Este archivo contiene constantes y utilidades pequeñas para el renderizado GPU.
/// Incluye detección de VRAM, conversión de color y mapeo de curvas de animación.

#[cfg(feature = "wgpu")]
use eframe::wgpu;

/// Tamaño máximo de textura para el renderizado GPU.
pub const MAX_GPU_TEXTURE_SIZE: u32 = 8192;

/// Detecta el tamaño aproximado de VRAM basado en el tipo de adaptador.
pub fn detect_vram_size(adapter_info: &wgpu::AdapterInfo) -> usize {
    let estimated_vram = match adapter_info.device_type {
        wgpu::DeviceType::DiscreteGpu => 6 * 1024 * 1024 * 1024,
        wgpu::DeviceType::IntegratedGpu => 2 * 1024 * 1024 * 1024,
        wgpu::DeviceType::VirtualGpu => 512 * 1024 * 1024,
        _ => 1024 * 1024 * 1024,
    };

    eprintln!(
        "[VRAM] Detected GPU: {} ({:?}) - Estimated VRAM: {} MB",
        adapter_info.name,
        adapter_info.device_type,
        estimated_vram / (1024 * 1024)
    );

    estimated_vram
}

/// Convierte un valor sRGB [0-255] a espacio lineal [0.0-1.0].
pub(crate) fn srgb_to_linear(u: u8) -> f32 {
    let x = u as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

/// Mapea un `Easing` de la escena a la constante correspondiente en el shader de computación.
pub fn easing_to_gpu(e: &crate::animations::easing::Easing) -> u32 {
    use crate::animations::easing::Easing;
    match e {
        Easing::Linear => 0,
        Easing::EaseIn { .. } => 1,
        Easing::EaseOut { .. } => 2,
        Easing::EaseInOut { .. } => 3,
        Easing::Sine => 4,
        Easing::Expo => 5,
        Easing::Circ => 6,
        // Otras curvas usan linear temporalmente hasta tener soporte completo en WGSL.
        _ => 0,
    }
}
