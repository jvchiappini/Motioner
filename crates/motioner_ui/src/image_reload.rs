//! Background re-decodes for image proxies that outgrew their texture
//! (prompt 18 P4).
//!
//! The live painter drops its CPU pixel copies once the texture is uploaded
//! (prompt 18 P3), so a proxy that has to grow when the view zooms in has no
//! pixels left to resample. Rather than keeping every source in memory, the
//! canvas asks for the keys the last frame reported through
//! [`LivePainter::missing_image_pixels`](motioner_render::LivePainter::missing_image_pixels)
//! and a worker re-decodes them from the project source.
//!
//! The worker only decodes: the frame thread installs the result with
//! `LivePainter::set_image`, exactly like a fresh open. A key that cannot be
//! recovered keeps the resident texture, so the preview degrades to the
//! previous resolution instead of showing a placeholder.

use std::sync::mpsc::{self, Receiver};
use std::thread;

use motioner_core::{DecodedImage, ImageKey, ImageResolver};

use crate::Waker;

/// One key a worker finished with.
///
/// A failure is reported too (as `None`), so the caller can stop waiting for
/// that key instead of keeping it in flight forever.
pub struct ReloadedImage {
    /// The key the worker was asked for.
    pub key: ImageKey,
    /// The decoded pixels, when the source could be recovered.
    pub image: Option<DecodedImage>,
}

/// A running re-decode of the keys the canvas is missing.
pub struct ImageReloads {
    receiver: Receiver<ReloadedImage>,
}

impl ImageReloads {
    /// Starts one worker that re-decodes `keys` with `resolver`.
    ///
    /// Returns `None` when there is nothing to reload or the thread cannot be
    /// spawned; the canvas then keeps its resident proxies.
    #[must_use]
    pub fn spawn(
        resolver: Box<dyn ImageResolver + Send>,
        keys: Vec<ImageKey>,
        waker: Waker,
    ) -> Option<Self> {
        if keys.is_empty() {
            return None;
        }
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("motioner-images".to_owned())
            .spawn(move || {
                for key in keys {
                    let image = resolver.decoded_image(&key);
                    if sender.send(ReloadedImage { key, image }).is_err() {
                        // The window closed: stop decoding.
                        break;
                    }
                }
                // Wake the loop once: the results install on the frame this
                // produces, and an idle window presents nothing else.
                waker.wake();
            })
            .ok()?;
        Some(Self { receiver })
    }

    /// The images that finished since the last call; never blocks.
    ///
    /// The flag is `true` while the worker is still decoding (the channel is
    /// empty but connected), so the caller can schedule a short wake.
    #[must_use]
    pub fn poll(&self) -> (Vec<ReloadedImage>, bool) {
        let mut images = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(image) => images.push(image),
                Err(mpsc::TryRecvError::Empty) => return (images, true),
                Err(mpsc::TryRecvError::Disconnected) => return (images, false),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{ImageAsset, ImageResolver};
    use std::time::{Duration, Instant};

    struct Fake {
        image: DecodedImage,
    }

    impl ImageResolver for Fake {
        fn resolve(&self, _source: &str) -> Result<ImageAsset, String> {
            Ok(self.image.asset)
        }

        fn decoded_image(&self, key: &ImageKey) -> Option<DecodedImage> {
            (self.image.asset.key == *key).then(|| self.image.clone())
        }
    }

    fn image() -> DecodedImage {
        DecodedImage {
            asset: ImageAsset {
                key: ImageKey::from_bytes([7; 32]),
                width: 2,
                height: 2,
            },
            rgba8: vec![255; 16],
        }
    }

    #[test]
    fn a_worker_returns_the_requested_pixels() {
        let decoded = image();
        let key = decoded.asset.key;
        let Some(reloads) = ImageReloads::spawn(
            Box::new(Fake {
                image: decoded.clone(),
            }),
            vec![key],
            Waker::default(),
        ) else {
            panic!("the worker starts");
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let (done, _pending) = reloads.poll();
            if let Some(result) = done.into_iter().next() {
                assert_eq!(result.key, key);
                assert_eq!(result.image, Some(decoded.clone()));
                break;
            }
            assert!(Instant::now() < deadline, "the decode finishes");
            thread::sleep(Duration::from_millis(5));
        }
    }

    struct Missing;

    impl ImageResolver for Missing {
        fn resolve(&self, _source: &str) -> Result<ImageAsset, String> {
            Err("missing".to_owned())
        }
    }

    #[test]
    fn a_key_that_cannot_be_decoded_is_still_reported() {
        let key = image().asset.key;
        let Some(reloads) = ImageReloads::spawn(Box::new(Missing), vec![key], Waker::default())
        else {
            panic!("the worker starts");
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let (done, _pending) = reloads.poll();
            if let Some(result) = done.into_iter().next() {
                assert_eq!(result.key, key);
                assert!(result.image.is_none(), "the failure frees the key");
                break;
            }
            assert!(Instant::now() < deadline, "the worker reports the key");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn an_empty_key_list_starts_no_worker() {
        let decoded = image();
        assert!(ImageReloads::spawn(
            Box::new(Fake { image: decoded }),
            Vec::new(),
            Waker::default()
        )
        .is_none());
    }
}
