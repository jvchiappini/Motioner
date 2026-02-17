use super::position_cache::position_cache_bytes;
use crate::app_state::AppState;
use eframe::egui;
use image::codecs::png::PngEncoder;
use image::ColorType;
use image::ImageEncoder;

pub fn preview_cache_ram_bytes(state: &AppState) -> usize {
    let mut bytes: usize = 0;
    for (_t, img) in &state.preview_frame_cache {
        let [w, h] = img.size;
        bytes += w * h * 4;
    }
    for (_t, data, _size) in &state.preview_compressed_cache {
        bytes += data.len();
    }
    bytes
}

pub fn preview_cache_vram_bytes(state: &AppState) -> usize {
    state
        .preview_texture_cache
        .iter()
        .map(|(_, _h, s)| *s)
        .sum()
}

pub fn total_preview_cache_bytes(state: &AppState) -> usize {
    preview_cache_ram_bytes(state) + preview_cache_vram_bytes(state) + position_cache_bytes(state)
}

pub fn color_image_to_rgba_bytes(img: &egui::ColorImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(img.size[0] * img.size[1] * 4);
    for c in &img.pixels {
        let arr = c.to_array();
        out.push(arr[0]);
        out.push(arr[1]);
        out.push(arr[2]);
        out.push(arr[3]);
    }
    out
}

pub fn compress_color_image_to_png(img: &egui::ColorImage) -> Option<Vec<u8>> {
    let raw = color_image_to_rgba_bytes(img);
    let mut buf: Vec<u8> = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    if encoder
        .write_image(
            &raw,
            img.size[0] as u32,
            img.size[1] as u32,
            ColorType::Rgba8,
        )
        .is_ok()
    {
        Some(buf)
    } else {
        None
    }
}

pub fn enforce_preview_cache_limits(state: &mut AppState, ctx: &egui::Context) {
    let mut total = total_preview_cache_bytes(state);
    let max_bytes = state.preview_cache_max_mb.saturating_mul(1024 * 1024);
    if max_bytes == 0 || total <= max_bytes {
        return;
    }

    let now_time = state.time;

    if !state.preview_frame_cache.is_empty() {
        state.preview_frame_cache.sort_by(|a, b| {
            let da = (a.0 - now_time).abs();
            let db = (b.0 - now_time).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        while total > max_bytes && state.preview_frame_cache.len() > 1 {
            if let Some((_t, img)) = state.preview_frame_cache.pop() {
                let [w, h] = img.size;
                total = total.saturating_sub(w * h * 4);
            } else {
                break;
            }
        }
        state
            .preview_frame_cache
            .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    }

    if total > max_bytes && !state.preview_compressed_cache.is_empty() {
        state.preview_compressed_cache.sort_by(|a, b| {
            let da = (a.0 - now_time).abs();
            let db = (b.0 - now_time).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        while total > max_bytes && state.preview_compressed_cache.len() > 1 {
            if let Some((_t, data, _)) = state.preview_compressed_cache.pop() {
                total = total.saturating_sub(data.len());
            } else {
                break;
            }
        }
    }

    if total > max_bytes && !state.preview_texture_cache.is_empty() {
        state.preview_texture_cache.sort_by(|a, b| {
            let da = (a.0 - now_time).abs();
            let db = (b.0 - now_time).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        while total > max_bytes && state.preview_texture_cache.len() > 1 {
            if let Some((_t, _handle, size)) = state.preview_texture_cache.pop() {
                total = total.saturating_sub(size);
            } else {
                break;
            }
        }
    }

    if !state.preview_frame_cache.is_empty() {
        let center_idx = state.preview_frame_cache.len() / 2;
        if let Some((_t, center_img)) = state.preview_frame_cache.get(center_idx) {
            let handle = ctx.load_texture(
                "preview_center",
                center_img.clone(),
                egui::TextureOptions::NEAREST,
            );
            state.preview_texture = Some(handle);
        }
    } else if !state.preview_texture_cache.is_empty() {
        let center_idx = state.preview_texture_cache.len() / 2;
        if let Some((_t, handle, _s)) = state.preview_texture_cache.get(center_idx) {
            state.preview_texture = Some(handle.clone());
        }
    } else {
        state.preview_texture = None;
        state.preview_cache_center_time = None;
    }

    if state.preview_cache_auto_clean {
        state.toast_message = Some("Preview cache exceeded limit — auto-cleaned".to_string());
        state.toast_type = crate::app_state::ToastType::Info;
        state.toast_deadline = ctx.input(|i| i.time) + 2.0;
    } else {
        state.toast_message = Some(format!(
            "Preview cache > {} MB — consider clearing or enabling Auto-clean",
            state.preview_cache_max_mb
        ));
        state.toast_type = crate::app_state::ToastType::Info;
        state.toast_deadline = ctx.input(|i| i.time) + 4.0;
    }
}
