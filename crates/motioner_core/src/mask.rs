//! Masks and track mattes (D-0033).
//!
//! A mask ties a node to a *source* node whose drawn subtree becomes the
//! coverage of the masked node's offscreen layer.  The source is not drawn on
//! its own; it is evaluated in world space with its own animated transform, so
//! a source that is a child of the masked node moves with it.  Feathering is a
//! `blur` effect on the source, exactly like any other effect.
//!
//! The source may be swapped by an editor override (`mask`), so it is stored as
//! a [`MaskSpec`] on the node rather than baked into the content.

use crate::node::NodeId;

/// How a mask source becomes coverage (`alpha` or `luma`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum MaskMode {
    /// Use the source's alpha channel.
    #[default]
    Alpha,
    /// Use the source's linear-light luminance.
    Luma,
}

impl MaskMode {
    /// Every mode, in a stable order.
    pub const ALL: [Self; 2] = [Self::Alpha, Self::Luma];

    /// Parses an MDF mode name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|mode| mode.name().eq_ignore_ascii_case(name))
    }

    /// The MDF name of this mode.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Alpha => "alpha",
            Self::Luma => "luma",
        }
    }

    /// A short human label used by the editor.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Alpha => "Alpha",
            Self::Luma => "Luma",
        }
    }
}

/// One node's mask: the source, how its pixels become coverage and whether the
/// coverage is inverted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskSpec {
    /// The node whose drawn subtree supplies the coverage.
    pub source: NodeId,
    /// Alpha or luma coverage.
    pub mode: MaskMode,
    /// `true` swaps coverage for `1 - coverage`.
    pub invert: bool,
}

impl MaskSpec {
    /// Builds a non-inverted mask.
    #[must_use]
    pub const fn new(source: NodeId, mode: MaskMode) -> Self {
        Self {
            source,
            mode,
            invert: false,
        }
    }

    /// The canonical override value: `"<source>:<mode>:<invert>"`.
    ///
    /// The source is written by identity, so the value is stable across
    /// sessions and readable by `query`.
    #[must_use]
    pub fn override_text(&self, source_identity: &str) -> String {
        format!(
            "{source_identity}:{}:{}",
            self.mode.name(),
            if self.invert { "true" } else { "false" }
        )
    }

    /// The inverse of [`Self::override_text`]: a source identity, a mode and a
    /// boolean.  `None` for `"none"`/empty (no mask).
    ///
    /// # Errors
    ///
    /// Returns the reason the text is not a mask value.
    pub fn parse_override(text: &str) -> Result<Option<(&str, MaskMode, bool)>, &'static str> {
        let text = text.trim();
        if text.is_empty() || text.eq_ignore_ascii_case("none") {
            return Ok(None);
        }
        let mut parts = text.rsplitn(3, ':');
        let invert = parts.next().ok_or("expected '<source>:<mode>:<invert>'")?;
        let mode = parts.next().ok_or("expected '<source>:<mode>:<invert>'")?;
        let source = parts.next().ok_or("expected '<source>:<mode>:<invert>'")?;
        let invert = match invert {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return Err("the third field must be true or false"),
        };
        let mode = MaskMode::from_name(mode).ok_or("the mode must be alpha or luma")?;
        if source.is_empty() {
            return Err("the source identity must not be empty");
        }
        Ok(Some((source, mode, invert)))
    }
}
