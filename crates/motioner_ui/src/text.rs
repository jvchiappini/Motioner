//! UI text: measurement, kerning, tracking, ellipsis and baseline placement.
//!
//! The engine's `GuiBatch::draw_text` takes the *top of the em box* as its `y`
//! (the baseline lands at `y + size`) and ignores kerning, which is why the
//! first window's top-bar labels sat on the bottom edge. This module places
//! glyphs from the same MSDF atlas itself: callers give a baseline, or a box
//! to centre the text in, and get kerned, optionally tracked text.
//!
//! The atlas cell of every glyph spans `[-0.3, 1.3]` em on both axes around
//! the pen position on the baseline (`ferrous_font::FontAtlas::cell_bounds`),
//! so a glyph quad is `1.6 × size` square with its top-left corner at
//! `(pen_x - 0.3 size, baseline - 1.3 size)`.
//!
//! Measurement ([`advance`], [`ellipsize`]) only reads glyph advances and
//! kerning through the [`Glyphs`] trait, so it is unit-tested without a GPU.

use std::borrow::Cow;

use ferrous_font::Font;
use ferrous_gui::{GuiBatch, TextQuad};

use crate::layout::Rect;
use crate::theme::CAP_HEIGHT_EM;

/// The ellipsis appended to truncated text.
pub const ELLIPSIS: &str = "…";

/// How a run of text looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Em size in physical pixels.
    pub size: f32,
    /// Linear RGBA colour.
    pub color: [f32; 4],
    /// Extra space between characters, in em.
    pub tracking: f32,
    /// Medium weight: the run is drawn twice, half a pixel apart. The UI font
    /// ships one weight, and this reads as a clean medium at UI sizes.
    pub medium: bool,
}

impl TextStyle {
    /// A regular-weight style.
    #[must_use]
    pub fn new(size: f32, color: [f32; 4]) -> Self {
        Self {
            size,
            color,
            tracking: 0.0,
            medium: false,
        }
    }

    /// The same style in medium weight.
    #[must_use]
    pub fn medium(self) -> Self {
        Self {
            medium: true,
            ..self
        }
    }

    /// The same style with `tracking` em of extra letter spacing.
    #[must_use]
    pub fn tracked(self, tracking: f32) -> Self {
        Self { tracking, ..self }
    }

    /// The same style in another colour.
    #[must_use]
    pub fn color(self, color: [f32; 4]) -> Self {
        Self { color, ..self }
    }
}

/// Horizontal placement inside a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// Flush left.
    Left,
    /// Centred.
    Center,
    /// Flush right.
    Right,
}

/// Glyph advances and kerning, in em. Implemented for the engine font and
/// for test fixtures.
pub trait Glyphs {
    /// Advance of `c`, or `None` when the font has no glyph for it.
    fn advance(&self, c: char) -> Option<f32>;
    /// Kerning adjustment between `left` and `right`.
    fn kerning(&self, left: char, right: char) -> f32;
}

impl Glyphs for Font {
    fn advance(&self, c: char) -> Option<f32> {
        self.atlas.metrics.get(&c).map(|m| m.advance)
    }

    fn kerning(&self, left: char, right: char) -> f32 {
        let atlas = &self.atlas;
        match (
            atlas.glyph_indices.get(&left),
            atlas.glyph_indices.get(&right),
        ) {
            (Some(a), Some(b)) => atlas.kerning.get(&(*a, *b)).copied().unwrap_or(0.0),
            _ => 0.0,
        }
    }
}

/// Advance used for characters the font cannot draw, in em.
const MISSING_ADVANCE: f32 = 0.5;

/// Width of `text` in physical pixels, including kerning and tracking.
///
/// Tracking is added between characters, not after the last one, so tracked
/// text still aligns flush right.
#[must_use]
pub fn advance(glyphs: &impl Glyphs, text: &str, style: &TextStyle) -> f32 {
    let mut width = 0.0;
    let mut previous = None;
    for c in text.chars() {
        if let Some(p) = previous {
            width += (glyphs.kerning(p, c) + style.tracking) * style.size;
        }
        width += glyphs.advance(c).unwrap_or(MISSING_ADVANCE) * style.size;
        previous = Some(c);
    }
    width
}

/// `text` shortened with a trailing ellipsis so it fits in `max_width`.
///
/// Returns the text unchanged when it fits, the longest prefix plus `…` when
/// it does not, and an empty string when not even the ellipsis fits.
#[must_use]
pub fn ellipsize<'a>(
    glyphs: &impl Glyphs,
    text: &'a str,
    style: &TextStyle,
    max_width: f32,
) -> Cow<'a, str> {
    if advance(glyphs, text, style) <= max_width {
        return Cow::Borrowed(text);
    }
    let ellipsis = advance(glyphs, ELLIPSIS, style);
    if ellipsis > max_width {
        return Cow::Borrowed("");
    }
    let budget = max_width - ellipsis;
    let mut end = 0;
    for (index, c) in text.char_indices() {
        let next = index + c.len_utf8();
        if advance(glyphs, text[..next].trim_end(), style) > budget {
            break;
        }
        end = next;
    }
    Cow::Owned(format!("{}{ELLIPSIS}", text[..end].trim_end()))
}

/// The baseline that centres capitals and digits on `center_y`, snapped to a
/// whole pixel so stems stay crisp.
#[must_use]
pub fn baseline_for_center(center_y: f32, size: f32) -> f32 {
    (center_y + CAP_HEIGHT_EM * size * 0.5).round()
}

/// Draws `text` with its pen starting at `x` on `baseline`.
///
/// Returns the advance actually drawn, so callers can place the next run.
pub fn draw(
    batch: &mut GuiBatch,
    font: &Font,
    text: &str,
    x: f32,
    baseline: f32,
    style: &TextStyle,
) -> f32 {
    let width = draw_pass(batch, font, text, x, baseline, style);
    if style.medium {
        draw_pass(batch, font, text, x + 0.5, baseline, style);
    }
    width
}

/// Draws `text` vertically centred in `rect` with the given alignment,
/// shortened with an ellipsis when it does not fit.
pub fn draw_in(
    batch: &mut GuiBatch,
    font: &Font,
    text: &str,
    rect: Rect,
    align: Align,
    style: &TextStyle,
) {
    let shown = ellipsize(font, text, style, rect.w);
    let width = advance(font, &shown, style);
    let x = match align {
        Align::Left => rect.x,
        Align::Center => rect.center_x() - width * 0.5,
        Align::Right => rect.right() - width,
    };
    let baseline = baseline_for_center(rect.center_y(), style.size);
    draw(batch, font, &shown, x.round(), baseline, style);
}

fn draw_pass(
    batch: &mut GuiBatch,
    font: &Font,
    text: &str,
    x: f32,
    baseline: f32,
    style: &TextStyle,
) -> f32 {
    let size = style.size;
    let quad = 1.6 * size;
    let mut pen = x;
    let mut previous = None;
    for c in text.chars() {
        if let Some(p) = previous {
            pen += (font.kerning(p, c) + style.tracking) * size;
        }
        previous = Some(c);
        let Some(metric) = font.atlas.metrics.get(&c) else {
            pen += MISSING_ADVANCE * size;
            continue;
        };
        if !c.is_whitespace() {
            batch.push_text(TextQuad {
                pos: [pen - 0.3 * size, baseline - 1.3 * size],
                size: [quad, quad],
                uv0: [metric.uv[0], metric.uv[1]],
                uv1: [metric.uv[2], metric.uv[3]],
                color: style.color,
                z_order: 0.0,
                node_id: 0,
            });
        }
        pen += metric.advance * size;
    }
    pen - x
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monospace fixture: every glyph is 0.5 em, "AV" kerns by -0.1 em.
    struct Mono;

    impl Glyphs for Mono {
        fn advance(&self, c: char) -> Option<f32> {
            (c != '\u{1}').then_some(0.5)
        }

        fn kerning(&self, left: char, right: char) -> f32 {
            if (left, right) == ('A', 'V') {
                -0.1
            } else {
                0.0
            }
        }
    }

    fn style() -> TextStyle {
        TextStyle::new(10.0, [1.0; 4])
    }

    #[test]
    fn advance_sums_glyphs_kerning_and_tracking() {
        assert!((advance(&Mono, "abc", &style()) - 15.0).abs() < 1e-4);
        assert!(
            (advance(&Mono, "AV", &style()) - 9.0).abs() < 1e-4,
            "kerned pair"
        );
        // Tracking goes between characters only: two gaps for three glyphs.
        assert!((advance(&Mono, "abc", &style().tracked(0.1)) - 17.0).abs() < 1e-4);
        assert!((advance(&Mono, "", &style())).abs() < 1e-6);
        assert!(
            (advance(&Mono, "\u{1}", &style()) - 5.0).abs() < 1e-4,
            "missing glyph advance"
        );
    }

    #[test]
    fn ellipsis_only_when_needed() {
        assert_eq!(ellipsize(&Mono, "abcd", &style(), 20.0), "abcd");
        // 6 chars = 30 px; the ellipsis is 5 px; 25 px leaves room for "abcd".
        assert_eq!(ellipsize(&Mono, "abcdef", &style(), 25.0), "abcd…");
        assert_eq!(ellipsize(&Mono, "abcdef", &style(), 4.0), "");
        assert_eq!(ellipsize(&Mono, "abcdef", &style(), 5.0), "…");
    }

    #[test]
    fn ellipsis_never_overflows_and_trims_spaces() {
        for width in [6.0, 11.0, 17.0, 23.0, 29.0] {
            let shown = ellipsize(&Mono, "ab cd ef gh", &style(), width);
            assert!(
                advance(&Mono, &shown, &style()) <= width + 1e-4,
                "{shown} at {width}"
            );
            assert!(
                !shown.contains(" …"),
                "no space before the ellipsis: {shown}"
            );
        }
    }

    #[test]
    fn ellipsis_respects_char_boundaries() {
        let shown = ellipsize(&Mono, "ñandú über", &style(), 22.0);
        assert!(shown.ends_with(ELLIPSIS));
        assert!(shown.starts_with("ña"));
    }

    #[test]
    fn baseline_centres_the_cap_height() {
        let baseline = baseline_for_center(20.0, 12.0);
        // Cap height 8.5 px: caps span baseline-8.5 .. baseline, centred on 20.
        let cap_top = baseline - CAP_HEIGHT_EM * 12.0;
        assert!(((cap_top + baseline) * 0.5 - 20.0).abs() <= 0.5);
        assert!((baseline - baseline.round()).abs() < f32::EPSILON);
    }
}
