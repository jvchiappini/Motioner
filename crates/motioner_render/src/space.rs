//! Coordinate, depth and colour mapping between Motioner and `Pure2D`.
//!
//! # Coordinate contract
//!
//! Motioner composition space is pixels with the origin at the top-left and
//! `y` pointing down. `Pure2D` world space has the origin at the target centre,
//! `y` pointing up and one world unit per pixel. The single mapping between the
//! two is the flip [`composition_flip`]:
//!
//! ```text
//! F(x, y) = (x - W/2, H/2 - y)
//! ```
//!
//! A render item's local-to-composition transform is applied first, then `F`.
//! Text additionally needs its local `y`-down axis flipped to the engine's
//! `y`-up text space (see [`crate::text`]).
//!
//! # Depth
//!
//! `Pure2D` has no depth buffer: painter order is the ascending-`z` sort of the
//! unified command stream. [`z_for`] maps a draw index to a strictly increasing
//! depth inside a window that stays well inside the camera's usable range.

use motioner_eval::Affine2;

/// Converts a composition-space point `(x, y)` to world space.
///
/// `F(x, y) = (x - W/2, H/2 - y)`.
pub(crate) fn composition_flip(width: u32, height: u32) -> Affine2 {
    Affine2 {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: -1.0,
        tx: -f64::from(width) / 2.0,
        ty: f64::from(height) / 2.0,
    }
}

/// Maps a local-to-composition transform to the shape local-to-world transform.
///
/// `M = F.then(item)`, so the item transform is applied first.
pub(crate) fn shape_model(flip: Affine2, item: Affine2) -> Affine2 {
    flip.then(item)
}

/// Builds an engine model matrix from a world affine and local dimensions.
///
/// The canonical shape quad `[-0.5, 0.5]²` is scaled by `(width, height)`; text
/// passes `1.0` for both because the engine places glyph quads itself. The
/// translation's `z` carries the painter-order depth.
pub(crate) fn model_mat4(model: Affine2, width: f64, height: f64, z: f32) -> glam::Mat4 {
    let x_axis_x = to_f32(model.a) * to_f32(width);
    let x_axis_y = to_f32(model.b) * to_f32(width);
    let y_axis_x = to_f32(model.c) * to_f32(height);
    let y_axis_y = to_f32(model.d) * to_f32(height);
    glam::Mat4::from_cols(
        glam::Vec4::new(x_axis_x, x_axis_y, 0.0, 0.0),
        glam::Vec4::new(y_axis_x, y_axis_y, 0.0, 0.0),
        glam::Vec4::new(0.0, 0.0, 1.0, 0.0),
        glam::Vec4::new(to_f32(model.tx), to_f32(model.ty), z, 1.0),
    )
}

/// Uniform world scale of an affine: `sqrt(|det(linear)|)`.
///
/// Corner radii and stroke widths are authored in local pixels and must be
/// converted to world units. Under a non-uniform scale no single factor is
/// exact, so this geometric mean is used as a documented approximation.
pub(crate) fn world_unit_scale(model: Affine2) -> f64 {
    model.determinant().abs().sqrt()
}

/// Maps a draw index to a depth in the open window `(-50, 50)`.
///
/// `z = -50 + (index + 0.5) * (100 / total)` is strictly increasing for
/// `index < total` and stays inside the camera's usable range `(-90, 110)`.
/// With no draws it returns the window start.
pub(crate) fn z_for(index: u32, total: u32) -> f32 {
    if total == 0 {
        return -50.0;
    }
    let index = f64::from(index);
    let total = f64::from(total);
    to_f32(-50.0 + (index + 0.5) * (100.0 / total))
}

/// Converts one 8-bit sRGB channel to linear light.
///
/// Applies the exact piecewise sRGB EOTF. Alpha is already linear coverage and
/// must not go through this function.
pub(crate) fn srgb_to_linear(value: u8) -> f32 {
    let channel = f32::from(value) / 255.0;
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// Converts a Motioner colour plus item opacity to a linear RGBA instance
/// colour.
///
/// RGB goes through [`srgb_to_linear`]; the final alpha is
/// `(colour.a / 255) * opacity`.
pub(crate) fn rgba(color: motioner_core::Color, opacity: f64) -> [f32; 4] {
    [
        srgb_to_linear(color.r),
        srgb_to_linear(color.g),
        srgb_to_linear(color.b),
        (f32::from(color.a) / 255.0) * to_f32(opacity),
    ]
}

/// Narrows an `f64` to the engine's `f32` precision.
///
/// Inputs come from validated, finite transforms and geometry; the narrowing is
/// the deliberate precision boundary at the engine adapter.
#[allow(clippy::cast_possible_truncation)] // deliberate f64 -> f32 engine boundary
pub(crate) fn to_f32(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::Vec2;

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    #[test]
    fn flip_maps_composition_corners_to_world_corners() {
        let flip = composition_flip(800, 600);
        let top_left = flip.apply(Vec2::ZERO);
        assert!(close(top_left.x, -400.0) && close(top_left.y, 300.0));
        let bottom_right = flip.apply(Vec2::new(800.0, 600.0));
        assert!(close(bottom_right.x, 400.0) && close(bottom_right.y, -300.0));
    }

    #[test]
    fn shape_model_is_mirrored_and_text_model_is_not() {
        let flip = composition_flip(200, 100);
        let item = Affine2::rotation_degrees(30.0).then(Affine2::scale(Vec2::splat(2.0)));
        let shape = shape_model(flip, item);
        assert!(shape.determinant() < 0.0);
        let text = crate::text::text_model(flip, item);
        assert!(text.determinant() > 0.0);
    }

    #[test]
    fn text_local_up_maps_above_the_composition_origin() {
        let item = Affine2::IDENTITY;
        let text_local = item.then(crate::text::local_flip());
        let origin = text_local.apply(Vec2::ZERO);
        let one_unit_up = text_local.apply(Vec2::new(0.0, 1.0));
        assert!(
            one_unit_up.y < origin.y,
            "y points down in composition space"
        );
        assert!(close(one_unit_up.y, origin.y - 1.0));
    }

    #[test]
    fn z_is_strictly_increasing_inside_the_safe_window() {
        let total: u32 = 1000;
        let mut previous = f32::NEG_INFINITY;
        for index in 0..total {
            let z = z_for(index, total);
            assert!(z > -90.0 && z < 110.0, "z {z} outside the safe window");
            assert!(z > previous, "z must increase with the draw index");
            previous = z;
        }
    }

    #[test]
    fn z_with_no_draws_is_the_window_start() {
        assert!(close(f64::from(z_for(0, 0)), -50.0));
    }

    #[test]
    fn srgb_to_linear_matches_known_points() {
        assert!(close(f64::from(srgb_to_linear(0)), 0.0));
        assert!(close(f64::from(srgb_to_linear(255)), 1.0));
        let mid = f64::from(srgb_to_linear(128));
        assert!((mid - 0.215_860_5).abs() < 1e-5, "got {mid}");
        let dark = f64::from(srgb_to_linear(10));
        assert!((dark - 0.003_035_3).abs() < 1e-5, "got {dark}");
        let quarter = f64::from(srgb_to_linear(64));
        assert!((quarter - 0.051_269_2).abs() < 1e-5, "got {quarter}");
    }

    #[test]
    fn world_unit_scale_uses_the_determinant() {
        let scale = Affine2::scale(Vec2::new(2.0, 8.0));
        assert!(close(world_unit_scale(scale), 4.0));
    }
}
