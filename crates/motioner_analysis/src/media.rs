//! Media analysis: palette, salience, empty regions, cuts, motion, tempo and
//! beats (D-0043 phase 9).
//!
//! Every algorithm is written here in a few dozen lines instead of pulling a
//! crate: deterministic seeding, fixed iteration counts, stable ordering.
//! Decoding (PNG, video, audio) stays in `motioner_io`; these functions take
//! pixels and samples.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)] // pixel/sample index math: values are bounded by image and buffer sizes

use motioner_core::Color;

use crate::geom::Box2;

/// Straight-alpha sRGB pixels, row 0 at the top.
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba8: Vec<u8>,
}

impl RgbaImage {
    /// Whether the buffer matches the dimensions.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.rgba8.len() == self.width as usize * self.height as usize * 4
    }

    fn pixel(&self, x: u32, y: u32) -> Color {
        let index = (y as usize * self.width as usize + x as usize) * 4;
        Color::rgb(
            self.rgba8[index],
            self.rgba8[index + 1],
            self.rgba8[index + 2],
        )
    }
}

/// One colour of the dominant palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteColor {
    /// The colour.
    pub color: Color,
    /// Fraction of sampled pixels in the cluster, `0.0..=1.0`.
    pub fraction: f64,
}

/// A region recommended for text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmptyRegion {
    /// Region in image pixels.
    pub rect: Box2,
    /// Average relative luminance, `0.0..=1.0`.
    pub luminance: f64,
    /// How busy the region is (`0.0` flat, `1.0` noisy). Lower is better.
    pub busyness: f64,
    /// Ranking score, higher is better.
    pub score: f64,
}

/// The image analysis result.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAnalysis {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Dominant colours, most frequent first.
    pub palette: Vec<PaletteColor>,
    /// The most salient region (highest edge and contrast energy).
    pub salient: Option<Box2>,
    /// Regions ranked for text placement, best first.
    pub empty_regions: Vec<EmptyRegion>,
    /// Average luminance of a 6 × 4 grid, row-major.
    pub luminance_grid: Vec<f64>,
}

/// Knobs of the media algorithms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MediaOptions {
    /// Palette clusters.
    pub palette_size: usize,
    /// k-means iterations.
    pub kmeans_iterations: usize,
    /// Empty-region candidates kept.
    pub empty_regions: usize,
}

impl Default for MediaOptions {
    fn default() -> Self {
        Self {
            palette_size: 5,
            kmeans_iterations: 10,
            empty_regions: 3,
        }
    }
}

fn srgb_to_linear(value: u8) -> f64 {
    let value = f64::from(value) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f64) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let srgb = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = (srgb * 255.0).round().clamp(0.0, 255.0) as u8;
    byte
}

fn linear(color: Color) -> [f64; 3] {
    [
        srgb_to_linear(color.r),
        srgb_to_linear(color.g),
        srgb_to_linear(color.b),
    ]
}

fn from_linear(values: [f64; 3]) -> Color {
    Color::rgb(
        linear_to_srgb(values[0]),
        linear_to_srgb(values[1]),
        linear_to_srgb(values[2]),
    )
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn sample_pixels(image: &RgbaImage, max_side: u32) -> Vec<[f64; 3]> {
    let step = (image.width.max(image.height) / max_side).max(1);
    let mut pixels = Vec::new();
    let mut y = 0;
    while y < image.height {
        let mut x = 0;
        while x < image.width {
            pixels.push(linear(image.pixel(x, y)));
            x += step;
        }
        y += step;
    }
    pixels
}

/// Deterministic k-means over perceptual (linear-light) colour.
fn kmeans(pixels: &[[f64; 3]], count: usize, iterations: usize) -> Vec<([f64; 3], usize)> {
    if pixels.is_empty() || count == 0 {
        return Vec::new();
    }
    // Seed: evenly spaced pixels; deterministic for a given image.
    let mut centers: Vec<[f64; 3]> = (0..count)
        .map(|index| pixels[index * pixels.len() / count.max(1)])
        .collect();
    let mut counts = vec![0_usize; count];
    for _ in 0..iterations {
        let mut sums = vec![[0.0_f64; 3]; count];
        counts = vec![0_usize; count];
        for pixel in pixels {
            let mut best = 0_usize;
            let mut best_distance = f64::INFINITY;
            for (index, center) in centers.iter().enumerate() {
                let candidate = distance(*pixel, *center);
                if candidate < best_distance {
                    best_distance = candidate;
                    best = index;
                }
            }
            sums[best][0] += pixel[0];
            sums[best][1] += pixel[1];
            sums[best][2] += pixel[2];
            counts[best] += 1;
        }
        for (index, center) in centers.iter_mut().enumerate() {
            if counts[index] > 0 {
                #[allow(clippy::cast_precision_loss)]
                let count = counts[index] as f64;
                *center = [
                    sums[index][0] / count,
                    sums[index][1] / count,
                    sums[index][2] / count,
                ];
            }
        }
    }
    centers.into_iter().zip(counts).collect()
}

fn luminance(color: Color) -> f64 {
    let [r, g, b] = linear(color);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// A coarse busyness/luminance map over a `cols × rows` grid.
fn cell_stats(image: &RgbaImage, cols: u32, rows: u32) -> Vec<(f64, f64)> {
    let mut stats = Vec::with_capacity((cols * rows) as usize);
    for row in 0..rows {
        for col in 0..cols {
            let x0 = col * image.width / cols;
            let x1 = ((col + 1) * image.width / cols)
                .max(x0 + 1)
                .min(image.width);
            let y0 = row * image.height / rows;
            let y1 = ((row + 1) * image.height / rows)
                .max(y0 + 1)
                .min(image.height);
            let mut total = 0.0_f64;
            let mut count = 0.0_f64;
            let mut energy = 0.0;
            let mut previous = luminance(image.pixel(x0, y0));
            for y in y0..y1 {
                for x in x0..x1 {
                    let value = luminance(image.pixel(x, y));
                    total += value;
                    count += 1.0;
                    energy += (value - previous).abs();
                    previous = value;
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let area = (x1 - x0) as f64 * (y1 - y0) as f64;
            let mean = total / count.max(1.0);
            stats.push((mean, energy / area.max(1.0)));
        }
    }
    stats
}

/// Analyses an image: palette, salience, empty regions and luminance.
#[must_use]
pub fn analyze_image(image: &RgbaImage, options: &MediaOptions) -> ImageAnalysis {
    let width = image.width;
    let height = image.height;
    if !image.is_valid() {
        return ImageAnalysis {
            width,
            height,
            palette: Vec::new(),
            salient: None,
            empty_regions: Vec::new(),
            luminance_grid: Vec::new(),
        };
    }
    let pixels = sample_pixels(image, 96);
    let clusters = kmeans(&pixels, options.palette_size, options.kmeans_iterations);
    let mut palette: Vec<PaletteColor> = clusters
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(center, count)| PaletteColor {
            color: from_linear(center),
            #[allow(clippy::cast_precision_loss)]
            fraction: count as f64 / pixels.len().max(1) as f64,
        })
        .collect();
    palette.sort_by(|a, b| b.fraction.total_cmp(&a.fraction));

    let cols = 6_u32;
    let rows = 4_u32;
    let stats = cell_stats(image, cols, rows);
    let luminance_grid: Vec<f64> = stats.iter().map(|(mean, _)| *mean).collect();

    // Salience: the cell with the most edge energy, preferring brighter areas
    // when two cells tie.
    let salient = stats
        .iter()
        .enumerate()
        .max_by(|(_, (a_luma, a_energy)), (_, (b_luma, b_energy))| {
            (a_energy + a_luma * 0.05).total_cmp(&(b_energy + b_luma * 0.05))
        })
        .map(|(index, _)| {
            let col = (index as u32) % cols;
            let row = (index as u32) / cols;
            Box2::new(
                (col * width / cols) as f64,
                (row * height / rows) as f64,
                ((col + 1) * width / cols) as f64,
                ((row + 1) * height / rows) as f64,
            )
            .unwrap_or(Box2 {
                min_x: 0.0,
                min_y: 0.0,
                max_x: f64::from(width),
                max_y: f64::from(height),
            })
        });

    let mut regions: Vec<EmptyRegion> = Vec::new();
    for candidate_cols in [2_u32, 3] {
        let candidate_rows = if candidate_cols == 2 { 3 } else { 2 };
        let cells = cell_stats(image, candidate_cols, candidate_rows);
        for (index, (mean, energy)) in cells.iter().enumerate() {
            let col = (index as u32) % candidate_cols;
            let row = (index as u32) / candidate_cols;
            let rect = Box2::new(
                (col * width / candidate_cols) as f64,
                (row * height / candidate_rows) as f64,
                ((col + 1) * width / candidate_cols) as f64,
                ((row + 1) * height / candidate_rows) as f64,
            );
            let Some(rect) = rect else {
                continue;
            };
            let busyness = (energy * 40.0).clamp(0.0, 1.0);
            // Middle luminance is the easiest to place text on; very dark and
            // very bright regions are both fine, extremes are not.
            let mid = 1.0 - (mean - 0.5).abs() * 1.2;
            let score = (1.0 - busyness) * 0.75 + mid.clamp(0.0, 1.0) * 0.25;
            regions.push(EmptyRegion {
                rect,
                luminance: *mean,
                busyness,
                score,
            });
        }
    }
    regions.sort_by(|a, b| b.score.total_cmp(&a.score));
    regions.truncate(options.empty_regions.max(1));

    ImageAnalysis {
        width,
        height,
        palette,
        salient,
        empty_regions: regions,
        luminance_grid,
    }
}

/// One decoded greyscale frame for video analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct GrayFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Luma, `width * height` bytes.
    pub luma: Vec<u8>,
}

/// The video analysis result.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoAnalysis {
    /// Frame indices where a cut happens (the first frame of the new shot).
    pub cuts: Vec<i64>,
    /// Motion intensity per frame, `0.0..=1.0`.
    pub motion: Vec<f64>,
    /// Calm runs `[start, end]` in frames with little motion.
    pub calm_segments: Vec<(i64, i64)>,
}

fn histogram(frame: &GrayFrame) -> [f64; 32] {
    let mut bins = [0.0_f64; 32];
    for value in &frame.luma {
        let bin = usize::from(*value) * 32 / 256;
        bins[bin.min(31)] += 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let total = frame.luma.len().max(1) as f64;
    for bin in &mut bins {
        *bin /= total;
    }
    bins
}

fn histogram_distance(a: &[f64; 32], b: &[f64; 32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(left, right)| (left - right).abs())
        .sum::<f64>()
        * 0.5
}

fn mean_absolute_difference(a: &GrayFrame, b: &GrayFrame) -> f64 {
    if a.luma.len() != b.luma.len() || a.luma.is_empty() {
        return 0.0;
    }
    let total: u64 = a
        .luma
        .iter()
        .zip(&b.luma)
        .map(|(left, right)| u64::from(left.abs_diff(*right)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let value = total as f64 / (a.luma.len() as f64 * 255.0);
    value
}

/// Detects cuts, motion and calm segments from decoded low-res frames.
#[must_use]
pub fn analyze_video(frames: &[GrayFrame], cut_threshold: f64) -> VideoAnalysis {
    let mut cuts = Vec::new();
    let mut motion = Vec::with_capacity(frames.len());
    if frames.is_empty() {
        return VideoAnalysis {
            cuts,
            motion,
            calm_segments: Vec::new(),
        };
    }
    motion.push(0.0);
    for index in 1..frames.len() {
        let distance =
            histogram_distance(&histogram(&frames[index - 1]), &histogram(&frames[index]));
        if distance > cut_threshold {
            cuts.push(i64::try_from(index).unwrap_or(i64::MAX));
        }
        let difference = mean_absolute_difference(&frames[index - 1], &frames[index]);
        motion.push((difference * 8.0).clamp(0.0, 1.0));
    }
    let mut calm_segments = Vec::new();
    let calm_threshold = 0.02;
    let mut run_start: Option<usize> = None;
    for (index, value) in motion.iter().enumerate() {
        if *value < calm_threshold {
            run_start.get_or_insert(index);
        } else if let Some(start) = run_start.take() {
            if index - start >= 15 {
                calm_segments.push((
                    i64::try_from(start).unwrap_or(0),
                    i64::try_from(index.saturating_sub(1)).unwrap_or(0),
                ));
            }
        }
    }
    if let Some(start) = run_start {
        if motion.len() - start >= 15 {
            calm_segments.push((
                i64::try_from(start).unwrap_or(0),
                i64::try_from(motion.len().saturating_sub(1)).unwrap_or(0),
            ));
        }
    }
    VideoAnalysis {
        cuts,
        motion,
        calm_segments,
    }
}

/// The audio analysis result.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioAnalysis {
    /// Duration in seconds.
    pub duration: f64,
    /// Detected tempo, when the signal has a clear period.
    pub bpm: Option<f64>,
    /// Beat times in seconds.
    pub beats: Vec<f64>,
    /// Downbeat times (every fourth beat from the first strong beat).
    pub downbeats: Vec<f64>,
    /// Section boundaries as `[start, end]` in seconds.
    pub sections: Vec<(f64, f64)>,
    /// Loudness curve, one RMS value in `0.0..=1.0` per analysis window.
    pub loudness: Vec<f64>,
    /// Onset strength per window, `0.0..=1.0`.
    pub onsets: Vec<f64>,
}

const HOP: usize = 512;
const WINDOW: usize = 1024;

fn onset_envelope(samples: &[f32]) -> (Vec<f64>, Vec<f64>) {
    let mut energy = Vec::new();
    let mut loudness = Vec::new();
    let mut index = 0;
    while index + WINDOW <= samples.len() {
        let mut total = 0.0_f64;
        for sample in &samples[index..index + WINDOW] {
            total += f64::from(*sample) * f64::from(*sample);
        }
        #[allow(clippy::cast_precision_loss)]
        let rms = (total / WINDOW as f64).sqrt();
        energy.push(rms);
        loudness.push(rms.clamp(0.0, 1.0));
        index += HOP;
    }
    let mut onsets = Vec::with_capacity(energy.len());
    // The first window can itself contain the first onset.
    onsets.push(energy.first().copied().unwrap_or(0.0));
    for pair in energy.windows(2) {
        onsets.push((pair[1] - pair[0]).max(0.0));
    }
    let peak = onsets.iter().copied().fold(0.0_f64, f64::max).max(1e-9);
    for value in &mut onsets {
        *value /= peak;
    }
    (onsets, loudness)
}

fn tempo_from_onsets(onsets: &[f64], sample_rate: u32) -> Option<f64> {
    if onsets.len() < 8 || sample_rate == 0 {
        return None;
    }
    let envelope_fps = f64::from(sample_rate) / HOP as f64;
    let min_lag = (envelope_fps * 60.0 / 200.0).round().max(1.0) as usize;
    let max_lag = (envelope_fps * 60.0 / 60.0).round() as usize;
    if max_lag <= min_lag || max_lag >= onsets.len() {
        return None;
    }
    let mean = onsets.iter().sum::<f64>() / onsets.len() as f64;
    let mut best = (0.0_f64, min_lag);
    for lag in min_lag..=max_lag {
        let mut score = 0.0;
        let mut count = 0.0;
        for index in lag..onsets.len() {
            score += (onsets[index] - mean) * (onsets[index - lag] - mean);
            count += 1.0;
        }
        let score = if count > 0.0 { score / count } else { 0.0 };
        if score > best.0 * 1.05 {
            best = (score, lag);
        }
    }
    if best.0 <= 0.0 {
        return None;
    }
    let bpm = 60.0 * envelope_fps / best.1 as f64;
    Some(bpm)
}

fn pick_beats(onsets: &[f64], sample_rate: u32) -> Vec<f64> {
    if onsets.len() < 3 || sample_rate == 0 {
        return Vec::new();
    }
    let hop_seconds = HOP as f64 / f64::from(sample_rate);
    let mean = onsets.iter().sum::<f64>() / onsets.len() as f64;
    let variance = onsets
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / onsets.len() as f64;
    let threshold = mean + variance.sqrt() * 0.25;
    let mut beats = Vec::new();
    for index in 0..onsets.len() {
        let left = if index == 0 { 0.0 } else { onsets[index - 1] };
        let right = onsets.get(index + 1).copied().unwrap_or(0.0);
        if onsets[index] > threshold && onsets[index] >= left && onsets[index] > right {
            #[allow(clippy::cast_precision_loss)]
            let time = index as f64 * hop_seconds;
            let close = beats.last().is_some_and(|last: &f64| time - *last < 0.25);
            if !close {
                beats.push(time);
            }
        }
    }
    beats
}

/// Beat times on a regular grid anchored at the first detected beat.
fn tempo_grid(first: f64, bpm: f64, duration: f64) -> Vec<f64> {
    let period = 60.0 / bpm;
    if !period.is_finite() || period <= 0.0 {
        return Vec::new();
    }
    let mut beats = Vec::new();
    let mut time = first.max(0.0);
    // Do not invent a lead-in before the first detected beat, and stop at the
    // end of the signal.
    while time <= duration + 1e-6 {
        beats.push(time);
        time += period;
    }
    beats
}

/// Least-squares fit of the beat times: `(anchor, period)` when the beats are
/// regular.
///
/// Regressing over every detected beat averages out the analysis-hop
/// quantisation, which a median interval cannot do: at 90 bpm a 11.6 ms error
/// in one interval becomes a 21 ms drift five beats later.
fn fit_tempo(beats: &[f64]) -> Option<(f64, f64)> {
    if beats.len() < 4 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let count = beats.len() as f64;
    #[allow(clippy::cast_precision_loss)]
    let mean_x = (0..beats.len()).map(|index| index as f64).sum::<f64>() / count;
    let mean_y = beats.iter().sum::<f64>() / count;
    let mut covariance = 0.0;
    let mut variance = 0.0;
    for (index, beat) in beats.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = index as f64 - mean_x;
        covariance += x * (beat - mean_y);
        variance += x * x;
    }
    if variance <= 0.0 {
        return None;
    }
    let period = covariance / variance;
    if !(0.25..=1.5).contains(&period) {
        return None;
    }
    let anchor = mean_y - period * mean_x;
    let residual = beats
        .iter()
        .enumerate()
        .map(|(index, beat)| {
            #[allow(clippy::cast_precision_loss)]
            let expected = anchor + period * index as f64;
            (beat - expected).abs()
        })
        .fold(0.0_f64, f64::max);
    if residual > 0.030 {
        return None;
    }
    Some((anchor, period))
}

/// Analyses a mono audio signal (interleaved stereo is averaged by the caller).
#[must_use]
pub fn analyze_audio(samples: &[f32], sample_rate: u32) -> AudioAnalysis {
    #[allow(clippy::cast_precision_loss)]
    let duration = samples.len() as f64 / f64::from(sample_rate.max(1));
    let (onsets, loudness) = onset_envelope(samples);
    let autocorrelation_bpm = tempo_from_onsets(&onsets, sample_rate);
    let detected = pick_beats(&onsets, sample_rate);
    // Beat intervals are exact for a click track and much finer than the
    // autocorrelation lag, which is quantised to the analysis hop.
    let fitted = fit_tempo(&detected);
    let bpm = fitted
        .map(|(_, period)| 60.0 / period)
        .or(autocorrelation_bpm);
    // A regular tempo is reported as the grid it defines: the detection hop
    // (11.6 ms at 44.1 kHz) is coarser than the 20 ms beat tolerance when a
    // click straddles two windows.
    let beats = match fitted {
        Some((anchor, period)) => tempo_grid(anchor, 60.0 / period, duration),
        None => detected,
    };
    let downbeats = beats.iter().step_by(4).copied().collect();
    let mut sections = Vec::new();
    if !beats.is_empty() {
        let mut start = 0.0;
        for beat in beats.iter().step_by(8).skip(1) {
            if *beat - start >= 2.0 {
                sections.push((start, *beat));
                start = *beat;
            }
        }
        sections.push((start, duration));
    } else if duration > 0.0 {
        sections.push((0.0, duration));
    }
    AudioAnalysis {
        duration,
        bpm,
        beats,
        downbeats,
        sections,
        loudness,
        onsets,
    }
}
