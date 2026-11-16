//! Stereo playback on the default output device (D-0048).
//!
//! The mixer lives in `motioner_io`; this module only owns the cpal stream
//! and a shared cursor. The transport reads [`AudioOutput::position`] while
//! playing so the playhead follows the audio clock instead of the wall clock.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use motioner_core::{Document, NodeContent};
use motioner_io::{
    decode_to_cache, mix_document, AssetKind, AudioStore, FfmpegLocation, Project, SAMPLE_RATE,
};

/// A running stereo stream and its shared cursor.
/// A stereo device driven by a worker thread.
///
/// `cpal::Stream` is not `Send`, so the stream is built and kept on the
/// `motioner-audio` worker. The window only sends small commands and reads the
/// shared cursor; opening the device (the expensive part on Windows) never
/// blocks a frame (prompt 17 S5).
pub struct AudioOutput {
    commands: std::sync::mpsc::Sender<Command>,
    playback: Arc<Mutex<Playback>>,
    meta: Arc<Mutex<Meta>>,
}

/// State the UI thread reads without touching the stream.
struct Meta {
    origin_frame: i64,
    fps: f64,
    silent: bool,
    /// The output stream exists (or was opened) and can be started.
    device_ready: bool,
    /// The buffer is playing; the cursor can be trusted.
    ready: bool,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            origin_frame: 0,
            fps: 1.0,
            silent: true,
            device_ready: false,
            ready: false,
        }
    }
}

/// Commands the UI sends to the audio worker.
enum Command {
    Open,
    Play {
        samples: Vec<f32>,
        origin_frame: i64,
        fps: f64,
        silent: bool,
    },
    Start(i64),
    Stop,
    Shutdown,
}

impl AudioOutput {
    /// Creates the worker; the device opens on [`Self::prewarm`] or the first
    /// [`Self::play`].
    #[must_use]
    pub fn new() -> Self {
        let playback = Arc::new(Mutex::new(Playback {
            samples: Vec::new(),
            cursor: 0,
            frames: 0,
        }));
        let meta = Arc::new(Mutex::new(Meta::default()));
        let (commands, receiver) = std::sync::mpsc::channel();
        let worker_playback = Arc::clone(&playback);
        let worker_meta = Arc::clone(&meta);
        let _ = std::thread::Builder::new()
            .name("motioner-audio".to_owned())
            .spawn(move || worker(worker_playback, worker_meta, receiver));
        Self {
            commands,
            playback,
            meta,
        }
    }

    /// Opens the output device before the first play, in the background.
    pub fn prewarm(&self) {
        let _ = self.commands.send(Command::Open);
    }

    /// Replaces the buffer. The device opens if needed, but playback waits for
    /// [`Self::start_at`] so the first sample matches the frame on screen.
    pub fn play(&self, samples: Vec<f32>, origin_frame: i64, fps: f64) {
        let silent = !samples.iter().any(|sample| *sample != 0.0);
        {
            let mut meta = lock_meta(&self.meta);
            meta.origin_frame = origin_frame;
            meta.fps = fps;
            meta.silent = silent;
            meta.ready = false;
        }
        let _ = self.commands.send(Command::Play {
            samples,
            origin_frame,
            fps,
            silent,
        });
    }

    /// Keeps the device open but stops reading the current buffer.
    pub fn stop(&self) {
        {
            let mut meta = lock_meta(&self.meta);
            meta.silent = true;
            meta.ready = false;
        }
        let _ = self.commands.send(Command::Stop);
    }

    /// Starts the staged buffer at `frame`.
    ///
    /// Called once the device is open; if the open finished after playback
    /// began, the transport kept the wall clock in the meantime and the audio
    /// joins at the frame currently on screen instead of jumping back.
    pub fn start_at(&self, frame: i64) {
        {
            let meta = lock_meta(&self.meta);
            if meta.silent {
                return;
            }
        }
        let _ = self.commands.send(Command::Start(frame));
    }

    /// Whether the output device is open and can start a buffer.
    #[must_use]
    pub fn device_ready(&self) -> bool {
        lock_meta(&self.meta).device_ready
    }

    /// Seconds since the composition start according to the device cursor.
    ///
    /// `None` while the device opens or when the mix is silent: the transport
    /// uses the wall clock in both cases.
    #[must_use]
    pub fn position(&self) -> Option<f64> {
        let meta = lock_meta(&self.meta);
        if !meta.ready || meta.silent {
            return None;
        }
        let playback = lock(&self.playback);
        #[allow(clippy::cast_precision_loss)]
        let played = playback.cursor as f64 / f64::from(SAMPLE_RATE);
        #[allow(clippy::cast_precision_loss)]
        let origin = meta.origin_frame as f64 / meta.fps;
        Some(origin + played)
    }

    /// Whether the device consumed every mixed sample.
    ///
    /// A silent or not-yet-open stream never reports the end; the wall clock
    /// drives the transport until the device is ready.
    #[must_use]
    pub fn ended(&self) -> bool {
        let meta = lock_meta(&self.meta);
        !meta.silent && meta.ready && lock(&self.playback).ended()
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

/// The audio worker: owns the `cpal::Stream` and processes commands in order.
#[allow(clippy::needless_pass_by_value)] // the worker owns these for its thread
fn worker(
    playback: Arc<Mutex<Playback>>,
    meta: Arc<Mutex<Meta>>,
    commands: std::sync::mpsc::Receiver<Command>,
) {
    let mut stream: Option<cpal::Stream> = None;
    while let Ok(command) = commands.recv() {
        match command {
            Command::Open => {
                if stream.is_none() {
                    stream = open_stream(&playback);
                }
                lock_meta(&meta).device_ready = stream.is_some();
            }
            Command::Play {
                samples,
                origin_frame,
                fps,
                silent,
            } => {
                let frames = samples.len() / 2;
                {
                    let mut playback = lock(&playback);
                    playback.samples = samples;
                    playback.frames = frames;
                    playback.cursor = 0;
                }
                {
                    let mut meta = lock_meta(&meta);
                    meta.origin_frame = origin_frame;
                    meta.fps = fps;
                    meta.silent = silent;
                    meta.ready = false;
                }
                if !silent && stream.is_none() {
                    stream = open_stream(&playback);
                }
                lock_meta(&meta).device_ready = stream.is_some();
            }
            Command::Start(frame) => {
                let (origin_frame, fps) = {
                    let meta = lock_meta(&meta);
                    (meta.origin_frame, meta.fps)
                };
                if fps > 0.0 {
                    #[allow(
                        clippy::cast_precision_loss,
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss
                    )]
                    let offset = ((frame - origin_frame).max(0) as f64 * f64::from(SAMPLE_RATE)
                        / fps)
                        .round() as usize;
                    let mut playback = lock(&playback);
                    playback.cursor = offset.min(playback.frames);
                }
                let mut meta = lock_meta(&meta);
                meta.device_ready = stream.is_some();
                if stream.is_some() && !meta.silent {
                    meta.ready = true;
                }
            }
            Command::Stop => {
                {
                    let mut playback = lock(&playback);
                    playback.samples.clear();
                    playback.frames = 0;
                    playback.cursor = 0;
                }
                let mut meta = lock_meta(&meta);
                meta.silent = true;
                meta.ready = false;
            }
            Command::Shutdown => break,
        }
    }
    drop(stream);
}

/// Builds and starts the default output stream; `None` when no device can.
fn open_stream(playback: &Arc<Mutex<Playback>>) -> Option<cpal::Stream> {
    let _span = motioner_ui::startup::span("motioner: audio device (worker)");
    let stream = build_stream(Arc::clone(playback)).ok()?;
    stream.play().ok()?;
    Some(stream)
}

/// Mixes `from_frame` to the end (or a 60 ms grain) of the document.
///
/// Decoding and mixing are cheap (the cpal device open was the slow part);
/// they stay on the caller so the worker only owns the stream.
#[must_use]
pub fn mix_for(
    project: &Project,
    document: &Document,
    from_frame: i64,
    muted: &[String],
    grain: bool,
) -> Vec<f32> {
    let rate = document.composition().frame_rate();
    let numerator = u128::from(rate.numerator());
    let denominator = u128::from(rate.denominator());
    let samples_per_frame = u128::from(SAMPLE_RATE) * denominator;
    let from_sample = if from_frame <= 0 {
        0
    } else {
        u128::try_from(from_frame).unwrap_or(0) * samples_per_frame / numerator
    };
    let to = if grain {
        from_frame.saturating_add(1)
    } else {
        document.composition().duration()
    };
    let mut count =
        u128::try_from((to - from_frame).max(0)).unwrap_or(0) * samples_per_frame / numerator;
    if grain {
        count = (u128::from(SAMPLE_RATE) * 60) / 1000; // 60 ms grain
    }
    let count = usize::try_from(count).unwrap_or(0);
    let store = {
        let _span = motioner_ui::startup::span("motioner: audio decode");
        store_for(project, document)
    };
    let muted: HashSet<String> = muted.iter().cloned().collect();
    let _span = motioner_ui::startup::span("motioner: audio mix");
    mix_document(
        document,
        &store,
        u64::try_from(from_sample).unwrap_or(0),
        count,
        &muted,
    )
}

/// The player cursor the device callback advances.
struct Playback {
    samples: Vec<f32>,
    cursor: usize,
    frames: usize,
}

impl Playback {
    fn ended(&self) -> bool {
        self.cursor >= self.frames
    }

    fn fill(&mut self, data: &mut [f32], channels: usize) {
        for frame in data.chunks_mut(channels) {
            if self.cursor < self.frames {
                let base = self.cursor * 2;
                frame[0] = self.samples[base];
                if channels > 1 {
                    frame[1] = self.samples[base + 1];
                }
                for sample in frame.iter_mut().skip(2) {
                    *sample = 0.0;
                }
                self.cursor += 1;
            } else {
                frame.fill(0.0);
            }
        }
    }
}

fn build_stream(playback: Arc<Mutex<Playback>>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no default audio output device".to_owned())?;
    let mut ranges = device
        .supported_output_configs()
        .map_err(|error| format!("cannot query the audio device: {error}"))?;
    let range = ranges
        .find(|range| {
            range.sample_format() == SampleFormat::F32
                && range.channels() >= 2
                && range.min_sample_rate() <= SAMPLE_RATE
                && range.max_sample_rate() >= SAMPLE_RATE
        })
        .ok_or_else(|| "the audio device does not support 48 kHz f32 stereo".to_owned())?;
    let config = range.with_sample_rate(SAMPLE_RATE).config();
    let channels = usize::from(config.channels);
    // A small fixed period keeps the cursor regular: a device default can be
    // hundreds of milliseconds, which makes the transport jump several frames
    // per callback. Fall back to the default when the device refuses it.
    let mut fixed = config;
    fixed.buffer_size = cpal::BufferSize::Fixed(1024);
    let build = |config: cpal::StreamConfig, playback: Arc<Mutex<Playback>>| {
        device.build_output_stream(
            config,
            move |data: &mut [f32], _| lock(&playback).fill(data, channels),
            |error| eprintln!("audio output: {error}"),
            None,
        )
    };
    match build(fixed, Arc::clone(&playback)) {
        Ok(stream) => Ok(stream),
        Err(_) => build(config, playback)
            .map_err(|error| format!("cannot open the audio device: {error}")),
    }
}

fn lock(playback: &Arc<Mutex<Playback>>) -> std::sync::MutexGuard<'_, Playback> {
    match playback.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn lock_meta(meta: &Arc<Mutex<Meta>>) -> std::sync::MutexGuard<'_, Meta> {
    match meta.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Decodes every audio asset a document references and indexes it by registry
/// key and original name.
fn store_for(project: &Project, document: &Document) -> AudioStore {
    let mut store = AudioStore::default();
    let location = FfmpegLocation::FromEnvOrPath;
    for node in document.nodes() {
        let NodeContent::Audio(audio) = node.content() else {
            continue;
        };
        let Some(entry) = project.find_asset(&audio.source) else {
            continue;
        };
        if entry.kind != AssetKind::Audio {
            continue;
        }
        let Ok(pcm) = decode_to_cache(
            &project.assets_dir().join(entry.file_name()),
            &project.cache_dir(),
            &entry.hash,
            &location,
        ) else {
            continue;
        };
        store.insert(entry.key.clone(), pcm.clone());
        store.insert(entry.original.clone(), pcm);
    }
    store
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_reports_the_end_only_after_the_last_frame() {
        let mut playback = Playback {
            samples: vec![0.25, -0.25, 0.5, -0.5],
            cursor: 0,
            frames: 2,
        };
        assert!(!playback.ended());
        let mut data = [0.0_f32; 2];
        playback.fill(&mut data, 2);
        assert!(!playback.ended());
        playback.fill(&mut data, 2);
        assert!(playback.ended());
        // Extra callbacks are silent and never move the cursor past the end.
        playback.fill(&mut data, 2);
        assert_eq!(playback.cursor, 2);
        assert!(data.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn the_shipped_demo_mix_is_not_silent() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo.motioner");
        let Ok(project) = Project::open(&root) else {
            return;
        };
        let composition = project.composition_path(&project.settings().main);
        let Ok(source) = std::fs::read_to_string(&composition) else {
            return;
        };
        let assets = motioner_io::ProjectAssets::new(project.root().to_path_buf());
        let metrics = motioner_core::NoTextMetrics;
        let Ok(document) = motioner_dsl::run_with_assets(
            &source,
            &motioner_dsl::Limits::default(),
            &metrics,
            &assets,
        ) else {
            return;
        };
        let samples = mix_for(&project, &document, 0, &[], false);
        assert!(!samples.is_empty(), "the demo has an audio track");
        assert!(
            samples.iter().any(|sample| *sample != 0.0),
            "the demo mix must not be silent"
        );
    }
}
