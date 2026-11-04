//! Deterministic audio decode, mixing and WAV export (D-0048).
//!
//! ffmpeg decodes each asset once to 48 kHz stereo f32 PCM under `cache/`
//! (keyed by the asset hash). [`mix_document`] is pure: it turns a document,
//! a sample store and a sample range into interleaved stereo samples, so the
//! preview and the MP4 export share exactly the same mix.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::path::Path;
use std::sync::Arc;

use motioner_core::{Document, NodeContent};

use crate::encode::{resolve_program, FfmpegLocation};
use crate::error::ExportError;

/// Output sample rate of every decoded clip and of the mix.
pub const SAMPLE_RATE: u32 = 48_000;
/// Interleaved channel count.
pub const CHANNELS: usize = 2;

/// Decoded stereo PCM at [`SAMPLE_RATE`].
#[derive(Debug, Clone, PartialEq)]
pub struct AudioPcm {
    /// Interleaved stereo samples.
    pub samples: Vec<f32>,
    /// Sample rate; always [`SAMPLE_RATE`] for decoded assets.
    pub sample_rate: u32,
}

impl AudioPcm {
    /// Length in seconds.
    #[must_use]
    pub fn duration(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let frames = (self.samples.len() / CHANNELS) as f64;
        frames / f64::from(self.sample_rate)
    }

    /// Number of stereo frames.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / CHANNELS
    }
}

/// Decoded clips indexed by registry key and by original name.
#[derive(Debug, Clone, Default)]
pub struct AudioStore {
    clips: HashMap<String, Arc<AudioPcm>>,
}

impl AudioStore {
    /// Adds `pcm` under every key.
    pub fn insert(&mut self, key: impl Into<String>, pcm: Arc<AudioPcm>) {
        self.clips.insert(key.into(), pcm);
    }

    /// The clip for a document source, when the store has it.
    #[must_use]
    pub fn get(&self, source: &str) -> Option<&Arc<AudioPcm>> {
        self.clips.get(source)
    }

    /// Whether the store has no clips.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}

/// Decodes `source` to 48 kHz stereo f32, caching the raw PCM as
/// `cache/<hash>.pcm`.
///
/// # Errors
///
/// [`ExportError::FfmpegNotFound`], [`ExportError::Ffmpeg`] or
/// [`ExportError::Io`] when decoding or caching fails.
pub fn decode_to_cache(
    source: &Path,
    cache_dir: &Path,
    hash: &str,
    location: &FfmpegLocation,
) -> Result<Arc<AudioPcm>, ExportError> {
    let cache = cache_dir.join(format!("{hash}.pcm"));
    if let Ok(bytes) = std::fs::read(&cache) {
        if !bytes.is_empty() && bytes.len().is_multiple_of(CHANNELS * 4) {
            return Ok(Arc::new(pcm_from_bytes(&bytes)));
        }
    }
    let program = resolve_program(location);
    let output = std::process::Command::new(&program)
        .args([
            "-v",
            "error",
            "-i",
            &source.to_string_lossy(),
            "-ac",
            "2",
            "-ar",
            &SAMPLE_RATE.to_string(),
            "-f",
            "f32le",
            "-",
        ])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ExportError::FfmpegNotFound {
                    message: format!(
                        "{} was not found; install ffmpeg or set MOTIONER_FFMPEG",
                        program.to_string_lossy()
                    ),
                }
            } else {
                ExportError::Ffmpeg {
                    message: format!("cannot run {}: {error}", program.to_string_lossy()),
                }
            }
        })?;
    if !output.status.success() {
        return Err(ExportError::Ffmpeg {
            message: String::from_utf8_lossy(&output.stderr)
                .lines()
                .next_back()
                .unwrap_or("ffmpeg failed to decode the audio")
                .to_owned(),
        });
    }
    if output.stdout.len() < CHANNELS * 4 {
        return Err(ExportError::Ffmpeg {
            message: "the audio stream decoded to no samples".to_owned(),
        });
    }
    std::fs::create_dir_all(cache_dir).map_err(|error| ExportError::Io {
        message: format!("cannot create {}: {error}", cache_dir.display()),
    })?;
    crate::project::write_atomic_bytes(&cache, &output.stdout).map_err(|error| {
        ExportError::Io {
            message: format!("cannot cache {}: {error}", cache.display()),
        }
    })?;
    Ok(Arc::new(pcm_from_bytes(&output.stdout)))
}

fn pcm_from_bytes(bytes: &[u8]) -> AudioPcm {
    let samples = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    AudioPcm {
        samples,
        sample_rate: SAMPLE_RATE,
    }
}

/// Mixes the audio layers of `document` for `count` stereo frames starting at
/// `from_sample` at [`SAMPLE_RATE`].
///
/// The result is pure and sample-exact: the same document, store and range
/// always produce the same samples. Hidden layers and identities in `muted`
/// are silent (the editor's mute and solo state).
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // sample-accurate audio math
pub fn mix_document<S: BuildHasher>(
    document: &Document,
    store: &AudioStore,
    from_sample: u64,
    count: usize,
    muted: &HashSet<String, S>,
) -> Vec<f32> {
    let mut out = vec![0.0_f32; count * CHANNELS];
    let rate = document.composition().frame_rate().as_f64();
    if rate <= 0.0 {
        return out;
    }
    for node in document.nodes() {
        let NodeContent::Audio(audio) = node.content() else {
            continue;
        };
        if !node.is_visible() || muted.contains(node.identity().as_str()) {
            continue;
        }
        let Some(pcm) = store.get(&audio.source) else {
            continue;
        };
        let start_seconds = audio.start as f64 / rate;
        let in_point = motioner_eval::sample_at(&audio.in_point, 0.0);
        let out_point = motioner_eval::sample_at(&audio.out_point, 0.0);
        let clip_end = if out_point > in_point {
            start_seconds + (out_point - in_point)
        } else {
            start_seconds + (pcm.duration() - in_point).max(0.0)
        };
        let fade_in = audio.fade_in.max(0.0);
        let fade_out = audio.fade_out.max(0.0);
        let pcm_frames = pcm.frames();
        #[allow(clippy::cast_precision_loss)]
        for index in 0..count {
            let sample = from_sample + index as u64;
            #[allow(clippy::cast_precision_loss)]
            let time = sample as f64 / f64::from(SAMPLE_RATE);
            if time < start_seconds || time >= clip_end {
                continue;
            }
            let local = time - start_seconds + in_point;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let frame = (local * f64::from(pcm.sample_rate)).floor() as usize;
            if frame >= pcm_frames {
                continue;
            }
            let mut fade = 1.0_f64;
            if fade_in > 0.0 {
                fade = fade.min((time - start_seconds) / fade_in);
            }
            if fade_out > 0.0 {
                fade = fade.min((clip_end - time) / fade_out);
            }
            fade = fade.clamp(0.0, 1.0);
            let composition_frame = time * rate;
            let gain = motioner_eval::sample_at(&audio.gain, composition_frame).max(0.0);
            let pan = motioner_eval::sample_at(&audio.pan, composition_frame).clamp(-1.0, 1.0);
            let angle = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
            let left = angle.cos();
            let right = angle.sin();
            let amplitude = (gain * fade) as f32;
            let base = frame * CHANNELS;
            out[index * CHANNELS] += pcm.samples[base] * amplitude * left as f32;
            out[index * CHANNELS + 1] += pcm.samples[base + 1] * amplitude * right as f32;
        }
    }
    out
}

/// Writes interleaved stereo `f32` samples as a 16-bit PCM WAV file.
///
/// # Errors
///
/// [`ExportError::Io`] when the destination cannot be written.
#[allow(clippy::cast_possible_truncation)] // clamped samples; two channels
pub fn write_wav_i16(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), ExportError> {
    let channels = CHANNELS as u16;
    let bits = 16_u16;
    let block_align = channels * bits / 8;
    #[allow(clippy::cast_possible_truncation)]
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * u32::from(block_align)).to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    crate::project::write_atomic_bytes(path, &bytes).map_err(|error| ExportError::Io {
        message: format!("cannot write {}: {error}", path.display()),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use motioner_core::{Animatable, AudioContent, Composition, Document, NodeContent};

    use super::*;

    fn document(fps: u32, duration: i64) -> Document {
        let composition = Composition::new(
            "Mix",
            100,
            100,
            motioner_core::FrameRate::new(fps, 1).expect("frame rate"),
            duration,
            motioner_core::Color::BLACK,
        )
        .expect("composition");
        Document::new(composition)
    }

    fn tone(store: &mut AudioStore, key: &str) {
        let samples = (0..SAMPLE_RATE as usize)
            .flat_map(|index| {
                #[allow(clippy::cast_precision_loss)]
                let value = (index as f32 / 64.0).sin() * 0.5;
                [value, -value]
            })
            .collect();
        store.insert(
            key,
            Arc::new(AudioPcm {
                samples,
                sample_rate: SAMPLE_RATE,
            }),
        );
    }

    #[test]
    fn the_mixer_is_sample_exact_and_pans_with_equal_power() {
        let mut document = document(60, 120);
        let mut audio = AudioContent::new("tone.wav", 0);
        audio.gain = Animatable::new(1.0);
        audio.pan = Animatable::new(-1.0);
        document
            .add_node(NodeContent::Audio(audio))
            .expect("audio node");
        let mut store = AudioStore::default();
        tone(&mut store, "tone.wav");
        let first = mix_document(&document, &store, 0, 8, &HashSet::new());
        let second = mix_document(&document, &store, 0, 8, &HashSet::new());
        assert_eq!(first, second, "the mixer is deterministic");
        // Hard left: the right channel is silent and the left is the source.
        assert!(first[3].abs() < 1e-6, "right {}", first[3]);
        assert!(first[2].abs() > 0.0);
        // A second of samples is exactly 48 000 stereo frames.
        let all = mix_document(&document, &store, 0, SAMPLE_RATE as usize, &HashSet::new());
        assert_eq!(all.len(), SAMPLE_RATE as usize * CHANNELS);
    }

    #[test]
    fn hidden_layers_and_the_muted_set_are_silent() {
        let mut document = document(60, 120);
        let audio = AudioContent::new("tone.wav", 0);
        let node = document
            .add_node(NodeContent::Audio(audio))
            .expect("audio node");
        let mut store = AudioStore::default();
        tone(&mut store, "tone.wav");
        let playing = mix_document(&document, &store, 0, 16, &HashSet::new());
        let identity = document
            .node(node)
            .expect("node")
            .identity()
            .as_str()
            .to_owned();
        let muted = mix_document(&document, &store, 0, 16, &HashSet::from([identity]));
        assert!(muted.iter().all(|sample| *sample == 0.0));
        assert!(playing.iter().any(|sample| *sample != 0.0));
        document.set_visible(node, false).expect("visible");
        assert!(mix_document(&document, &store, 0, 16, &HashSet::new())
            .iter()
            .all(|sample| *sample == 0.0));
    }
}
