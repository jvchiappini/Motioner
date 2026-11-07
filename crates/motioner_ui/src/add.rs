//! The Add Layer menu model: the kinds a person can create without an asset.
//!
//! The drawing lives in `panels::layers`; this module owns the mapping from a
//! menu row to the typed [`Command::AddLayer`] so the future `GUIMaker` port
//! only replaces the drawing code.

use crate::command::Command;
use motioner_dsl::structural::LayerKind;

/// A layer kind the Add Layer menu offers.
///
/// Image, audio and video layers need an asset file, so they are created from
/// the Assets panel (drag onto the canvas or the outline) instead of this menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddKind {
    /// A rectangle.
    Rect,
    /// A circle.
    Circle,
    /// A text layer.
    Text,
    /// A vector path (a triangle).
    Path,
    /// An empty group.
    Group,
}

impl AddKind {
    /// Every kind, in menu order.
    pub const ALL: [Self; 5] = [
        Self::Rect,
        Self::Circle,
        Self::Text,
        Self::Path,
        Self::Group,
    ];

    /// The menu label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rect => "Rectangle",
            Self::Circle => "Circle",
            Self::Text => "Text",
            Self::Path => "Path",
            Self::Group => "Group",
        }
    }

    /// The item at `index`, if any.
    #[must_use]
    pub fn at(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// The constructor data for this kind.
    ///
    /// Sizes are composition-agnostic defaults a person adjusts in the
    /// inspector right after inserting the layer.
    #[must_use]
    pub fn layer_kind(self) -> LayerKind {
        match self {
            Self::Rect => LayerKind::Rect {
                width: 200.0,
                height: 120.0,
            },
            Self::Circle => LayerKind::Circle { radius: 60.0 },
            Self::Text => LayerKind::Text {
                content: "Text".to_owned(),
            },
            Self::Path => LayerKind::Path {
                points: vec![(0.0, 100.0), (100.0, 0.0), (200.0, 100.0)],
            },
            Self::Group => LayerKind::Group,
        }
    }

    /// The command that inserts this kind after `after`.
    #[must_use]
    pub fn command(self, after: Option<String>) -> Command {
        Command::AddLayer {
            kind: self.layer_kind(),
            after,
            name: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AddKind;
    use crate::command::CommandKind;

    #[test]
    fn every_menu_row_maps_to_an_add_layer_command() {
        for (index, kind) in AddKind::ALL.into_iter().enumerate() {
            assert_eq!(AddKind::at(index), Some(kind));
            let command = kind.command(Some("title".to_owned()));
            assert_eq!(command.kind(), CommandKind::AddLayer, "{kind:?}");
            assert_eq!(
                command.structural(),
                Some(motioner_dsl::structural::StructuralCommand::AddLayer {
                    kind: kind.layer_kind(),
                    after: Some("title".to_owned()),
                    name: None,
                })
            );
        }
        assert_eq!(AddKind::at(AddKind::ALL.len()), None);
    }
}
