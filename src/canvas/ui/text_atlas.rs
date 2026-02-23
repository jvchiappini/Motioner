// This module previously generated a CPU-side text atlas for the preview
// pipeline.  After migrating all text rendering to the GPU, the CPU path is
// no longer used.  We keep an empty stub so the module can remain referenced
// in existing `mod` declarations without causing build errors.

use crate::app_state::AppState;

// Alias matching the old return type, but we always return None now.
type AtlasData = (Option<(Vec<u8>, u32, u32)>, Option<Vec<(usize, [f32; 4])>>);

/// Stub that returns no atlas; GPU path handles everything.
pub fn prepare_text_atlas(_state: &mut AppState) -> AtlasData {
    (None, None)
}
