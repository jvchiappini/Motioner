//! Text measurement and text boxes.
//!
//! Exact glyph advances live in the renderer's font set (D-0035), so the
//! analysis crate asks for them through [`TextMeasure`] instead of guessing.
//! A caller that has no font (a unit test) can implement the trait with a
//! fixed-width stand-in; the production adapter wraps
//! `motioner_render::CpuTextMetrics`.

use motioner_core::{TextAlign, TextContent, Vec2};

use crate::geom::Box2;

/// Ascender and descender are measured from the baseline; the descender is
/// positive here even though the engine reports it as a negative offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineExtent {
    /// Pen advance including kerning.
    pub advance: f64,
    /// Distance from the baseline up to the ascender.
    pub ascender: f64,
    /// Distance from the baseline down to the descender.
    pub descender: f64,
}

/// Measures one text line at a font size, in composition pixels.
pub trait TextMeasure {
    /// The extent of `text` at `size`, or `None` when it cannot be measured.
    fn line(&self, text: &str, size: f64) -> Option<LineExtent>;
}

impl<F: Fn(&str, f64) -> Option<LineExtent>> TextMeasure for F {
    fn line(&self, text: &str, size: f64) -> Option<LineExtent> {
        self(text, size)
    }
}

/// The font's natural line step as a fraction of the em size (Inter/Roboto
/// ascend + descend + gap).
pub const LINE_STEP_RATIO: f64 = 1.2;

/// The local box of a text node at one resolved font size.
///
/// Line 0's baseline passes through the origin; subsequent baselines are
/// `LINE_STEP_RATIO * size * line_height` apart. Horizontal alignment is
/// applied per line, exactly as the renderer does.
#[must_use]
pub fn text_local_box(
    content: &TextContent,
    size: f64,
    line_height: f64,
    align: TextAlign,
    measure: &dyn TextMeasure,
) -> Option<Box2> {
    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    let step = LINE_STEP_RATIO * size * line_height.max(0.0);
    let mut bounds: Option<Box2> = None;
    let mut line_index = 0_usize;
    for line in content.text.split('\n') {
        let extent = match measure.line(line, size) {
            Some(extent) => extent,
            // A blank line has no glyphs but still advances the baseline.
            None if line.is_empty() => {
                line_index += 1;
                continue;
            }
            None => return None,
        };
        #[allow(clippy::cast_precision_loss)] // line counts are tiny
        let baseline = step * line_index as f64;
        let x0 = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -extent.advance * 0.5,
            TextAlign::Right => -extent.advance,
        };
        let line_box = Box2::new(
            x0,
            baseline - extent.ascender,
            x0 + extent.advance,
            baseline + extent.descender,
        )?;
        bounds = Some(bounds.map_or(line_box, |current: Box2| current.union(line_box)));
        line_index += 1;
    }
    bounds
}

/// The width and height of a text block, for distance-relative thresholds.
#[must_use]
pub fn text_size(bounds: Box2) -> f64 {
    bounds.width().max(bounds.height()).max(1.0)
}

/// The local position of the centre of a text block.
#[must_use]
pub fn text_center(bounds: Box2) -> Vec2 {
    bounds.center()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unnecessary_wraps)] // same signature as the fallible TextMeasure adapter
    fn fixed(text: &str, size: f64) -> Option<LineExtent> {
        #[allow(clippy::cast_precision_loss)]
        let advance = size * 0.5 * text.chars().count() as f64;
        Some(LineExtent {
            advance,
            ascender: size * 0.8,
            descender: size * 0.2,
        })
    }

    fn content(text: &str) -> TextContent {
        TextContent::new(
            text,
            "Inter",
            40.0,
            motioner_core::Color::WHITE,
            TextAlign::Left,
        )
    }

    #[test]
    fn a_single_line_starts_at_the_origin() {
        let box_ = text_local_box(&content("Hello"), 40.0, 1.0, TextAlign::Left, &fixed).unwrap();
        assert!((box_.min_x - 0.0).abs() < 1e-9);
        assert!((box_.min_y + 32.0).abs() < 1e-9);
        assert!((box_.max_x - 100.0).abs() < 1e-9);
        assert!((box_.max_y - 8.0).abs() < 1e-9);
    }

    #[test]
    fn alignment_moves_the_line_around_the_origin() {
        let centered =
            text_local_box(&content("Hello"), 40.0, 1.0, TextAlign::Center, &fixed).unwrap();
        assert!((centered.center().x).abs() < 1e-9);
        let right = text_local_box(&content("Hello"), 40.0, 1.0, TextAlign::Right, &fixed).unwrap();
        assert!((right.max_x).abs() < 1e-9);
    }

    #[test]
    fn lines_stack_by_the_line_step() {
        let box_ = text_local_box(&content("A\nB"), 40.0, 1.0, TextAlign::Left, &fixed).unwrap();
        assert!(
            (box_.height() - (32.0 + 48.0 + 8.0)).abs() < 1e-9,
            "{box_:?}"
        );
    }
}
