//! Background bakes of the faces a document needs (prompt 17 S2).
//!
//! A document's text faces are independent of the GPU and of the window, so a
//! worker bakes them through the on-disk cache while the engine initialises
//! the device. The frame thread only uploads the finished atlases, which keeps
//! the first frame free of MSDF work.
//!
//! The worker is an optimisation, not a correctness requirement: the live
//! painter registers any face a visible text node needs before it draws (see
//! `motioner_render::LivePainter::ensure_faces`), so a face that has not
//! arrived yet costs a cache-backed upload, never a wrong-font frame.

use std::sync::mpsc::{self, Receiver};
use std::thread;

use motioner_core::Document;
use motioner_render::FontSet;

/// One atlas a worker baked for a face.
pub struct BakedFace {
    /// Family the atlas belongs to, as resolved through the font set.
    pub family: String,
    /// Weight the atlas belongs to.
    pub weight: u16,
    /// The baked atlas, ready to upload on the frame thread.
    pub atlas: ferrous_font::BakedAtlas,
}

/// A running bake of every face `document` uses.
pub struct FontBakes {
    receiver: Receiver<BakedFace>,
}

impl FontBakes {
    /// Starts one worker that bakes the faces `document` uses.
    ///
    /// Returns `None` when the document has no text, when every face is
    /// missing from the font set (nothing to bake), or when the thread cannot
    /// be spawned; the painter then registers faces on demand.
    #[must_use]
    pub fn spawn(
        document: &Document,
        fonts: &FontSet,
        cache: Option<ferrous_font::AtlasCache>,
    ) -> Option<Self> {
        let faces = motioner_render::document_faces(document, fonts);
        if faces.is_empty() {
            return None;
        }
        let (sender, receiver) = mpsc::channel();
        let fonts = fonts.clone();
        thread::Builder::new()
            .name("motioner-fonts".to_owned())
            .spawn(move || {
                for (family, weight) in faces {
                    let Ok(face) = fonts.resolve(&family, weight) else {
                        continue;
                    };
                    let _span = ferrous_engine::startup::span_with(|| {
                        format!("motioner: font worker {family} {weight}")
                    });
                    let Ok(atlas) = ferrous_font::BakedAtlas::build_cached(
                        cache.as_ref(),
                        face.bytes,
                        &ferrous_font::charset::latin_western(),
                    ) else {
                        continue;
                    };
                    if sender
                        .send(BakedFace {
                            family,
                            weight,
                            atlas,
                        })
                        .is_err()
                    {
                        // The window closed: stop baking.
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self { receiver })
    }

    /// The bakes that finished since the last call; never blocks.
    #[must_use]
    pub fn poll(&self) -> Vec<BakedFace> {
        self.receiver.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{Composition, Document, FrameRate, NodeContent, TextAlign, TextContent};
    use std::time::{Duration, Instant};

    const INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Bold.ttf");

    fn document(font: &str) -> Document {
        let rate = FrameRate::new(60, 1).unwrap();
        let mut document = Document::new(
            Composition::new("Test", 320, 180, rate, 10, motioner_core::Color::BLACK).unwrap(),
        );
        document
            .add_node(NodeContent::Text(TextContent::new(
                "Hello",
                font,
                24.0,
                motioner_core::Color::WHITE,
                TextAlign::Left,
            )))
            .unwrap();
        document
    }

    fn set() -> FontSet {
        let mut set = FontSet::default();
        set.push_face("Inter", 700, INTER.to_vec());
        set
    }

    #[test]
    fn a_document_without_text_starts_no_worker() {
        let rate = FrameRate::new(60, 1).unwrap();
        let document = Document::new(
            Composition::new("Empty", 320, 180, rate, 10, motioner_core::Color::BLACK).unwrap(),
        );
        assert!(FontBakes::spawn(&document, &set(), None).is_none());
    }

    #[test]
    fn a_worker_bakes_the_faces_the_document_uses() {
        let Some(bakes) = FontBakes::spawn(&document("Inter"), &set(), None) else {
            panic!("the worker starts");
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let done = bakes.poll();
            if let Some(face) = done.into_iter().next() {
                assert_eq!(face.family, "Inter");
                assert_eq!(face.weight, 700);
                assert!(!face.atlas.pixels.is_empty());
                break;
            }
            assert!(Instant::now() < deadline, "the bake finishes");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn an_unknown_family_bakes_nothing() {
        // "Missing" is not in the set: there is no face to bake and the
        // painter's documented Roboto fallback applies.
        assert!(FontBakes::spawn(&document("Missing"), &set(), None).is_none());
    }
}
