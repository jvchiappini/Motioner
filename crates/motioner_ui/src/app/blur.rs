//! The live motion-blur preview (prompt 10 Part A1).
//!
//! When `motion_blur_preview(on)` is set and the composition enables motion
//! blur, the canvas is painted from a headless [`motioner_render::FrameRenderer`]
//! that runs the **same** accumulation pass as `still` and `render`
//! ([`motioner_render::FrameRenderer::render_preview_frame`]). The finished
//! RGBA frame is uploaded as one sprite and drawn in the composition rect, so
//! the preview and the export cannot drift apart.
//!
//! While paused every sample is accumulated; while playing the editor caps the
//! count at four (documented in `docs/agents.md`) so playback keeps a usable
//! frame rate. The sample instants come from
//! [`motioner_render::motion_blur_times`], shared with the render path.

use ferrous_2d::render::SpriteInstance;
use ferrous_renderer::Renderer;
use motioner_core::{Document, Frame};
use motioner_render::{FontSet, FrameRenderer, ImageSet};

use crate::layout::Rect;

/// Sample cap while playback is running.
const PLAYING_SAMPLES: u32 = 4;

/// The offscreen renderer and sprite texture of the preview.
pub(super) struct BlurPreview {
    renderer: Option<FrameRenderer>,
    size: (u32, u32),
    texture: Option<u32>,
    rgba: Vec<u8>,
    images: ImageSet,
    fonts: FontSet,
    /// What the current texture was rendered from, so a paused frame is not
    /// re-rendered on every UI frame.
    last: Option<(u64, Frame, bool)>,
    /// Wall time of the last offscreen render plus upload, in milliseconds.
    last_ms: f32,
}

impl BlurPreview {
    pub(super) fn new(fonts: FontSet) -> Self {
        Self {
            renderer: None,
            size: (0, 0),
            texture: None,
            rgba: Vec::new(),
            images: ImageSet::new(),
            fonts,
            last: None,
            last_ms: 0.0,
        }
    }

    pub(super) fn last_ms(&self) -> f32 {
        self.last_ms
    }

    pub(super) fn set_images(&mut self, images: ImageSet) {
        self.images = images;
        self.last = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_images(self.images.clone());
        }
    }

    /// Adds or replaces one image (a live video frame) without dropping the
    /// other textures.
    pub(super) fn set_image(&mut self, image: motioner_render::ImageData) {
        self.images.insert(image.clone());
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_image(image);
        }
        self.last = None;
    }

    /// Renders `frame` into the composition `rect` and returns `true` when
    /// the sprite was drawn. A failure leaves the normal preview in place.
    #[allow(clippy::too_many_arguments)] // the frame's inputs, kept flat
    pub(super) fn paint(
        &mut self,
        renderer: &mut Renderer,
        document: &Document,
        frame: Frame,
        rect: Rect,
        window: (f32, f32),
        playing: bool,
        generation: u64,
    ) -> bool {
        if rect.is_empty() {
            return false;
        }
        // The plan is evaluated at the composition size; the headless
        // renderer must match it exactly, then the sprite scales the result
        // into the on-screen composition rect.
        let (width, height) = (
            document.composition().width(),
            document.composition().height(),
        );
        if self.size != (width, height) || self.renderer.is_none() {
            match FrameRenderer::with_fonts(width, height, self.fonts.clone()) {
                Ok(mut offscreen) => {
                    offscreen.set_images(self.images.clone());
                    self.renderer = Some(offscreen);
                    self.size = (width, height);
                    self.texture = None;
                }
                Err(_) => return false,
            }
        }
        if self.last != Some((generation, frame, playing)) || self.texture.is_none() {
            let started = std::time::Instant::now();
            let Some(offscreen) = self.renderer.as_mut() else {
                return false;
            };
            let requested = document.composition().samples();
            let samples = if playing {
                requested.min(PLAYING_SAMPLES)
            } else {
                requested
            };
            if offscreen
                .render_preview_frame(document, frame, samples, &mut self.rgba)
                .is_err()
            {
                return false;
            }
            // A same-size preview frame rewrites the texels in place: the
            // texture, sampler and bind group stay alive, so steady-state
            // playback only pays for the upload. A resized texture (or a
            // rejected update) falls back to the free + register path.
            match self.texture {
                Some(texture)
                    if renderer
                        .update_sprite_texture(texture, width, height, &self.rgba)
                        .is_ok() => {}
                Some(texture) => {
                    renderer.free_sprite_texture(texture);
                    self.texture =
                        Some(renderer.register_sprite_texture_linear(width, height, &self.rgba));
                }
                None => {
                    self.texture =
                        Some(renderer.register_sprite_texture_linear(width, height, &self.rgba));
                }
            }
            self.last_ms = started.elapsed().as_secs_f32() * 1000.0;
            self.last = Some((generation, frame, playing));
        }
        let texture = self.texture.unwrap_or(0);
        let center = ferrous_renderer::glam::Vec2::new(
            rect.center_x() - window.0 * 0.5,
            window.1 * 0.5 - rect.center_y(),
        );
        renderer.draw_sprite_2d(
            texture,
            SpriteInstance {
                transform_c0: [rect.w, 0.0, 0.0, 0.0],
                transform_c1: [0.0, rect.h, 0.0, 0.0],
                transform_c2: [0.0, 0.0, 1.0, 0.0],
                transform_c3: [center.x, center.y, 0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                properties: [0.0, 1.0, 0.0, 0.0],
            },
        );
        true
    }
}
