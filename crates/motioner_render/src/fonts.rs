//! Font lookup and CPU text measurement (D-0034).
//!
//! A [`FontSet`] is an ordered list of static faces. Bundled faces are always
//! present; a project can prepend faces loaded from `assets/fonts/`.
//! [`CpuTextMetrics`] implements `motioner_core::TextMetrics` on top of the
//! same `ferrous_font` parser the renderer bakes atlases from, so measurement
//! needs no GPU and no window.

use std::fmt;

use motioner_core::{TextAlign, TextBounds, TextMeasurement, TextMetrics, TextRequest};

/// Inter Regular (400), bundled so a default document always has a real face.
pub(crate) const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
/// Inter semibold (600).
pub(crate) const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
/// Inter Bold (700).
pub(crate) const INTER_BOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-Bold.ttf");

/// One static font face: a family name, a weight and the file bytes.
#[derive(Clone)]
pub struct FontFace {
    /// Family name as written in MDF (`"Inter"`).
    pub family: String,
    /// Weight in 100..=900.
    pub weight: u16,
    /// TTF/OTF bytes.
    pub bytes: Vec<u8>,
}

/// A font family missing from both the project and the bundled set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamilyError {
    /// The requested family.
    pub family: String,
    /// Everything that was searched, in order, for the message.
    pub searched: Vec<String>,
}

impl fmt::Display for FontFamilyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "font family {:?} was not found; searched {}",
            self.family,
            self.searched.join(", ")
        )
    }
}

impl std::error::Error for FontFamilyError {}

/// A non-fatal font resolution warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWarning {
    /// The family exists but not at the requested weight; the nearest static
    /// face is used. The family is the one in the request.
    NearestWeight {
        /// Requested weight.
        requested: u16,
        /// Weight actually used.
        used: u16,
    },
}

/// A resolved face borrowed from a [`FontSet`].
#[derive(Debug, Clone, Copy)]
pub struct ResolvedFace<'a> {
    /// Family actually used.
    pub family: &'a str,
    /// Weight actually used.
    pub weight: u16,
    /// File bytes.
    pub bytes: &'a [u8],
    /// Non-fatal resolution warning, if any.
    pub warning: Option<FontWarning>,
}

/// One or more static faces, project faces first, bundled faces last.
#[derive(Clone, Default)]
pub struct FontSet {
    faces: Vec<FontFace>,
    /// Description of the project location, for missing-family errors.
    project_label: Option<String>,
}

impl FontSet {
    /// The faces every renderer ships: Inter 400/600/700 and Roboto 400.
    #[must_use]
    pub fn bundled() -> Self {
        let mut set = Self::default();
        set.push_face("Inter", 400, INTER_REGULAR.to_vec());
        set.push_face("Inter", 600, INTER_SEMIBOLD.to_vec());
        set.push_face("Inter", 700, INTER_BOLD.to_vec());
        set.push_face("Roboto", 400, crate::ROBOTO.to_vec());
        set
    }

    /// Adds one face. A later face with the same `(family, weight)` replaces
    /// an earlier one, so a project face wins over a bundled one.
    pub fn push_face(&mut self, family: impl Into<String>, weight: u16, bytes: Vec<u8>) {
        self.faces.push(FontFace {
            family: family.into(),
            weight,
            bytes,
        });
    }

    /// Records the project font directory used in missing-family errors.
    pub fn set_project_label(&mut self, label: impl Into<String>) {
        self.project_label = Some(label.into());
    }

    /// Every family and weight, for the inspector picker and diagnostics.
    pub fn faces(&self) -> impl Iterator<Item = (&str, u16)> {
        self.faces
            .iter()
            .map(|face| (face.family.as_str(), face.weight))
    }

    /// Every face, bytes included.
    pub fn all(&self) -> impl Iterator<Item = &FontFace> {
        self.faces.iter()
    }

    /// Resolves a family and weight.
    ///
    /// A family present without the exact weight falls back to the nearest
    /// weight (ties choose the lower weight) and returns a
    /// [`FontWarning::NearestWeight`]. Only a missing family is an error.
    ///
    /// # Errors
    ///
    /// Returns [`FontFamilyError`] when no face has the requested family.
    pub fn resolve(&self, family: &str, weight: u16) -> Result<ResolvedFace<'_>, FontFamilyError> {
        let same_family = |face: &&FontFace| face.family.eq_ignore_ascii_case(family);
        if let Some(face) = self
            .faces
            .iter()
            .rev()
            .find(|face| same_family(face) && face.weight == weight)
        {
            return Ok(ResolvedFace {
                family: &face.family,
                weight: face.weight,
                bytes: &face.bytes,
                warning: None,
            });
        }
        let nearest = self
            .faces
            .iter()
            .rev()
            .filter(same_family)
            .min_by_key(|face| (face.weight.abs_diff(weight), face.weight));
        match nearest {
            Some(face) => Ok(ResolvedFace {
                family: &face.family,
                weight: face.weight,
                bytes: &face.bytes,
                warning: Some(FontWarning::NearestWeight {
                    requested: weight,
                    used: face.weight,
                }),
            }),
            None => Err(FontFamilyError {
                family: family.to_owned(),
                searched: self.searched(family, weight),
            }),
        }
    }

    /// The list used in a missing-family message.
    fn searched(&self, family: &str, weight: u16) -> Vec<String> {
        let project = self
            .project_label
            .clone()
            .unwrap_or_else(|| "assets/fonts".to_owned());
        let mut out = vec![format!("{project}/{family}-<weight>.ttf")];
        let mut bundled: Vec<String> = self
            .faces
            .iter()
            .map(|face| format!("{} {}", face.family, face.weight))
            .collect();
        bundled.dedup();
        if !bundled.is_empty() {
            out.push(format!("bundled: {}", bundled.join(", ")));
        }
        out.push(format!("(requested weight {weight})"));
        out
    }
}

/// CPU text measurement backed by a [`FontSet`] (D-0034).
#[derive(Clone, Default)]
pub struct CpuTextMetrics {
    fonts: FontSet,
}

impl CpuTextMetrics {
    /// Wraps a font set.
    #[must_use]
    pub fn new(fonts: FontSet) -> Self {
        Self { fonts }
    }

    /// The wrapped font set.
    #[must_use]
    pub fn fonts(&self) -> &FontSet {
        &self.fonts
    }
}

impl TextMetrics for CpuTextMetrics {
    fn measure(&self, request: &TextRequest<'_>) -> Option<TextMeasurement> {
        let face = self.fonts.resolve(request.font, request.weight).ok()?;
        let parser = ferrous_font::FontParser::new(face.bytes.to_vec()).ok()?;
        Some(measure_parser(&parser, request))
    }
}

/// Measures with the parser directly, mirroring `ferrous_font::layout`'s
/// block arithmetic (the pinned engine has no block layout yet).
#[allow(clippy::too_many_lines)] // one linear pass per line: advance, ink, align
fn measure_parser(parser: &ferrous_font::FontParser, request: &TextRequest<'_>) -> TextMeasurement {
    let size = if request.size.is_finite() && request.size > 0.0 {
        request.size
    } else {
        0.0
    };
    let tracking = if request.tracking.is_finite() {
        request.tracking
    } else {
        0.0
    };
    let line_height = if request.line_height.is_finite() && request.line_height > 0.0 {
        request.line_height
    } else {
        1.0
    };
    let metrics = parser.metrics();
    let step = (f64::from(metrics.ascender) - f64::from(metrics.descender)
        + f64::from(metrics.line_gap))
        * size
        * line_height;
    let ascender = f64::from(metrics.ascender) * size;
    let descender = f64::from(metrics.descender) * size;
    let tracking_px = tracking * size;
    let notdef_advance = f64::from(parser.get_glyph_advance_by_index(0)) * size;
    let has_notdef = !parser.get_glyph_outline_by_index(0).is_empty();

    let mut width = 0.0f64;
    let mut ink: Option<TextBounds> = None;
    let mut line_advances = Vec::new();
    let mut line_count = 0u32;

    for (line_index, raw) in request.text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let mut pen = 0.0f64;
        let mut prev: Option<char> = None;
        let mut line_ink: Option<TextBounds> = None;
        let chars: Vec<char> = line.chars().collect();
        let last = chars.len().saturating_sub(1);
        for (index, &c) in chars.iter().enumerate() {
            let after = if index == last { 0.0 } else { tracking_px };
            if c.is_whitespace() {
                let advance = if parser.get_glyph_index(c) != 0 {
                    f64::from(parser.get_glyph_advance(c))
                } else {
                    f64::from(parser.get_glyph_advance(' '))
                };
                let advance = if advance == 0.0 { 0.3 } else { advance };
                pen += advance * size + after;
                prev = None;
                continue;
            }
            let glyph = parser.get_glyph_index(c);
            let missing = glyph == 0;
            let (advance, bounds) = if missing {
                if has_notdef {
                    (notdef_advance, parser.get_glyph_bounds_by_index(0))
                } else {
                    (0.0, None)
                }
            } else {
                (
                    f64::from(parser.get_glyph_advance_by_index(glyph)) * size,
                    parser.get_glyph_bounds_by_index(glyph),
                )
            };
            if let Some(left) = prev {
                pen += f64::from(parser.get_kerning(left, c)) * size;
            }
            if let Some(bounds) = bounds {
                let glyph_ink = TextBounds {
                    left: pen + f64::from(bounds.left) * size,
                    top: -(f64::from(bounds.top) * size),
                    right: pen + f64::from(bounds.right) * size,
                    bottom: -(f64::from(bounds.bottom) * size),
                };
                line_ink = Some(match line_ink {
                    None => glyph_ink,
                    Some(acc) => TextBounds {
                        left: acc.left.min(glyph_ink.left),
                        top: acc.top.min(glyph_ink.top),
                        right: acc.right.max(glyph_ink.right),
                        bottom: acc.bottom.max(glyph_ink.bottom),
                    },
                });
            }
            pen += advance + after;
            prev = Some(c);
        }
        let shift = match request.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -pen * 0.5,
            TextAlign::Right => -pen,
        };
        if let Some(line_ink) = line_ink {
            let shifted = TextBounds {
                left: line_ink.left + shift,
                right: line_ink.right + shift,
                ..line_ink
            };
            ink = Some(match ink {
                None => shifted,
                Some(acc) => TextBounds {
                    left: acc.left.min(shifted.left),
                    top: acc.top.min(shifted.top),
                    right: acc.right.max(shifted.right),
                    bottom: acc.bottom.max(shifted.bottom),
                },
            });
        }
        width = width.max(pen);
        line_advances.push(pen);
        let index = u32::try_from(line_index).unwrap_or(u32::MAX);
        line_count = line_count.max(index.saturating_add(1));
    }

    TextMeasurement {
        width,
        line_count: line_count.max(1),
        height: if size == 0.0 {
            0.0
        } else {
            (ascender - descender) + step * f64::from(line_count.saturating_sub(1))
        },
        ink: ink.unwrap_or_default(),
        line_advances,
    }
}
