//! Design tokens: colours, spacing, radii, type scale and DPI metrics.
//!
//! This is the only module that contains colour literals. Colours are written
//! as sRGB hex (what design tools show) and converted to linear light with
//! [`motioner_render::srgb_to_linear`], the single sRGB conversion in the
//! workspace. The engine's UI pass writes to an sRGB surface, which encodes
//! linear values back to sRGB, so a token renders exactly as its hex says.
//!
//! The engine's own `Color::hex` does **not** linearise (it stores the sRGB
//! bytes as if they were linear), which is why the first preview window looked
//! washed out. Never build UI colours with it.
//!
//! Sizes are written in logical pixels on a 4 px grid and scaled once by
//! [`Metrics`], which rounds to whole physical pixels so edges stay crisp.

use motioner_render::srgb_to_linear;

/// A colour written as `0xRRGGBB` sRGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Srgb(pub u32);

impl Srgb {
    /// The red, green and blue bytes.
    #[must_use]
    pub const fn channels(self) -> [u8; 3] {
        let [_, r, g, b] = self.0.to_be_bytes();
        [r, g, b]
    }

    /// Linear RGBA with full opacity.
    #[must_use]
    pub fn linear(self) -> [f32; 4] {
        self.alpha(1.0)
    }

    /// Linear RGBA with the given straight alpha.
    #[must_use]
    pub fn alpha(self, alpha: f32) -> [f32; 4] {
        let [r, g, b] = self.channels();
        [
            srgb_to_linear(r),
            srgb_to_linear(g),
            srgb_to_linear(b),
            alpha.clamp(0.0, 1.0),
        ]
    }

    /// WCAG relative luminance in `0.0..=1.0`.
    #[must_use]
    pub fn luminance(self) -> f32 {
        let [r, g, b] = self.channels();
        0.2126 * srgb_to_linear(r) + 0.7152 * srgb_to_linear(g) + 0.0722 * srgb_to_linear(b)
    }

    /// WCAG contrast ratio between two opaque colours (`1.0..=21.0`).
    #[must_use]
    pub fn contrast(self, other: Self) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        let (light, dark) = if a > b { (a, b) } else { (b, a) };
        (light + 0.05) / (dark + 0.05)
    }
}

/// Composites a straight-alpha linear colour over an opaque one, giving the
/// opaque colour a viewer sees. Icons use it to cut gaps that match a
/// translucent surface.
#[must_use]
pub fn over(top: [f32; 4], bottom: [f32; 4]) -> [f32; 4] {
    let a = top[3].clamp(0.0, 1.0);
    [
        top[0] * a + bottom[0] * (1.0 - a),
        top[1] * a + bottom[1] * (1.0 - a),
        top[2] * a + bottom[2] * (1.0 - a),
        1.0,
    ]
}

/// The raw palette. Panels and text pick from these through [`Theme`].
pub mod palette {
    use super::Srgb;

    /// Cool-neutral ramp, darkest first. The canvas pasteboard is the darkest
    /// step so the composition reads as the brightest object on screen.
    pub const N0: Srgb = Srgb(0x0A_0B_0D);
    /// App background between panels, timeline tracks.
    pub const N1: Srgb = Srgb(0x11_12_15);
    /// Panel surface.
    pub const N2: Srgb = Srgb(0x17_18_1C);
    /// Raised surface: panel headers, input fields.
    pub const N3: Srgb = Srgb(0x1E_1F_24);
    /// Control surface and row hover.
    pub const N4: Srgb = Srgb(0x26_28_2E);
    /// Pressed controls and strong borders.
    pub const N5: Srgb = Srgb(0x30_32_3A);
    /// Hairlines between regions.
    pub const N6: Srgb = Srgb(0x2A_2C_33);
    /// Disabled text, inactive icons.
    pub const N7: Srgb = Srgb(0x5E_62_6D);
    /// Secondary text and labels.
    pub const N8: Srgb = Srgb(0x90_95_A1);
    /// Body text.
    pub const N9: Srgb = Srgb(0xC9_CD_D5);
    /// Primary text and active icons.
    pub const N10: Srgb = Srgb(0xF0_F2_F5);

    /// The single accent (selection, playhead, primary action).
    pub const ACCENT: Srgb = Srgb(0x5B_8C_FF);
    /// Accent on hover.
    pub const ACCENT_HOVER: Srgb = Srgb(0x7A_A2_FF);
    /// Errors.
    pub const DANGER: Srgb = Srgb(0xF2_55_5A);
    /// Warnings.
    pub const WARNING: Srgb = Srgb(0xF5_A5_24);
    /// Success / healthy state.
    pub const SUCCESS: Srgb = Srgb(0x3F_CF_8E);
    /// Keyframe markers.
    pub const KEYFRAME: Srgb = Srgb(0xE8_C2_5A);
    /// Pure black for shadows.
    pub const BLACK: Srgb = Srgb(0x00_00_00);
    /// Pure white for overlays.
    pub const WHITE: Srgb = Srgb(0xFF_FF_FF);
}

/// Semantic colours in linear RGBA, resolved once at startup.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Canvas pasteboard around the composition.
    pub pasteboard: [f32; 4],
    /// App background and timeline track lanes.
    pub app: [f32; 4],
    /// Panel surface.
    pub panel: [f32; 4],
    /// Panel headers and input fields.
    pub raised: [f32; 4],
    /// Control surface (buttons, pills).
    pub control: [f32; 4],
    /// Control surface while hovered.
    pub control_hover: [f32; 4],
    /// Control surface while pressed.
    pub control_pressed: [f32; 4],
    /// Row hover tint (translucent white).
    pub row_hover: [f32; 4],
    /// Hairline separators.
    pub hairline: [f32; 4],
    /// Strong borders (canvas frame, focused fields).
    pub border: [f32; 4],
    /// Primary text and active icons.
    pub text: [f32; 4],
    /// Body text.
    pub text_body: [f32; 4],
    /// Secondary text and labels.
    pub text_muted: [f32; 4],
    /// Disabled text and inactive icons.
    pub text_disabled: [f32; 4],
    /// Accent.
    pub accent: [f32; 4],
    /// Accent while hovered.
    pub accent_hover: [f32; 4],
    /// Selected row background (translucent accent).
    pub selection: [f32; 4],
    /// Hovered-but-not-selected outline on the canvas.
    pub hover_outline: [f32; 4],
    /// Errors.
    pub danger: [f32; 4],
    /// Error banner background (translucent danger).
    pub danger_soft: [f32; 4],
    /// Warnings.
    pub warning: [f32; 4],
    /// Success.
    pub success: [f32; 4],
    /// Keyframe markers.
    pub keyframe: [f32; 4],
    /// Item lifetime bars in the timeline.
    pub track_bar: [f32; 4],
    /// Animated range inside a lifetime bar.
    pub track_bar_animated: [f32; 4],
    /// Drop shadows.
    pub shadow: [f32; 4],
    /// Tooltip background.
    pub tooltip: [f32; 4],
}

impl Theme {
    /// The dark editor theme.
    #[must_use]
    pub fn dark() -> Self {
        use palette::{
            ACCENT, ACCENT_HOVER, BLACK, DANGER, KEYFRAME, N0, N1, N10, N2, N3, N4, N5, N6, N7, N8,
            N9, SUCCESS, WARNING, WHITE,
        };
        Self {
            pasteboard: N0.linear(),
            app: N1.linear(),
            panel: N2.linear(),
            raised: N3.linear(),
            control: N4.linear(),
            control_hover: N5.linear(),
            control_pressed: N6.linear(),
            row_hover: WHITE.alpha(0.035),
            hairline: N6.linear(),
            border: N5.linear(),
            text: N10.linear(),
            text_body: N9.linear(),
            text_muted: N8.linear(),
            text_disabled: N7.linear(),
            accent: ACCENT.linear(),
            accent_hover: ACCENT_HOVER.linear(),
            selection: ACCENT.alpha(0.16),
            hover_outline: ACCENT.alpha(0.55),
            danger: DANGER.linear(),
            danger_soft: DANGER.alpha(0.14),
            warning: WARNING.linear(),
            success: SUCCESS.linear(),
            keyframe: KEYFRAME.linear(),
            track_bar: N4.linear(),
            track_bar_animated: ACCENT.alpha(0.28),
            shadow: BLACK.alpha(0.45),
            tooltip: N4.linear(),
        }
    }
}

/// Type scale in logical pixels (em size of the UI font).
pub mod type_scale {
    /// Captions, section labels, ruler numbers.
    pub const CAPTION: f32 = 11.0;
    /// Body text: rows, values.
    pub const BODY: f32 = 12.0;
    /// Emphasis: file name, panel titles.
    pub const EMPHASIS: f32 = 13.0;
    /// Large readouts: the timecode.
    pub const TITLE: f32 = 15.0;
}

/// Cap height of the embedded Roboto as a fraction of the em (1456 / 2048).
///
/// Text is centred on its cap height, which is what reads as optically
/// centred for UI labels made of capitals, digits and lowercase.
pub const CAP_HEIGHT_EM: f32 = 0.711;

/// Logical sizes on the 4 px grid. [`Metrics`] scales them.
pub mod size {
    /// Base grid unit.
    pub const GRID: f32 = 4.0;
    /// Top bar height.
    pub const TOP_BAR: f32 = 40.0;
    /// Status bar height.
    pub const STATUS_BAR: f32 = 24.0;
    /// Panel header height.
    pub const PANEL_HEADER: f32 = 32.0;
    /// Layer and track row height.
    pub const ROW: f32 = 28.0;
    /// Timeline ruler height.
    pub const RULER: f32 = 24.0;
    /// Timeline header (transport strip) height.
    pub const TRANSPORT: f32 = 44.0;
    /// Transport button size.
    pub const BUTTON: f32 = 28.0;
    /// Primary (play) button size.
    pub const BUTTON_PRIMARY: f32 = 32.0;
    /// Icon box inside buttons and rows.
    pub const ICON: f32 = 16.0;
    /// Indentation per outline depth.
    pub const INDENT: f32 = 16.0;
    /// Default layers panel width.
    pub const LEFT_PANEL: f32 = 264.0;
    /// Default inspector width.
    pub const RIGHT_PANEL: f32 = 288.0;
    /// Default timeline height.
    pub const TIMELINE: f32 = 236.0;
    /// Smallest canvas edge kept when panels grow.
    pub const MIN_CANVAS: f32 = 160.0;
    /// Splitter hit zone width.
    pub const SPLITTER_HIT: f32 = 8.0;
    /// Small corner radius (rows, fields).
    pub const RADIUS_SM: f32 = 4.0;
    /// Medium corner radius (buttons, cards).
    pub const RADIUS_MD: f32 = 6.0;
    /// Horizontal padding inside panels.
    pub const PAD: f32 = 12.0;
}

/// DPI-scaled metrics. Every value is a whole number of physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Physical pixels per logical pixel.
    pub scale: f32,
}

impl Metrics {
    /// Metrics for a scale factor; non-finite or tiny factors fall back to 1.
    #[must_use]
    pub fn new(scale: f32) -> Self {
        let scale = if scale.is_finite() && scale >= 0.5 {
            scale
        } else {
            1.0
        };
        Self { scale }
    }

    /// Scales a logical length and rounds it to whole physical pixels.
    #[must_use]
    pub fn px(self, logical: f32) -> f32 {
        (logical * self.scale).round()
    }

    /// One physical-pixel hairline at any scale (at least one pixel).
    #[must_use]
    pub fn hairline(self) -> f32 {
        self.scale.floor().max(1.0)
    }

    /// Scales a font size without rounding (MSDF text scales smoothly).
    #[must_use]
    pub fn font(self, logical: f32) -> f32 {
        logical * self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::palette::*;
    use super::*;

    #[test]
    fn hex_is_linearised_exactly_once() {
        // #808080 is 0.2158 in linear light; the old bug passed 0.502 through.
        let [r, g, b, a] = Srgb(0x80_80_80).linear();
        assert!((r - 0.215_861).abs() < 1e-4, "{r}");
        assert!((g - r).abs() < 1e-7 && (b - r).abs() < 1e-7);
        assert!((a - 1.0).abs() < f32::EPSILON);
        assert!(Srgb(0x00_00_00).linear()[0].abs() < f32::EPSILON);
        assert!((Srgb(0xFF_FF_FF).linear()[0] - 1.0).abs() < 1e-6);
        assert_eq!(Srgb(0x12_34_56).channels(), [0x12, 0x34, 0x56]);
    }

    #[test]
    fn dark_panels_stay_dark() {
        // The washed-out bug rendered #2f3d4d at ~#768699. A dark token must
        // stay below 5 % linear luminance.
        for token in [N0, N1, N2, N3, N4] {
            assert!(token.luminance() < 0.05, "{token:?}");
        }
        assert!(
            N0.luminance() < N2.luminance(),
            "pasteboard darker than panels"
        );
        assert!(
            N1.luminance() < N2.luminance(),
            "background darker than panels"
        );
    }

    #[test]
    fn text_meets_contrast_targets() {
        for surface in [N1, N2, N3] {
            assert!(N9.contrast(surface) >= 7.0, "body on {surface:?}");
            assert!(N8.contrast(surface) >= 4.5, "muted on {surface:?}");
            assert!(ACCENT.contrast(surface) >= 4.5, "accent on {surface:?}");
            assert!(DANGER.contrast(surface) >= 4.5, "danger on {surface:?}");
        }
        // Body text on a hovered control stays readable.
        assert!(N9.contrast(N5) >= 4.5);
    }

    #[test]
    fn compositing_over_an_opaque_surface() {
        let surface = [0.2, 0.2, 0.2, 1.0];
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(close(over([1.0, 0.0, 0.0, 0.0], surface), surface));
        assert!(close(
            over([1.0, 0.0, 0.0, 1.0], surface),
            [1.0, 0.0, 0.0, 1.0]
        ));
        let half = over([1.0, 1.0, 1.0, 0.5], surface);
        assert!((half[0] - 0.6).abs() < 1e-6 && (half[3] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn metrics_round_to_whole_pixels() {
        let m = Metrics::new(1.25);
        assert!((m.px(size::ROW) - 35.0).abs() < f32::EPSILON);
        assert!((m.px(size::GRID) - 5.0).abs() < f32::EPSILON);
        assert!((m.hairline() - 1.0).abs() < f32::EPSILON);
        assert!((Metrics::new(2.0).hairline() - 2.0).abs() < f32::EPSILON);
        assert!((Metrics::new(f32::NAN).scale - 1.0).abs() < f32::EPSILON);
    }
}
