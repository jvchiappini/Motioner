//! Este archivo contiene constantes y utilidades pequeñas para el renderizado GPU.
//! Incluye detección de VRAM, conversión de color y mapeo de curvas de animación.

// wgpu is used in other parts of this module; import inline when needed.

/// Tamaño máximo de textura para el renderizado GPU.
pub const MAX_GPU_TEXTURE_SIZE: u32 = 8192;

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
        Easing::Step => 7,
        // Otras curvas usan linear temporalmente hasta tener soporte completo en WGSL.
        _ => 0,
    }
}
