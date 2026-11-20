//! Text transform and alignment mapping.
//!
//! Motioner text local space is `y`-down with the baseline through the origin.
//! `Pure2D` text local space is `y`-up with the pen starting at the baseline
//! origin, so the composition flip `F` is followed by a local `y` flip `S`:
//!
//! ```text
//! M_text = F.then(item.transform).then(S)
//! ```
//!
//! `S` makes the text model determinant positive, so glyphs stay upright. The
//! engine receives the em size separately; the transform already carries the
//! item scale.

use ferrous_font::TextAlign as EngineTextAlign;
use motioner_core::TextAlign;
use motioner_eval::Affine2;

use crate::Canvas2d;

/// Flips the engine's `y`-up text local space to Motioner's `y`-down local
/// space.
pub(crate) fn local_flip() -> Affine2 {
    Affine2 {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: -1.0,
        tx: 0.0,
        ty: 0.0,
    }
}

/// Maps a local-to-composition item transform to the text local-to-world
/// transform.
pub(crate) fn text_model(flip: Affine2, item: Affine2) -> Affine2 {
    flip.then(item).then(local_flip())
}

/// Maps a Motioner alignment to the engine's identical-named alignment.
pub(crate) fn align_to_engine(align: TextAlign) -> EngineTextAlign {
    match align {
        TextAlign::Left => EngineTextAlign::Left,
        TextAlign::Center => EngineTextAlign::Center,
        TextAlign::Right => EngineTextAlign::Right,
    }
}

/// How many characters of `text` have no glyph in `font`.
///
/// The engine answers with a character scan, without laying out or
/// allocating; it returns zero for a stale font handle.
pub(crate) fn count_missing(
    canvas: &impl Canvas2d,
    font: ferrous_2d::render::FontId2d,
    text: &str,
) -> u32 {
    canvas.missing_glyphs(font, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_maps_one_to_one() {
        assert_eq!(align_to_engine(TextAlign::Left), EngineTextAlign::Left);
        assert_eq!(align_to_engine(TextAlign::Center), EngineTextAlign::Center);
        assert_eq!(align_to_engine(TextAlign::Right), EngineTextAlign::Right);
    }

    #[test]
    fn local_flip_is_a_y_mirror() {
        let flip = local_flip();
        assert!((flip.determinant() + 1.0).abs() < 1e-12);
        assert_eq!(
            flip.apply(motioner_core::Vec2::new(2.0, 3.0)),
            motioner_core::Vec2::new(2.0, -3.0)
        );
    }
}
