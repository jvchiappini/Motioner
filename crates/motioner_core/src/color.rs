//! sRGB colors with straight alpha, parsed from `#rrggbb[aa]` strings.

use std::fmt;

/// An 8-bit sRGB color with straight (non-premultiplied) alpha.
///
/// The document stores colors exactly as authored; conversion to linear
/// light happens in the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red channel, sRGB encoded.
    pub r: u8,
    /// Green channel, sRGB encoded.
    pub g: u8,
    /// Blue channel, sRGB encoded.
    pub b: u8,
    /// Alpha channel, linear coverage.
    pub a: u8,
}

/// Error returned when a color string is not `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorParseError {
    input: String,
}

impl fmt::Display for ColorParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid color {:?}: expected \"#rrggbb\" or \"#rrggbbaa\"",
            self.input
        )
    }
}

impl std::error::Error for ColorParseError {}

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    /// Creates an opaque color.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Parses `#rrggbb` or `#rrggbbaa` (hex digits are case-insensitive).
    ///
    /// # Errors
    ///
    /// Returns [`ColorParseError`] for any other shape or non-hex digit.
    pub fn parse_hex(input: &str) -> Result<Self, ColorParseError> {
        let error = || ColorParseError {
            input: input.to_owned(),
        };
        let digits = input.strip_prefix('#').ok_or_else(error)?;
        if !(digits.len() == 6 || digits.len() == 8) || !digits.is_ascii() {
            return Err(error());
        }
        let channel = |index: usize| {
            u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16).map_err(|_| error())
        };
        let a = if digits.len() == 8 { channel(3)? } else { 255 };
        Ok(Self {
            r: channel(0)?,
            g: channel(1)?,
            b: channel(2)?,
            a,
        })
    }

    /// The `#RRGGBB[AA]` text of this colour (alpha omitted when opaque).
    #[must_use]
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// Converts one sRGB-encoded channel byte to linear light (the sRGB EOTF).
    ///
    /// Alpha is linear coverage and must not go through this function. The
    /// renderer and colour interpolation share this one transfer function
    /// (D-0028), so a keyed colour and its alpha blend cannot disagree.
    #[must_use]
    pub fn srgb_to_linear(value: u8) -> f32 {
        let channel = f32::from(value) / 255.0;
        if channel <= 0.040_45 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    /// Converts a linear-light channel in `0.0..=1.0` to the nearest sRGB byte.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // The clamp makes the value a valid byte; the cast is the rounding step.
    pub fn linear_to_srgb(value: f32) -> u8 {
        let channel = value.clamp(0.0, 1.0);
        let encoded = if channel <= 0.003_130_8 {
            channel * 12.92
        } else {
            1.055 * channel.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round() as u8
    }

    /// Interpolates two colours in linear-light sRGB, alpha included (D-0028).
    ///
    /// This is the colour space every keyed colour channel uses, so a midpoint
    /// matches the linear compositor instead of the naive 8-bit average.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // Every channel is clamped to a byte before the cast.
    pub fn lerp(a: Self, b: Self, t: f64) -> Self {
        let channel = |from: u8, to: u8| {
            let from = f64::from(Self::srgb_to_linear(from));
            let to = f64::from(Self::srgb_to_linear(to));
            let linear = (from + (to - from) * t).clamp(0.0, 1.0);
            Self::linear_to_srgb(linear as f32)
        };
        let alpha = f64::from(a.a) + (f64::from(b.a) - f64::from(a.a)) * t;
        Self {
            r: channel(a.r, b.r),
            g: channel(a.g, b.g),
            b: channel(a.b, b.b),
            a: alpha.round().clamp(0.0, 255.0) as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rgb_and_rgba() {
        assert_eq!(
            Color::parse_hex("#ff3366").unwrap(),
            Color::rgb(255, 51, 102)
        );
        let c = Color::parse_hex("#0B0B1280").unwrap();
        assert_eq!((c.r, c.g, c.b, c.a), (11, 11, 18, 128));
    }

    #[test]
    fn rejects_malformed_colors() {
        for bad in [
            "ff3366",
            "#ff336",
            "#gg0000",
            "#ff33661",
            "",
            "#\u{e9}\u{e9}\u{e9}",
        ] {
            assert!(Color::parse_hex(bad).is_err(), "{bad}");
        }
    }
}
