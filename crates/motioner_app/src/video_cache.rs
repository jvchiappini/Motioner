//! Bounded video frame decode cache and background decode-ahead worker.
//!
//! Preview never blocks the UI thread on ffmpeg: a request is answered from
//! the cache when the frame is already resident and queued to one worker
//! otherwise. While playing, the worker decodes a contiguous batch ahead of
//! the playhead into the bounded LRU; a paused request decodes one full
//! resolution frame. Every decoded frame is bounded by a byte budget and an
//! LRU order, so scrubbing cannot grow memory without limit.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};

use motioner_core::{DecodedImage, ImageAsset, ImageKey};
use motioner_io::{probe_video, FfmpegLocation, Project, VideoInfo};

/// Default decode-cache budget: 192 MiB.
pub const FRAME_CACHE_BYTES: usize = 192 * 1024 * 1024;
/// Hard cap on cached frames even for tiny proxies.
pub const FRAME_CACHE_COUNT: usize = 48;
/// Proxy long-edge width.
pub const PROXY_WIDTH: u32 = 640;
/// Frames decoded ahead of the playhead in one batch.
pub const LOOKAHEAD_FRAMES: i64 = 24;

/// One cache key: asset hash, source frame and preview mode.
type FrameKey = (String, i64, bool);

/// One decoded frame and its source metadata.
struct CachedFrame {
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
    key: ImageKey,
}

/// The answer to one [`decode`] request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameLookup {
    /// The frame is resident and ready to upload.
    Ready(DecodedImage),
    /// The worker is decoding it; ask again on the next UI frame.
    Pending,
    /// The asset or the stream cannot be read.
    Unavailable,
}

/// Shared state between the UI thread and the decode worker.
#[derive(Default)]
struct CacheInner {
    frames: HashMap<FrameKey, CachedFrame>,
    order: VecDeque<FrameKey>,
    info: HashMap<String, VideoInfo>,
    pending: HashSet<FrameKey>,
    bytes: usize,
}

impl CacheInner {
    fn get(&mut self, key: &FrameKey) -> Option<&CachedFrame> {
        if !self.frames.contains_key(key) {
            return None;
        }
        if let Some(index) = self.order.iter().position(|candidate| candidate == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.clone());
        self.frames.get(key)
    }

    fn insert(&mut self, key: FrameKey, frame: CachedFrame) {
        if let Some(old) = self.frames.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.rgba8.len());
        }
        self.bytes = self.bytes.saturating_add(frame.rgba8.len());
        self.pending.remove(&key);
        self.order.push_back(key.clone());
        self.frames.insert(key, frame);
        while self.bytes > FRAME_CACHE_BYTES || self.frames.len() > FRAME_CACHE_COUNT {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.frames.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(old.rgba8.len());
            }
        }
    }

    fn clear_pending(&mut self, hash: &str, start: i64, count: i64, preview: bool) {
        for frame in start..start.saturating_add(count) {
            self.pending.remove(&(hash.to_owned(), frame, preview));
        }
    }
}

/// One decode batch handed to the worker.
struct DecodeJob {
    hash: String,
    original: PathBuf,
    proxy: PathBuf,
    proxy_ready: bool,
    preview: bool,
    width: u32,
    height: u32,
    start_frame: i64,
    count: i64,
    seconds: f64,
    key: ImageKey,
}

impl DecodeJob {
    /// The file this batch reads: the proxy when it exists, the original
    /// otherwise (downscaled to the preview size).
    fn source(&self) -> &std::path::Path {
        if self.preview && self.proxy_ready {
            &self.proxy
        } else {
            &self.original
        }
    }
}

/// A bounded LRU of decoded frames plus the worker that fills it.
pub struct VideoCache {
    inner: Arc<Mutex<CacheInner>>,
    jobs: Sender<DecodeJob>,
}

impl Default for VideoCache {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoCache {
    /// Starts the decode worker.
    #[must_use]
    pub fn new() -> Self {
        let inner = Arc::new(Mutex::new(CacheInner::default()));
        let (jobs, receiver) = mpsc::channel();
        let worker = Arc::clone(&inner);
        let spawned = std::thread::Builder::new()
            .name("motioner-video-decode".to_owned())
            .spawn(move || run_worker(&worker, &receiver));
        if spawned.is_err() {
            // Without a worker every request stays pending; the preview keeps
            // the last uploaded frame instead of blocking or panicking.
            return Self {
                inner,
                jobs: mpsc::channel().0,
            };
        }
        Self { inner, jobs }
    }

    /// Cached bytes, for the memory readout.
    #[must_use]
    pub fn bytes(&self) -> usize {
        lock(&self.inner).bytes
    }
}

fn lock(inner: &Arc<Mutex<CacheInner>>) -> MutexGuard<'_, CacheInner> {
    inner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One open sequential decode kept between batches.
struct StreamState {
    hash: String,
    preview: bool,
    width: u32,
    height: u32,
    /// The next source frame this stream will return.
    next_frame: i64,
    stream: motioner_io::FrameStream,
}

/// Decodes queued batches, keeping one sequential `ffmpeg` stream open while
/// the playhead moves forward so playback does not spawn a process per batch.
fn run_worker(inner: &Arc<Mutex<CacheInner>>, jobs: &Receiver<DecodeJob>) {
    let mut active: Option<StreamState> = None;
    while let Ok(job) = jobs.recv() {
        let count = usize::try_from(job.count.max(1)).unwrap_or(1);
        let mut state = match active.take() {
            Some(state)
                if state.hash == job.hash
                    && state.preview == job.preview
                    && state.width == job.width
                    && state.height == job.height
                    && job.start_frame >= state.next_frame =>
            {
                state
            }
            _ => {
                let Ok(stream) = motioner_io::FrameStream::open(
                    job.source(),
                    job.seconds,
                    job.width,
                    job.height,
                    &FfmpegLocation::FromEnvOrPath,
                ) else {
                    lock(inner).clear_pending(&job.hash, job.start_frame, job.count, job.preview);
                    continue;
                };
                StreamState {
                    hash: job.hash.clone(),
                    preview: job.preview,
                    width: job.width,
                    height: job.height,
                    next_frame: job.start_frame,
                    stream,
                }
            }
        };
        let mut failed = false;
        // A batch can start ahead of the stream when the worker fell behind;
        // discard the intervening frames so the batch is exact.
        while state.next_frame < job.start_frame {
            if let Ok(Some(_)) = state.stream.next_frame() {
                state.next_frame = state.next_frame.saturating_add(1);
            } else {
                failed = true;
                break;
            }
        }
        let mut decoded = 0;
        while !failed && decoded < count {
            match state.stream.next_frame() {
                Ok(Some(rgba8)) => {
                    let frame = state.next_frame;
                    lock(inner).insert(
                        (job.hash.clone(), frame, job.preview),
                        CachedFrame {
                            width: job.width,
                            height: job.height,
                            rgba8,
                            key: job.key,
                        },
                    );
                    state.next_frame = state.next_frame.saturating_add(1);
                    decoded += 1;
                }
                Ok(None) | Err(_) => failed = true,
            }
        }
        // Frames ffmpeg did not return (end of file, decode failure) must not
        // stay pending forever or the UI would retry them every frame.
        lock(inner).clear_pending(&job.hash, job.start_frame, job.count, job.preview);
        if !failed {
            active = Some(state);
        }
    }
}

/// Hash and size of one video asset.
#[derive(Clone)]
pub struct VideoEntry {
    /// Content hash (the asset key).
    pub hash: String,
    /// Blob path.
    pub path: PathBuf,
}

/// Locates the video asset behind a registry key or original name.
#[must_use]
pub fn video_entry(project: &Project, source: &str) -> Option<VideoEntry> {
    let entry = project.find_asset(source)?;
    (entry.kind == motioner_io::AssetKind::Video).then(|| VideoEntry {
        hash: entry.hash.clone(),
        path: project.assets_dir().join(entry.file_name()),
    })
}

/// The proxy path for an asset hash.
#[must_use]
pub fn proxy_path(project: &Project, hash: &str) -> PathBuf {
    project.cache_dir().join(format!("{hash}.proxy.mp4"))
}

/// The completion marker written after a proxy finishes.
#[must_use]
pub fn proxy_done_path(project: &Project, hash: &str) -> PathBuf {
    project.cache_dir().join(format!("{hash}.proxy.done"))
}

/// Answers one frame request from the cache or queues it on the worker.
///
/// `preview` means "playing": the frame comes from the proxy (or the original
/// downscaled to the proxy size) and the worker decodes a lookahead batch.
/// Paused requests decode the original at full resolution.
pub fn decode(
    project: &Project,
    entry: &VideoEntry,
    cache: &VideoCache,
    source_frame: i64,
    preview: bool,
) -> FrameLookup {
    let proxy_file = proxy_path(project, &entry.hash);
    let proxy_ready = preview && proxy_done_path(project, &entry.hash).is_file();
    let key = (entry.hash.clone(), source_frame, preview);
    {
        let mut inner = lock(&cache.inner);
        if let Some(frame) = inner.get(&key) {
            return FrameLookup::Ready(DecodedImage {
                asset: ImageAsset {
                    key: frame.key,
                    width: frame.width,
                    height: frame.height,
                },
                rgba8: frame.rgba8.clone(),
            });
        }
        if inner.pending.contains(&key) {
            return FrameLookup::Pending;
        }
    }
    let cached_info = lock(&cache.inner).info.get(&entry.hash).cloned();
    let info = if let Some(info) = cached_info {
        info
    } else {
        let Ok(info) = probe_video(&entry.path, &FfmpegLocation::FromEnvOrPath) else {
            return FrameLookup::Unavailable;
        };
        lock(&cache.inner)
            .info
            .insert(entry.hash.clone(), info.clone());
        info
    };
    #[allow(clippy::cast_precision_loss)]
    let seconds = source_frame as f64 / info.fps();
    let (width, height) = if preview {
        proxy_dimensions(&info)
    } else {
        (info.width, info.height)
    };
    let Some(key_bytes) = motioner_io::image_key_from_hex(&entry.hash) else {
        return FrameLookup::Unavailable;
    };
    let count = if preview {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let remaining = ((info.duration - seconds).max(0.0) * info.fps()).ceil() as i64;
        LOOKAHEAD_FRAMES.min(remaining.max(1))
    } else {
        1
    };
    let job = DecodeJob {
        hash: entry.hash.clone(),
        original: entry.path.clone(),
        proxy: proxy_file,
        proxy_ready,
        preview,
        width,
        height,
        start_frame: source_frame,
        count,
        seconds,
        key: key_bytes,
    };
    let mut inner = lock(&cache.inner);
    if let Some(frame) = inner.get(&key) {
        return FrameLookup::Ready(DecodedImage {
            asset: ImageAsset {
                key: frame.key,
                width: frame.width,
                height: frame.height,
            },
            rgba8: frame.rgba8.clone(),
        });
    }
    if inner.pending.contains(&key) {
        return FrameLookup::Pending;
    }
    for frame in source_frame..source_frame.saturating_add(count) {
        inner.pending.insert((entry.hash.clone(), frame, preview));
    }
    drop(inner);
    let _ = cache.jobs.send(job);
    FrameLookup::Pending
}

/// The proxy frame size for a source, preserving the aspect ratio.
fn proxy_dimensions(info: &VideoInfo) -> (u32, u32) {
    let ratio = f64::from(info.height) / f64::from(info.width.max(1));
    let width = info.width.min(PROXY_WIDTH);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let height = ((f64::from(width) * ratio).round() as u32).max(2);
    (width, height)
}

/// Decodes `count` small thumbnails evenly across the source.
#[must_use]
pub fn thumbnails(
    _project: &Project,
    entry: &VideoEntry,
    count: usize,
) -> Vec<motioner_core::DecodedImage> {
    let Ok(info) = probe_video(&entry.path, &FfmpegLocation::FromEnvOrPath) else {
        return Vec::new();
    };
    let count = count.max(1);
    let width = 64_u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let height =
        ((f64::from(width) * f64::from(info.height) / f64::from(info.width.max(1))).round() as u32)
            .max(2);
    let Some(key) = motioner_io::image_key_from_hex(&entry.hash) else {
        return Vec::new();
    };
    (0..count)
        .filter_map(|index| {
            #[allow(clippy::cast_precision_loss)]
            let seconds = info.duration * (index as f64 + 0.5) / count as f64;
            let rgba8 = motioner_io::decode_frame(
                &entry.path,
                seconds,
                width,
                height,
                &FfmpegLocation::FromEnvOrPath,
            )
            .ok()?;
            Some(motioner_core::DecodedImage {
                asset: ImageAsset { key, width, height },
                rgba8,
            })
        })
        .collect()
}

/// Starts a background proxy job; the caller stores the progress cell.
pub fn spawn_proxy(source: PathBuf, destination: PathBuf, duration: f64) -> Arc<AtomicU32> {
    let progress = Arc::new(AtomicU32::new(0));
    let cell = Arc::clone(&progress);
    let done = destination.with_extension("done");
    std::thread::spawn(move || {
        let result = motioner_io::make_proxy(
            &source,
            &destination,
            PROXY_WIDTH,
            duration,
            &FfmpegLocation::FromEnvOrPath,
            |fraction| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let value = (fraction.clamp(0.0, 1.0) * 10_000.0) as u32;
                cell.store(value, Ordering::Relaxed);
            },
        );
        if result.is_ok() {
            let _ = std::fs::write(done, b"");
        }
        cell.store(10_000, Ordering::Relaxed);
    });
    progress
}

/// `true` when a proxy file already exists.
#[must_use]
pub fn has_proxy(project: &Project, hash: &str) -> bool {
    proxy_done_path(project, hash).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_frame(value: u8) -> CachedFrame {
        CachedFrame {
            width: 1,
            height: 1,
            rgba8: vec![value; 4],
            key: ImageKey::from_bytes([0; 32]),
        }
    }

    #[test]
    fn the_frame_cache_evicts_old_frames() {
        let mut cache = CacheInner::default();
        for frame_index in 0..(FRAME_CACHE_COUNT + 4) {
            cache.insert(
                (
                    "hash".to_owned(),
                    i64::try_from(frame_index).unwrap(),
                    false,
                ),
                test_frame(0),
            );
        }
        assert!(cache.frames.len() <= FRAME_CACHE_COUNT);
        assert!(cache.get(&("hash".to_owned(), 0, false)).is_none());
    }

    #[test]
    fn inserting_a_frame_clears_its_pending_marker() {
        let mut cache = CacheInner::default();
        let key = ("hash".to_owned(), 3, true);
        cache.pending.insert(key.clone());
        cache.insert(key.clone(), test_frame(1));
        assert!(!cache.pending.contains(&key));
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn proxy_paths_are_per_hash() {
        let project = Project::open(Path::new("."));
        if let Ok(project) = project {
            assert!(proxy_path(&project, "abc").ends_with("abc.proxy.mp4"));
        }
    }
}
