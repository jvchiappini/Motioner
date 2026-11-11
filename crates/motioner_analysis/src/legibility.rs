//! `legibility_report`: reading time and contrast against the real background.
//!
//! Reading time comes from the evaluated scene: how long a text stays visible
//! (opacity above the threshold and a box on screen). Contrast needs pixels,
//! so the caller supplies a [`BackgroundSample`] built from a render **without
//! the text**; that is the only way to know what is really behind the glyphs.

use std::collections::BTreeMap;

use motioner_core::{Color, Document, NodeId};
use motioner_eval::{Evaluator, RenderPlan};

use crate::element::{frame_elements, ElementKind};
use crate::geom::Box2;
use crate::rules::Rules;
use crate::text::TextMeasure;

/// One reading target: same centre and size.
type TextGroup = (i64, i64, f64, Vec<(NodeId, Seen)>);

/// Samples the colour behind a rectangle from a render without the text.
pub trait BackgroundSample {
    /// The average sRGB colour behind `rect` at `frame`.
    ///
    /// Returns `None` when the frame or region is not available, which the
    /// report renders as "contrast not measured" rather than a failure.
    fn behind(&self, frame: i64, rect: Box2) -> Option<Color>;
}

/// WCAG 2.1 relative luminance of an sRGB colour.
#[must_use]
pub fn relative_luminance(color: Color) -> f64 {
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
}

/// WCAG contrast ratio between two sRGB colours (`1.0..=21.0`).
#[must_use]
pub fn contrast_ratio(a: Color, b: Color) -> f64 {
    let first = relative_luminance(a);
    let second = relative_luminance(b);
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// One text's legibility.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLegibility {
    /// Node id.
    pub id: NodeId,
    /// Stable identity.
    pub identity: String,
    /// The text.
    pub text: String,
    /// Word count.
    pub words: usize,
    /// Em size in pixels.
    pub size: f64,
    /// Weight.
    pub weight: u16,
    /// Frames the text is visible on (sampled).
    pub visible_frames: i64,
    /// Visible time in seconds.
    pub visible_seconds: f64,
    /// Reading time the text needs, in seconds.
    pub required_seconds: f64,
    /// Contrast against the real background, when it could be sampled.
    pub contrast: Option<f64>,
    /// Contrast the text must reach (WCAG AA, large text aware).
    pub required_contrast: f64,
    /// The frame used for the contrast sample.
    pub sample_frame: i64,
    /// The text box at the sample frame.
    pub sample_box: Option<Box2>,
}

impl TextLegibility {
    /// Whether the reading time is enough.
    #[must_use]
    pub fn holds_long_enough(&self) -> bool {
        self.visible_seconds + 1e-9 >= self.required_seconds
    }

    /// Whether the measured contrast passes WCAG AA.
    #[must_use]
    pub fn contrast_ok(&self) -> bool {
        self.contrast
            .is_none_or(|ratio| ratio + 1e-9 >= self.required_contrast)
    }
}

/// The full legibility report.
#[derive(Debug, Clone, PartialEq)]
pub struct LegibilityReport {
    /// Per-text facts, in id order.
    pub texts: Vec<TextLegibility>,
    /// Composition frame rate.
    pub fps: f64,
    /// Frame step used for visibility.
    pub step: i64,
}

/// Whether a size/weight pair counts as WCAG large text.
#[must_use]
pub fn is_large_text(size: f64, weight: u16, rules: &Rules) -> bool {
    size >= rules.large_text_px || (weight >= 700 && size >= 18.66)
}

struct Seen {
    identity: String,
    text: String,
    words: usize,
    size: f64,
    weight: u16,
    color: Color,
    frames: std::collections::BTreeSet<i64>,
    sample_frame: i64,
    sample_box: Option<Box2>,
    sample_area: f64,
    sample_opacity: f64,
}

/// Builds the legibility report for `[start, end]` (inclusive).
#[allow(clippy::too_many_lines)] // the sampling pass is linear and explicit
#[must_use]
pub fn legibility_report(
    document: &Document,
    start: i64,
    end: i64,
    rules: &Rules,
    measure: &dyn TextMeasure,
    background: Option<&dyn BackgroundSample>,
) -> LegibilityReport {
    let duration = document.composition().duration().max(0);
    let start = start.max(0).min(duration.saturating_sub(1));
    let end = end.max(start).min(duration.saturating_sub(1));
    let span = end - start + 1;
    let step = if span > 3_600 { 2 } else { 1 };
    let fps = document.composition().frame_rate().as_f64();

    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    let mut seen: BTreeMap<NodeId, Seen> = BTreeMap::new();
    let mut frame = start;
    while frame <= end {
        evaluator.evaluate(document, frame, &mut plan);
        let scene = frame_elements(document, &plan, frame, measure);
        for element in &scene.elements {
            if element.kind != ElementKind::Text || element.opacity < rules.min_visible_opacity {
                continue;
            }
            let Some(content) = element.as_text() else {
                continue;
            };
            let Some(bounds) = element.bounds else {
                continue;
            };
            let area = bounds.area();
            let entry = seen.entry(element.id).or_insert_with(|| Seen {
                identity: element.identity.clone(),
                text: content.text.clone(),
                words: content.words,
                size: content.size,
                weight: content.weight,
                color: content.color,
                frames: std::collections::BTreeSet::new(),
                sample_frame: frame,
                sample_box: Some(bounds),
                sample_area: area,
                sample_opacity: element.opacity,
            });
            entry.frames.insert(frame);
            // Sample contrast where the text is most opaque (not mid-fade)
            // and, at equal opacity, where it is largest.
            let more_opaque = element.opacity > entry.sample_opacity + 0.01;
            let same_and_bigger =
                (element.opacity - entry.sample_opacity).abs() <= 0.01 && area > entry.sample_area;
            if more_opaque || same_and_bigger {
                entry.sample_area = area;
                entry.sample_opacity = element.opacity;
                entry.sample_frame = frame;
                entry.sample_box = Some(bounds);
            }
        }
        frame += step;
    }

    // Texts that swap in the same place (a ticker, an odometer) are one
    // reading target: their visible time adds up and the longest variant sets
    // the requirement. Grouping by box centre and size is deterministic.
    let mut groups: Vec<TextGroup> = Vec::new();
    for (id, entry) in seen {
        let center = entry
            .sample_box
            .map_or(motioner_core::Vec2::new(0.0, 0.0), |rect| rect.center());
        #[allow(clippy::cast_possible_truncation)]
        let key_x = center.x.round() as i64;
        #[allow(clippy::cast_possible_truncation)]
        let key_y = center.y.round() as i64;
        let key_size = (entry.size * 10.0).round() / 10.0;
        if let Some(group) = groups
            .iter_mut()
            .find(|(x, y, size, _)| *x == key_x && *y == key_y && (*size - key_size).abs() < 1e-6)
        {
            group.3.push((id, entry));
        } else {
            groups.push((key_x, key_y, key_size, vec![(id, entry)]));
        }
    }

    let mut texts = Vec::with_capacity(groups.len());
    for (_, _, _, entries) in groups {
        let frames: std::collections::BTreeSet<i64> = entries
            .iter()
            .flat_map(|(_, entry)| entry.frames.iter().copied())
            .collect();
        let words = entries
            .iter()
            .map(|(_, entry)| entry.words)
            .max()
            .unwrap_or(0);
        let (id, representative) = entries
            .iter()
            .max_by(|a, b| a.1.sample_opacity.total_cmp(&b.1.sample_opacity))
            .map_or((NodeId::from_raw(0), &entries[0].1), |(id, entry)| {
                (*id, entry)
            });
        #[allow(clippy::cast_precision_loss)]
        let visible_seconds =
            (frames.len() * usize::try_from(step).unwrap_or(1)) as f64 / fps.max(1.0);
        #[allow(clippy::cast_precision_loss)]
        let required_seconds = rules.reading_base + rules.reading_per_word * words as f64;
        let contrast = representative.sample_box.and_then(|box_| {
            background
                .and_then(|sampler| sampler.behind(representative.sample_frame, box_))
                .map(|behind| contrast_ratio(representative.color, behind))
        });
        let required_contrast = if is_large_text(representative.size, representative.weight, rules)
        {
            rules.min_contrast_large
        } else {
            rules.min_contrast
        };
        #[allow(clippy::cast_possible_wrap)]
        let visible_frames = i64::try_from(frames.len()).unwrap_or(0) * step;
        texts.push(TextLegibility {
            id,
            identity: representative.identity.clone(),
            text: representative.text.clone(),
            words,
            size: representative.size,
            weight: representative.weight,
            visible_frames,
            visible_seconds,
            required_seconds,
            contrast,
            required_contrast,
            sample_frame: representative.sample_frame,
            sample_box: representative.sample_box,
        });
    }

    LegibilityReport { texts, fps, step }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_on_white_is_twenty_one_to_one() {
        let ratio = contrast_ratio(Color::BLACK, Color::WHITE);
        assert!((ratio - 21.0).abs() < 1e-6, "{ratio}");
    }

    #[test]
    fn grey_on_white_fails_aa() {
        let grey = Color::rgb(150, 150, 150);
        assert!(contrast_ratio(grey, Color::WHITE) < 4.5);
    }

    #[test]
    fn large_text_uses_the_lower_threshold() {
        let rules = Rules::default();
        assert!(is_large_text(32.0, 400, &rules));
        assert!(is_large_text(20.0, 700, &rules));
        assert!(!is_large_text(20.0, 400, &rules));
    }
}
