//! Font lookup and CPU text measurement (D-0034).
//!
//! A [`FontSet`] is an ordered list of static faces. Bundled faces are always
//! present; a project can prepend faces loaded from `assets/fonts/`.
//! [`CpuTextMetrics`] implements `motioner_core::TextMetrics` on top of
//! `ferrous_font::LayoutData`, the same CPU tables the renderer's atlases are
//! built from, so measurement (including GPOS kerning) can never drift from
//! what is drawn.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

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

/// One cached layout table per resolved `(family, weight)`.
type LayoutCache = Arc<Mutex<HashMap<(String, u16), Arc<FaceLayout>>>>;

/// CPU text measurement backed by a [`FontSet`] (D-0034).
///
/// Each resolved face is parsed once and cached; later measurements reuse the
/// tables. Measurement runs through the engine's `ferrous_font::LayoutData`
/// block layout, including GPOS kerning. The cache is shared between clones, so the
/// interpreter thread and the editor can measure through one instance.
#[derive(Clone, Default)]
pub struct CpuTextMetrics {
    fonts: FontSet,
    layouts: LayoutCache,
}

/// The per-face layout source.
type FaceLayout = ferrous_font::LayoutData;

impl CpuTextMetrics {
    /// Wraps a font set.
    #[must_use]
    pub fn new(fonts: FontSet) -> Self {
        Self {
            fonts,
            layouts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The wrapped font set.
    #[must_use]
    pub fn fonts(&self) -> &FontSet {
        &self.fonts
    }

    /// The layout tables of one resolved face, parsed on first use.
    fn layout_for(&self, family: &str, weight: u16, bytes: &[u8]) -> Option<Arc<FaceLayout>> {
        let key = (family.to_owned(), weight);
        {
            let cache = self
                .layouts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(layout) = cache.get(&key) {
                return Some(Arc::clone(layout));
            }
        }
        let layout =
            ferrous_font::LayoutData::from_bytes(bytes, ferrous_font::charset::latin_western())
                .ok()?;
        let layout = Arc::new(layout);
        let mut cache = self
            .layouts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.insert(key, Arc::clone(&layout));
        Some(layout)
    }
}

impl TextMetrics for CpuTextMetrics {
    fn measure(&self, request: &TextRequest<'_>) -> Option<TextMeasurement> {
        let face = self.fonts.resolve(request.font, request.weight).ok()?;
        let layout = self.layout_for(face.family, face.weight, face.bytes)?;
        let style = ferrous_font::TextStyle {
            size_px: to_f32(request.size),
            align: align_to_engine(request.align),
            tracking: to_f32(request.tracking),
            line_height: to_f32(request.line_height),
        };
        let metrics = layout.measure_block(request.text, style);
        Some(TextMeasurement {
            width: f64::from(metrics.width),
            line_count: metrics.line_count,
            height: f64::from(metrics.height),
            ink: if metrics.ink.is_empty() {
                TextBounds::empty()
            } else {
                TextBounds {
                    left: f64::from(metrics.ink.left),
                    top: -f64::from(metrics.ink.top),
                    right: f64::from(metrics.ink.right),
                    bottom: -f64::from(metrics.ink.bottom),
                }
            },
            line_advances: metrics
                .lines
                .iter()
                .map(|line| f64::from(line.advance))
                .collect(),
        })
    }
}

/// Maps a Motioner alignment to the engine's identical-named alignment.
fn align_to_engine(align: TextAlign) -> ferrous_font::TextAlign {
    match align {
        TextAlign::Left => ferrous_font::TextAlign::Left,
        TextAlign::Center => ferrous_font::TextAlign::Center,
        TextAlign::Right => ferrous_font::TextAlign::Right,
    }
}

/// Narrows an `f64` to the engine's `f32` precision.
#[allow(clippy::cast_possible_truncation)] // deliberate f64 -> f32 engine boundary
fn to_f32(value: f64) -> f32 {
    value as f32
}
