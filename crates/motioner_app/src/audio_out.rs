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
pub struct AudioOutput {
    stream: Option<cpal::Stream>,
    playback: Arc<Mutex<Playback>>,
    origin_seconds: f64,
    /// The document mixes to silence, so the wall clock drives the transport.
    silent: bool,
}

impl AudioOutput {
    /// Mixes the document and starts playing from `from_frame`.
    ///
    /// # Errors
    ///
    /// A human-readable message when no output device supports 48 kHz f32
    /// stereo or the stream cannot be built.
    pub fn start(
        project: &Project,
        document: &Document,
        from_frame: i64,
        muted: &[String],
        grain: bool,
    ) -> Result<Self, String> {
        let rate = document.composition().frame_rate();
        #[allow(clippy::cast_precision_loss)]
        let fps = rate.as_f64();
        #[allow(clippy::cast_precision_loss)]
        let origin_seconds = from_frame as f64 / fps;
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
        let count = usize::try_from(count).map_err(|_| "the audio range is too long")?;
        let store = store_for(project, document);
        let muted: HashSet<String> = muted.iter().cloned().collect();
        let samples = mix_document(
            document,
            &store,
            u64::try_from(from_sample).unwrap_or(0),
            count,
            &muted,
        );
        let silent = !samples.iter().any(|sample| *sample != 0.0);
        let frames = count;
        let playback = Arc::new(Mutex::new(Playback {
            samples,
            cursor: 0,
            frames,
        }));
        let stream = if silent {
            None
        } else {
            let stream = build_stream(Arc::clone(&playback))?;
            stream.play().map_err(|error| error.to_string())?;
            Some(stream)
        };
        Ok(Self {
            stream,
            playback,
            origin_seconds,
            silent,
        })
    }

    /// Seconds since the composition start according to the device cursor.
    ///
    /// `None` when the mix is silent: there is no audio to stay in sync with,
    /// so the transport uses the wall clock instead of a device cursor that
    /// may not be running.
    #[must_use]
    pub fn position(&self) -> Option<f64> {
        if self.silent {
            return None;
        }
        let playback = lock(&self.playback);
        #[allow(clippy::cast_precision_loss)]
        let played = playback.cursor as f64 / f64::from(SAMPLE_RATE);
        Some(self.origin_seconds + played)
    }

    /// Whether the device consumed every mixed sample.
    ///
    /// The transport uses this to wrap a looping transport and restart the
    /// stream at the loop start; a silent mix never reports the end because
    /// the wall clock drives it.
    #[must_use]
    pub fn ended(&self) -> bool {
        if self.silent {
            return false;
        }
        lock(&self.playback).ended()
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.stream.take();
    }
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
}
