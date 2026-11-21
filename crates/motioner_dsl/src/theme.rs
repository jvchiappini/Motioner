//! Design tokens and the curated themes of the MDF standard library (D-0039).
//!
//! A theme is a small, fixed token set: colours, a type scale for 1080p, a
//! spacing and radius scale, and motion tokens. Constructors and setters accept
//! a token name wherever they accept a colour or a number, and [`Theme::color`]
//! / [`Theme::size`] resolve it. The interpreter owns the active theme; this
//! module is engine-free and has no state.
//!
//! The ten themes are reviewed visually (see `docs/std.md`); the contrast
//! check in [`Theme::validate_contrast`] is the WCAG AA gate applied when a
//! theme becomes active.

use motioner_core::{Color, Ease};

/// A curated design-token set.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Stable theme name used by `theme(name)` and `use_theme(t)`.
    pub name: &'static str,
    /// Composition background.
    pub bg: Color,
    /// Raised surface (cards, panels).
    pub surface: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text.
    pub muted: Color,
    /// Primary accent.
    pub accent: Color,
    /// Secondary accent.
    pub accent_2: Color,
    /// Success colour.
    pub success: Color,
    /// Warning colour.
    pub warning: Color,
    /// Danger colour.
    pub danger: Color,
    /// Display font family.
    pub display: &'static str,
    /// Body font family.
    pub body: &'static str,
    /// Display weight.
    pub display_weight: u16,
    /// Heading weight.
    pub heading_weight: u16,
    /// Body weight.
    pub body_weight: u16,
    /// Label weight.
    pub label_weight: u16,
    /// Display size in pixels at 1080p.
    pub display_size: f64,
    /// `h1` size in pixels at 1080p.
    pub h1: f64,
    /// `h2` size in pixels at 1080p.
    pub h2: f64,
    /// `h3` size in pixels at 1080p.
    pub h3: f64,
    /// Body size in pixels at 1080p.
    pub body_size: f64,
    /// Caption size in pixels at 1080p.
    pub caption: f64,
    /// Label size in pixels at 1080p.
    pub label: f64,
    /// Heading tracking in em units.
    pub heading_tracking: f64,
    /// Body tracking in em units.
    pub body_tracking: f64,
    /// Extra-small spacing.
    pub space_xs: f64,
    /// Small spacing.
    pub space_s: f64,
    /// Medium spacing.
    pub space_m: f64,
    /// Large spacing.
    pub space_l: f64,
    /// Extra-large spacing.
    pub space_xl: f64,
    /// Small radius.
    pub radius_s: f64,
    /// Medium radius.
    pub radius_m: f64,
    /// Large radius.
    pub radius_l: f64,
    /// Default tempo in beats per minute.
    pub tempo: f64,
    /// Enter ease.
    pub enter_ease: Ease,
    /// Exit ease.
    pub exit_ease: Ease,
    /// Move ease.
    pub move_ease: Ease,
    /// Emphasis ease.
    pub emphasis_ease: Ease,
}

/// Every curated theme, in a stable order.
pub const THEMES: &[Theme] = &[
    Theme {
        name: "dark",
        bg: rgb(0x0B, 0x0B, 0x12),
        surface: rgb(0x16, 0x18, 0x27),
        text: rgb(0xF5, 0xF7, 0xFF),
        muted: rgb(0x9A, 0xA3, 0xB8),
        accent: rgb(0x6C, 0x8C, 0xFF),
        accent_2: rgb(0xB4, 0x7C, 0xFF),
        success: rgb(0x35, 0xD0, 0x7F),
        warning: rgb(0xFF, 0xB0, 0x20),
        danger: rgb(0xFF, 0x5C, 0x6C),
        display: "Inter",
        body: "Inter",
        display_weight: 700,
        heading_weight: 600,
        body_weight: 400,
        label_weight: 600,
        display_size: 96.0,
        h1: 72.0,
        h2: 56.0,
        h3: 40.0,
        body_size: 32.0,
        caption: 24.0,
        label: 18.0,
        heading_tracking: 0.0,
        body_tracking: 0.0,
        space_xs: 8.0,
        space_s: 16.0,
        space_m: 24.0,
        space_l: 40.0,
        space_xl: 64.0,
        radius_s: 8.0,
        radius_m: 16.0,
        radius_l: 28.0,
        tempo: 120.0,
        enter_ease: Ease::CubicOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::CubicInOut,
        emphasis_ease: Ease::BackOut,
    },
    Theme {
        name: "light",
        bg: rgb(0xF7, 0xF8, 0xFC),
        surface: rgb(0xFF, 0xFF, 0xFF),
        text: rgb(0x12, 0x14, 0x1C),
        muted: rgb(0x5C, 0x64, 0x75),
        accent: rgb(0x3D, 0x5A, 0xFE),
        accent_2: rgb(0x7C, 0x4D, 0xFF),
        success: rgb(0x0E, 0x9F, 0x6E),
        warning: rgb(0xB4, 0x6A, 0x00),
        danger: rgb(0xD7, 0x1D, 0x3C),
        display: "Inter",
        body: "Inter",
        display_weight: 700,
        heading_weight: 600,
        body_weight: 400,
        label_weight: 600,
        display_size: 96.0,
        h1: 72.0,
        h2: 56.0,
        h3: 40.0,
        body_size: 32.0,
        caption: 24.0,
        label: 18.0,
        heading_tracking: 0.0,
        body_tracking: 0.0,
        space_xs: 8.0,
        space_s: 16.0,
        space_m: 24.0,
        space_l: 40.0,
        space_xl: 64.0,
        radius_s: 8.0,
        radius_m: 16.0,
        radius_l: 28.0,
        tempo: 120.0,
        enter_ease: Ease::CubicOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::CubicInOut,
        emphasis_ease: Ease::BackOut,
    },
    Theme {
        name: "editorial",
        bg: rgb(0xFB, 0xF8, 0xF2),
        surface: rgb(0xFF, 0xFF, 0xFF),
        text: rgb(0x1A, 0x18, 0x15),
        muted: rgb(0x6B, 0x65, 0x5C),
        accent: rgb(0xB5, 0x43, 0x2B),
        accent_2: rgb(0x2F, 0x5D, 0x50),
        success: rgb(0x2F, 0x5D, 0x50),
        warning: rgb(0xA8, 0x6A, 0x1F),
        danger: rgb(0xA8, 0x2B, 0x2B),
        display: "Inter",
        body: "Inter",
        display_weight: 700,
        heading_weight: 600,
        body_weight: 400,
        label_weight: 600,
        display_size: 92.0,
        h1: 68.0,
        h2: 52.0,
        h3: 38.0,
        body_size: 30.0,
        caption: 23.0,
        label: 17.0,
        heading_tracking: 0.0,
        body_tracking: 0.0,
        space_xs: 8.0,
        space_s: 14.0,
        space_m: 22.0,
        space_l: 36.0,
        space_xl: 58.0,
        radius_s: 4.0,
        radius_m: 8.0,
        radius_l: 14.0,
        tempo: 96.0,
        enter_ease: Ease::CubicOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::CubicInOut,
        emphasis_ease: Ease::BackOut,
    },
    Theme {
        name: "playful",
        bg: rgb(0xFF, 0xF7, 0xFB),
        surface: rgb(0xFF, 0xFF, 0xFF),
        text: rgb(0x24, 0x18, 0x26),
        muted: rgb(0x7A, 0x68, 0x78),
        accent: rgb(0xFF, 0x4D, 0x8D),
        accent_2: rgb(0x2B, 0xB6, 0xFF),
        success: rgb(0x1F, 0xB8, 0x6B),
        warning: rgb(0xD9, 0x7A, 0x00),
        danger: rgb(0xE0, 0x33, 0x55),
        display: "Inter",
        body: "Inter",
        display_weight: 800,
        heading_weight: 700,
        body_weight: 400,
        label_weight: 700,
        display_size: 104.0,
        h1: 78.0,
        h2: 60.0,
        h3: 42.0,
        body_size: 32.0,
        caption: 24.0,
        label: 18.0,
        heading_tracking: 0.0,
        body_tracking: 0.0,
        space_xs: 10.0,
        space_s: 18.0,
        space_m: 28.0,
        space_l: 44.0,
        space_xl: 72.0,
        radius_s: 12.0,
        radius_m: 22.0,
        radius_l: 36.0,
        tempo: 128.0,
        enter_ease: Ease::BackOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::BackOut,
        emphasis_ease: Ease::ElasticOut,
    },
    Theme {
        name: "corporate",
        bg: rgb(0xFF, 0xFF, 0xFF),
        surface: rgb(0xF2, 0xF5, 0xFA),
        text: rgb(0x0F, 0x1B, 0x2D),
        muted: rgb(0x5A, 0x6B, 0x84),
        accent: rgb(0x1B, 0x57, 0xC2),
        accent_2: rgb(0x0E, 0x9F, 0x8A),
        success: rgb(0x0E, 0x9F, 0x6E),
        warning: rgb(0xB4, 0x6A, 0x00),
        danger: rgb(0xC6, 0x28, 0x28),
        display: "Inter",
        body: "Inter",
        display_weight: 700,
        heading_weight: 600,
        body_weight: 400,
        label_weight: 600,
        display_size: 88.0,
        h1: 64.0,
        h2: 48.0,
        h3: 36.0,
        body_size: 30.0,
        caption: 22.0,
        label: 18.0,
        heading_tracking: 0.0,
        body_tracking: 0.0,
        space_xs: 8.0,
        space_s: 16.0,
        space_m: 24.0,
        space_l: 40.0,
        space_xl: 64.0,
        radius_s: 6.0,
        radius_m: 10.0,
        radius_l: 18.0,
        tempo: 110.0,
        enter_ease: Ease::CubicOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::CubicInOut,
        emphasis_ease: Ease::BackOut,
    },
    Theme {
        name: "neon",
        bg: rgb(0x05, 0x06, 0x0A),
        surface: rgb(0x0D, 0x12, 0x20),
        text: rgb(0xEA, 0xF6, 0xFF),
        muted: rgb(0x7C, 0x8B, 0xA6),
        accent: rgb(0x00, 0xE5, 0xFF),
        accent_2: rgb(0xFF, 0x2B, 0xD6),
        success: rgb(0x3D, 0xFF, 0xB0),
        warning: rgb(0xFF, 0xD2, 0x3D),
        danger: rgb(0xFF, 0x3D, 0x6B),
        display: "Inter",
        body: "Inter",
        display_weight: 800,
        heading_weight: 700,
        body_weight: 400,
        label_weight: 700,
        display_size: 104.0,
        h1: 78.0,
        h2: 60.0,
        h3: 42.0,
        body_size: 32.0,
        caption: 24.0,
        label: 18.0,
        heading_tracking: 0.02,
        body_tracking: 0.0,
        space_xs: 10.0,
        space_s: 18.0,
        space_m: 28.0,
        space_l: 46.0,
        space_xl: 74.0,
        radius_s: 4.0,
        radius_m: 8.0,
        radius_l: 16.0,
        tempo: 128.0,
        enter_ease: Ease::ExpoInOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::ExpoInOut,
        emphasis_ease: Ease::ElasticOut,
    },
    Theme {
        name: "pastel",
        bg: rgb(0xFD, 0xF6, 0xF0),
        surface: rgb(0xFF, 0xFF, 0xFF),
        text: rgb(0x2A, 0x24, 0x30),
        muted: rgb(0x8B, 0x7F, 0x86),
        accent: rgb(0xF4, 0x9A, 0xC2),
        accent_2: rgb(0x8F, 0xD3, 0xC1),
        success: rgb(0x6F, 0xC2, 0x9A),
        warning: rgb(0xE8, 0xB4, 0x6A),
        danger: rgb(0xE0, 0x7A, 0x8A),
        display: "Inter",
        body: "Inter",
        display_weight: 700,
        heading_weight: 600,
        body_weight: 400,
        label_weight: 600,
        display_size: 96.0,
        h1: 72.0,
        h2: 54.0,
        h3: 40.0,
        body_size: 32.0,
        caption: 24.0,
        label: 18.0,
        heading_tracking: 0.0,
        body_tracking: 0.0,
        space_xs: 10.0,
        space_s: 18.0,
        space_m: 28.0,
        space_l: 44.0,
        space_xl: 70.0,
        radius_s: 12.0,
        radius_m: 22.0,
        radius_l: 36.0,
        tempo: 112.0,
        enter_ease: Ease::SineInOut,
        exit_ease: Ease::SineInOut,
        move_ease: Ease::SineInOut,
        emphasis_ease: Ease::BackOut,
    },
    Theme {
        name: "mono",
        bg: rgb(0x11, 0x11, 0x11),
        surface: rgb(0x1D, 0x1D, 0x1D),
        text: rgb(0xF4, 0xF4, 0xF4),
        muted: rgb(0x9A, 0x9A, 0x9A),
        accent: rgb(0xE6, 0xE6, 0xE6),
        accent_2: rgb(0xBF, 0xBF, 0xBF),
        success: rgb(0xC9, 0xC9, 0xC9),
        warning: rgb(0xD8, 0xD8, 0xD8),
        danger: rgb(0xFF, 0xFF, 0xFF),
        display: "Inter",
        body: "Inter",
        display_weight: 700,
        heading_weight: 600,
        body_weight: 400,
        label_weight: 600,
        display_size: 96.0,
        h1: 72.0,
        h2: 56.0,
        h3: 40.0,
        body_size: 30.0,
        caption: 22.0,
        label: 16.0,
        heading_tracking: 0.04,
        body_tracking: 0.01,
        space_xs: 8.0,
        space_s: 16.0,
        space_m: 24.0,
        space_l: 40.0,
        space_xl: 64.0,
        radius_s: 0.0,
        radius_m: 2.0,
        radius_l: 4.0,
        tempo: 100.0,
        enter_ease: Ease::CubicOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::CubicInOut,
        emphasis_ease: Ease::Linear,
    },
    Theme {
        name: "warm",
        bg: rgb(0x19, 0x13, 0x10),
        surface: rgb(0x24, 0x1A, 0x15),
        text: rgb(0xFF, 0xF3, 0xE6),
        muted: rgb(0xB8, 0xA9, 0x9C),
        accent: rgb(0xFF, 0x8A, 0x3D),
        accent_2: rgb(0xFF, 0xC2, 0x47),
        success: rgb(0x7F, 0xC4, 0x6A),
        warning: rgb(0xFF, 0xC2, 0x47),
        danger: rgb(0xFF, 0x6B, 0x5B),
        display: "Inter",
        body: "Inter",
        display_weight: 700,
        heading_weight: 600,
        body_weight: 400,
        label_weight: 600,
        display_size: 96.0,
        h1: 72.0,
        h2: 56.0,
        h3: 40.0,
        body_size: 32.0,
        caption: 24.0,
        label: 18.0,
        heading_tracking: 0.0,
        body_tracking: 0.0,
        space_xs: 8.0,
        space_s: 16.0,
        space_m: 26.0,
        space_l: 42.0,
        space_xl: 66.0,
        radius_s: 8.0,
        radius_m: 16.0,
        radius_l: 30.0,
        tempo: 104.0,
        enter_ease: Ease::CubicOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::CubicInOut,
        emphasis_ease: Ease::BackOut,
    },
    Theme {
        name: "tech",
        bg: rgb(0x08, 0x11, 0x1A),
        surface: rgb(0x0E, 0x1B, 0x28),
        text: rgb(0xE6, 0xF1, 0xFF),
        muted: rgb(0x7F, 0x97, 0xAD),
        accent: rgb(0x29, 0xB6, 0xF6),
        accent_2: rgb(0x7C, 0x6C, 0xFF),
        success: rgb(0x2E, 0xD5, 0x9A),
        warning: rgb(0xFF, 0xB0, 0x20),
        danger: rgb(0xFF, 0x5C, 0x6C),
        display: "Inter",
        body: "Inter",
        display_weight: 700,
        heading_weight: 600,
        body_weight: 400,
        label_weight: 600,
        display_size: 96.0,
        h1: 72.0,
        h2: 56.0,
        h3: 40.0,
        body_size: 30.0,
        caption: 22.0,
        label: 16.0,
        heading_tracking: 0.0,
        body_tracking: 0.01,
        space_xs: 8.0,
        space_s: 16.0,
        space_m: 24.0,
        space_l: 40.0,
        space_xl: 64.0,
        radius_s: 6.0,
        radius_m: 12.0,
        radius_l: 20.0,
        tempo: 120.0,
        enter_ease: Ease::ExpoInOut,
        exit_ease: Ease::CubicIn,
        move_ease: Ease::ExpoInOut,
        emphasis_ease: Ease::BackOut,
    },
];

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::rgb(r, g, b)
}

/// The theme used when a program never calls `use_theme` or
/// `comp(theme=...)`.
#[must_use]
pub fn default_theme() -> &'static Theme {
    &THEMES[0]
}

/// Looks up a curated theme by name (case-insensitive).
#[must_use]
pub fn by_name(name: &str) -> Option<&'static Theme> {
    let wanted = name.trim().to_ascii_lowercase();
    THEMES.iter().find(|theme| theme.name == wanted)
}

/// Every curated theme name, in board order.
pub fn names() -> impl Iterator<Item = &'static str> {
    THEMES.iter().map(|theme| theme.name)
}

/// A colour token name, for error suggestions.
const COLOR_TOKENS: [&str; 9] = [
    "bg", "surface", "text", "muted", "accent", "accent_2", "success", "warning", "danger",
];

/// A size token name.
const SIZE_TOKENS: [&str; 7] = ["display", "h1", "h2", "h3", "body", "caption", "label"];

/// A spacing token name.
const SPACING_TOKENS: [&str; 5] = ["xs", "s", "m", "l", "xl"];

/// A radius token name.
const RADIUS_TOKENS: [&str; 3] = ["radius_s", "radius_m", "radius_l"];

/// A motion token name.
const MOTION_TOKENS: [&str; 4] = ["enter", "exit", "move", "emphasis"];

impl Theme {
    /// Resolves a colour token (`"accent"`, `"text"`, ...).
    #[must_use]
    pub fn color(&self, token: &str) -> Option<Color> {
        Some(match token {
            "bg" => self.bg,
            "surface" => self.surface,
            "text" => self.text,
            "muted" => self.muted,
            "accent" => self.accent,
            "accent_2" => self.accent_2,
            "success" => self.success,
            "warning" => self.warning,
            "danger" => self.danger,
            _ => return None,
        })
    }

    /// Resolves a type-scale token to pixels at a composition height.
    ///
    /// The scale is authored for 1080p and scales linearly with the
    /// composition height, so a 4K render keeps the same visual hierarchy.
    #[must_use]
    pub fn size(&self, token: &str, composition_height: f64) -> Option<f64> {
        let base = match token {
            "display" => self.display_size,
            "h1" => self.h1,
            "h2" => self.h2,
            "h3" => self.h3,
            "body" => self.body_size,
            "caption" => self.caption,
            "label" => self.label,
            _ => return None,
        };
        Some(base * composition_height / 1080.0)
    }

    /// Resolves a spacing token to pixels at a composition height.
    #[must_use]
    pub fn space(&self, token: &str, composition_height: f64) -> Option<f64> {
        let base = match token {
            "xs" => self.space_xs,
            "s" => self.space_s,
            "m" => self.space_m,
            "l" => self.space_l,
            "xl" => self.space_xl,
            _ => return None,
        };
        Some(base * composition_height / 1080.0)
    }

    /// Resolves a radius token to pixels at a composition height.
    #[must_use]
    pub fn radius(&self, token: &str, composition_height: f64) -> Option<f64> {
        let base = match token {
            "radius_s" | "s" => self.radius_s,
            "radius_m" | "m" => self.radius_m,
            "radius_l" | "l" => self.radius_l,
            _ => return None,
        };
        Some(base * composition_height / 1080.0)
    }

    /// Resolves a motion token to an ease.
    #[must_use]
    pub fn ease(&self, token: &str) -> Option<Ease> {
        Some(match token {
            "enter" => self.enter_ease,
            "exit" => self.exit_ease,
            "move" => self.move_ease,
            "emphasis" => self.emphasis_ease,
            _ => return None,
        })
    }

    /// Resolves a font token to a family name.
    #[must_use]
    pub fn font(&self, token: &str) -> Option<&'static str> {
        match token {
            "display" => Some(self.display),
            "body" => Some(self.body),
            _ => None,
        }
    }

    /// The WCAG AA ratio between `text` and `bg` and `text` and `surface`.
    ///
    /// # Errors
    ///
    /// Returns the failing pair, its measured ratio and the required 4.5:1
    /// when the theme is not accessible.
    pub fn validate_contrast(&self) -> Result<(), ContrastError> {
        for (pair, background) in [("text/bg", self.bg), ("text/surface", self.surface)] {
            let ratio = contrast_ratio(self.text, background);
            if ratio < 4.5 {
                return Err(ContrastError {
                    theme: self.name,
                    pair,
                    ratio,
                });
            }
        }
        Ok(())
    }
}

/// A theme that does not meet WCAG AA.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastError {
    /// Theme name.
    pub theme: &'static str,
    /// The failing pair (`text/bg` or `text/surface`).
    pub pair: &'static str,
    /// The measured contrast ratio.
    pub ratio: f64,
}

impl std::fmt::Display for ContrastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "theme {:?} fails WCAG AA for {}: measured {:.2}:1, required 4.5:1",
            self.theme, self.pair, self.ratio
        )
    }
}

impl std::error::Error for ContrastError {}

/// WCAG relative luminance of an sRGB colour.
#[must_use]
pub fn relative_luminance(color: Color) -> f64 {
    let channel = |value: u8| f64::from(Color::srgb_to_linear(value));
    0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
}

/// WCAG contrast ratio between two colours, from 1:1 to 21:1.
#[must_use]
pub fn contrast_ratio(a: Color, b: Color) -> f64 {
    let (light, dark) = {
        let (a, b) = (relative_luminance(a), relative_luminance(b));
        if a >= b {
            (a, b)
        } else {
            (b, a)
        }
    };
    (light + 0.05) / (dark + 0.05)
}

/// The token closest to `token`, or the first candidate.
#[must_use]
pub fn nearest(token: &str, candidates: &[&'static str]) -> String {
    candidates
        .iter()
        .min_by_key(|candidate| edit_distance(token, candidate))
        .copied()
        .unwrap_or("?")
        .to_owned()
}

/// The colour tokens, for suggested fixes.
#[must_use]
pub fn color_token_suggestion(token: &str) -> String {
    nearest(token, &COLOR_TOKENS)
}

/// The size tokens, for suggested fixes.
#[must_use]
pub fn size_token_suggestion(token: &str) -> String {
    nearest(token, &SIZE_TOKENS)
}

/// The spacing tokens, for suggested fixes.
#[must_use]
pub fn spacing_token_suggestion(token: &str) -> String {
    nearest(token, &SPACING_TOKENS)
}

/// The radius tokens, for suggested fixes.
#[must_use]
pub fn radius_token_suggestion(token: &str) -> String {
    nearest(token, &RADIUS_TOKENS)
}

/// The motion tokens, for suggested fixes.
#[must_use]
pub fn motion_token_suggestion(token: &str) -> String {
    nearest(token, &MOTION_TOKENS)
}

/// Plain Levenshtein distance, used only for suggestions.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_meets_wcag_aa() {
        for theme in THEMES {
            theme
                .validate_contrast()
                .unwrap_or_else(|error| panic!("{error}"));
        }
    }

    #[test]
    fn token_names_are_unique_and_resolve() {
        let mut names: Vec<&str> = THEMES.iter().map(|theme| theme.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), THEMES.len());
        let dark = by_name("DARK").unwrap();
        assert_eq!(dark.name, "dark");
        assert_eq!(dark.color("accent"), Some(dark.accent));
        assert_eq!(dark.color("nope"), None);
        assert_eq!(dark.size("h1", 1080.0), Some(dark.h1));
        assert_eq!(dark.size("h1", 2160.0), Some(dark.h1 * 2.0));
    }

    #[test]
    fn tokens_suggest_the_nearest_name() {
        assert_eq!(color_token_suggestion("acent"), "accent");
        assert!(matches!(
            size_token_suggestion("h4").as_str(),
            "h1" | "h2" | "h3"
        ));
        assert_eq!(spacing_token_suggestion("med"), "m");
    }

    #[test]
    fn contrast_ratio_matches_wcag_extremes() {
        let white = Color::WHITE;
        let black = Color::BLACK;
        assert!((contrast_ratio(white, black) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 0.001);
    }
}
