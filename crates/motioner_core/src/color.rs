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
