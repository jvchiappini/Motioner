/// Procesa los elementos de texto para generar el atlas que se enviará a la GPU.

use crate::app_state::AppState;

/// Genera los datos del atlas de texto y los overrides de UV para el frame actual.
pub fn prepare_text_atlas(state: &mut AppState) -> (Option<(Vec<u8>, u32, u32)>, Option<Vec<(usize, [f32; 4])>>) {
    let frame_idx = crate::shapes::element_store::seconds_to_frame(state.time, state.preview_fps);
    let mut text_entries: Vec<(usize, crate::scene::Shape, f32)> = Vec::new();

    for (scene_idx, ek) in state.scene.iter().enumerate() {
        if frame_idx < ek.spawn_frame { continue; }
        if let Some(kf) = ek.kill_frame { if frame_idx >= kf { continue; } }

        if ek.kind == "text" {
            if let Some(mut shape) = ek.to_shape_at_frame(frame_idx, state.preview_fps) {
                // Aplicar handlers de eventos para contenido de texto dinámico
                crate::events::time_changed_event::apply_on_time_handlers(
                    std::slice::from_mut(&mut shape),
                    &state.dsl_event_handlers,
                    state.time,
                    frame_idx as u32,
                );
                let spawn_time = ek.spawn_frame as f32 / state.preview_fps as f32;
                text_entries.push((scene_idx, shape, spawn_time));
            }
        }
    }

    if text_entries.is_empty() {
        return (None, None);
    }

    let n_texts = text_entries.len();
    let rw = state.render_width;
    let rh = state.render_height;
    let atlas_h = rh * n_texts as u32;
    let mut atlas = vec![0u8; (rw * atlas_h * 4) as usize];
    let mut text_overrides = Vec::new();

    for (tile_idx, (scene_idx, shape, parent_spawn)) in text_entries.iter().enumerate() {
        let uv0_y = tile_idx as f32 / n_texts as f32;
        let uv1_y = (tile_idx + 1) as f32 / n_texts as f32;

        if let Some(result) = crate::canvas::text_rasterizer::rasterize_single_text(
            shape, rw, rh, state.time, state.duration_secs,
            &mut state.font_arc_cache, &state.font_map, &state.dsl_event_handlers, *parent_spawn
        ) {
            let row_offset = (tile_idx as u32 * rh * rw * 4) as usize;
            let len = (rw * rh * 4) as usize;
            if row_offset + len <= atlas.len() {
                atlas[row_offset..row_offset + len].copy_from_slice(&result.pixels);
            }
        }
        text_overrides.push((*scene_idx, [0.0, uv0_y, 1.0, uv1_y]));
    }

    (Some((atlas, rw, atlas_h)), Some(text_overrides))
}
