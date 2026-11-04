//! Bounded video frame decode cache and proxy bookkeeping.
//!
//! Preview decodes proxies while playing and originals when paused. Every
//! decoded frame is bounded by a byte budget and an LRU order, so scrubbing
//! cannot grow memory without limit.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use motioner_core::{DecodedImage, ImageAsset, ImageKey};
use motioner_io::{probe_video, FfmpegLocation, Project};

/// Default decode-cache budget: 192 MiB.
pub const FRAME_CACHE_BYTES: usize = 192 * 1024 * 1024;
/// Hard cap on cached frames even for tiny proxies.
pub const FRAME_CACHE_COUNT: usize = 48;
/// Proxy long-edge width.
pub const PROXY_WIDTH: u32 = 640;

/// One decoded frame and its source metadata.
struct CachedFrame {
    width: u32,
    height: u32,
    rgba8: Vec<u8>,
    key: ImageKey,
}

/// A bounded LRU of decoded frames.
#[derive(Default)]
pub struct VideoCache {
    frames: HashMap<(String, i64, bool), CachedFrame>,
    order: VecDeque<(String, i64, bool)>,
    info: HashMap<String, motioner_io::VideoInfo>,
    bytes: usize,
}

impl VideoCache {
    fn get(&mut self, key: &(String, i64, bool)) -> Option<&CachedFrame> {
        if !self.frames.contains_key(key) {
            return None;
        }
        if let Some(index) = self.order.iter().position(|candidate| candidate == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.clone());
        self.frames.get(key)
    }

    fn insert(&mut self, key: (String, i64, bool), frame: CachedFrame) {
        if self.frames.contains_key(&key) {
            let old = self.frames.remove(&key).map_or(0, |old| old.rgba8.len());
            self.bytes = self.bytes.saturating_sub(old);
        }
        self.bytes = self.bytes.saturating_add(frame.rgba8.len());
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

    /// Cached bytes, for the memory readout.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
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

/// Decodes one frame from the original or its proxy.
#[allow(clippy::too_many_arguments)]
pub fn decode(
    project: &Project,
    entry: &VideoEntry,
    cache: &mut VideoCache,
    source_frame: i64,
    proxy: bool,
) -> Option<DecodedImage> {
    let proxy_file = proxy_path(project, &entry.hash);
    let use_proxy = proxy && proxy_done_path(project, &entry.hash).is_file();
    let key = (entry.hash.clone(), source_frame, use_proxy);
    if let Some(frame) = cache.get(&key) {
        return Some(DecodedImage {
            asset: ImageAsset {
                key: frame.key,
                width: frame.width,
                height: frame.height,
            },
            rgba8: frame.rgba8.clone(),
        });
    }
    let info = if let Some(info) = cache.info.get(&entry.hash) {
        info.clone()
    } else {
        let info = probe_video(&entry.path, &FfmpegLocation::FromEnvOrPath).ok()?;
        cache.info.insert(entry.hash.clone(), info.clone());
        info
    };
    #[allow(clippy::cast_precision_loss)]
    let seconds = source_frame as f64 / info.fps();
    let (width, height) = if use_proxy {
        let ratio = f64::from(info.height) / f64::from(info.width.max(1));
        let width = info.width.min(PROXY_WIDTH);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let height = ((f64::from(width) * ratio).round() as u32).max(2);
        (width, height)
    } else {
        (info.width, info.height)
    };
    let rgba8 = motioner_io::decode_frame(
        if use_proxy { &proxy_file } else { &entry.path },
        seconds,
        width,
        height,
        &FfmpegLocation::FromEnvOrPath,
    )
    .ok()?;
    let key_bytes = motioner_io::image_key_from_hex(&entry.hash)?;
    cache.insert(
        key,
        CachedFrame {
            width,
            height,
            rgba8: rgba8.clone(),
            key: key_bytes,
        },
    );
    Some(DecodedImage {
        asset: ImageAsset {
            key: key_bytes,
            width,
            height,
        },
        rgba8,
    })
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

    #[test]
    fn the_frame_cache_evicts_old_frames() {
        let mut cache = VideoCache::default();
        for frame in 0..(FRAME_CACHE_COUNT + 4) {
            cache.insert(
                ("hash".to_owned(), i64::try_from(frame).unwrap(), false),
                CachedFrame {
                    width: 1,
                    height: 1,
                    rgba8: vec![0; 4],
                    key: ImageKey::from_bytes([0; 32]),
                },
            );
        }
        assert!(cache.frames.len() <= FRAME_CACHE_COUNT);
        assert!(cache.get(&("hash".to_owned(), 0, false)).is_none());
    }

    #[test]
    fn proxy_paths_are_per_hash() {
        let project = Project::open(Path::new("."));
        if let Ok(project) = project {
            assert!(proxy_path(&project, "abc").ends_with("abc.proxy.mp4"));
        }
    }
}
