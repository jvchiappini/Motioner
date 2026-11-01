//! In-app screenshots through the engine's presented-frame copy.
//!
//! A request is asked for from `draw_ui` (the engine only honours requests
//! made while drawing), the engine copies the frame it presents, and
//! `on_screenshot` hands the pixels to the [`PngWriter`] the application
//! supplied (`motioner_ui` does not depend on `motioner_io`). Adapters whose
//! surface cannot be copied never deliver; [`Capture`] turns that silence
//! into a typed error after [`DELIVERY_FRAMES`] frames.

use std::path::{Path, PathBuf};

/// Writes tight RGBA8 pixels to a PNG file. Supplied by the application
/// (`motioner_io::write_png`), so the UI crate stays free of export code.
pub type PngWriter = fn(&Path, u32, u32, &[u8]) -> Result<(), String>;

/// Frames to wait for a requested screenshot before giving up.
pub const DELIVERY_FRAMES: u64 = 30;

/// A finished capture: the written file and its size, or why it failed.
pub type CaptureResult = Result<(PathBuf, u32, u32), String>;

/// One screenshot request and where it stands.
#[derive(Debug)]
struct Request {
    path: PathBuf,
    /// Frame counter value when the engine was asked; `None` until then.
    asked_at: Option<u64>,
}

/// Screenshot requests waiting for the engine, and finished results.
#[derive(Debug, Default)]
pub(crate) struct Capture {
    pending: Option<Request>,
    done: Vec<(PathBuf, CaptureResult)>,
}

impl Capture {
    /// Queues a capture to `path`. A second request while one is pending
    /// fails immediately.
    pub(crate) fn request(&mut self, path: PathBuf) {
        if self.pending.is_some() {
            self.done
                .push((path, Err("a screenshot is already being taken".to_owned())));
            return;
        }
        self.pending = Some(Request {
            path,
            asked_at: None,
        });
    }

    /// Called from `draw_ui`: returns `true` when the engine must be asked
    /// for this frame. Also times out a request the engine never delivered.
    pub(crate) fn poll(&mut self, frame_counter: u64) -> bool {
        let Some(request) = self.pending.as_mut() else {
            return false;
        };
        match request.asked_at {
            None => {
                request.asked_at = Some(frame_counter);
                true
            }
            Some(at) if frame_counter.saturating_sub(at) > DELIVERY_FRAMES => {
                let request = self.pending.take();
                if let Some(request) = request {
                    self.done.push((
                        request.path,
                        Err(
                            "the window surface does not support COPY_SRC, so screenshots \
                             cannot be read back on this adapter"
                                .to_owned(),
                        ),
                    ));
                }
                false
            }
            Some(_) => false,
        }
    }

    /// Called from `on_screenshot`: writes the pending request's file.
    pub(crate) fn deliver(
        &mut self,
        writer: Option<PngWriter>,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) {
        let Some(request) = self.pending.take() else {
            return;
        };
        let result = match writer {
            Some(write) => write(&request.path, width, height, rgba)
                .map(|()| (request.path.clone(), width, height)),
            None => Err("no PNG writer was configured for screenshots".to_owned()),
        };
        self.done.push((request.path, result));
    }

    /// Finished captures since the last call, oldest first.
    pub(crate) fn take_done(&mut self) -> Vec<(PathBuf, CaptureResult)> {
        std::mem::take(&mut self.done)
    }
}

#[cfg(test)]
mod tests {
    use super::{Capture, DELIVERY_FRAMES};
    use std::path::{Path, PathBuf};

    #[allow(clippy::unnecessary_wraps)] // must match `PngWriter`
    fn ok_writer(_: &Path, _: u32, _: u32, _: &[u8]) -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn a_request_asks_the_engine_once_and_is_written_on_delivery() {
        let mut capture = Capture::default();
        capture.request(PathBuf::from("a.png"));
        assert!(capture.poll(10));
        assert!(!capture.poll(11));
        capture.deliver(Some(ok_writer), 4, 2, &[0; 32]);
        let done = capture.take_done();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].1, Ok((PathBuf::from("a.png"), 4, 2)));
        assert!(capture.pending.is_none());
    }

    #[test]
    fn an_undelivered_request_times_out_with_a_copy_src_error() {
        let mut capture = Capture::default();
        capture.request(PathBuf::from("a.png"));
        assert!(capture.poll(0));
        assert!(!capture.poll(DELIVERY_FRAMES));
        assert!(!capture.poll(DELIVERY_FRAMES + 1));
        let done = capture.take_done();
        assert!(done[0].1.as_ref().unwrap_err().contains("COPY_SRC"));
    }

    #[test]
    fn a_second_request_while_pending_fails() {
        let mut capture = Capture::default();
        capture.request(PathBuf::from("a.png"));
        capture.request(PathBuf::from("b.png"));
        let done = capture.take_done();
        assert_eq!(done.len(), 1);
        assert!(done[0].1.is_err());
        assert!(capture.pending.is_some());
    }
}
