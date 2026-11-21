//! The one kind → property-set table.
//!
//! Visual properties (blend, effects, masks, transform) belong to layers that
//! draw pixels. Audio properties belong to layers that contribute samples.
//! A video layer owns both sets. Keeping the table in one place is what stops
//! the inspector and the timeline from disagreeing about which rows a kind
//! has.

use motioner_core::NodeContent;

/// Which property groups a node kind owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PropertySet {
    /// Blend mode, effects, masks and the transform rows.
    pub visual: bool,
    /// Source, start, in/out point, gain, pan, fades, mute and solo.
    pub audio: bool,
}

impl PropertySet {
    /// The set of a node's content kind. This is the single table.
    #[must_use]
    pub const fn of(content: &NodeContent) -> Self {
        match content {
            NodeContent::Audio(_) => Self {
                visual: false,
                audio: true,
            },
            NodeContent::Video(_) => Self {
                visual: true,
                audio: true,
            },
            NodeContent::Group
            | NodeContent::Rect { .. }
            | NodeContent::Circle { .. }
            | NodeContent::Path { .. }
            | NodeContent::Text(_)
            | NodeContent::Image(_) => Self {
                visual: true,
                audio: false,
            },
        }
    }

    /// Whether the kind draws pixels and therefore has visual properties.
    #[must_use]
    pub const fn has_visual(self) -> bool {
        self.visual
    }

    /// Whether the kind contributes samples and therefore has audio properties.
    #[must_use]
    pub const fn has_audio(self) -> bool {
        self.audio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{AudioContent, ImageContent};

    #[test]
    fn audio_has_only_the_audio_set() {
        let content = NodeContent::Audio(AudioContent::new("music.wav", 0));
        let set = PropertySet::of(&content);
        assert!(!set.has_visual());
        assert!(set.has_audio());
    }

    #[test]
    fn visual_kinds_have_no_audio_set() {
        for content in [
            NodeContent::Group,
            NodeContent::rect(
                10.0,
                10.0,
                0.0,
                motioner_core::Paint {
                    fill: None,
                    stroke: None,
                },
            ),
            NodeContent::Image(ImageContent::new("logo.png")),
        ] {
            let set = PropertySet::of(&content);
            assert!(set.has_visual());
            assert!(!set.has_audio());
        }
    }
}
