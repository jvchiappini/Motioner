//! The one display-label table for transform and content channels.
//!
//! The inspector and the timeline both read channel names through this module
//! so the two panels can never disagree (for example "Width" versus "width").
//! Identity names used by commands and overrides stay in `motioner_core`
//! (`ContentField::name`, `Property::name`); these labels are display-only.

use motioner_core::{Axis, ContentField, Property, StopPart};

/// The inspector/timeline label of a transform channel.
#[must_use]
pub fn transform_label(property: Property, axis: Option<Axis>) -> String {
    let base = match property {
        Property::Position => "Pos",
        Property::Scale => "Scale",
        Property::Rotation => "Rotation",
        Property::Anchor => "Anchor",
        Property::Opacity => "Opacity",
    };
    match axis {
        Some(axis) => format!("{base} {}", axis.name().to_uppercase()),
        None => base.to_owned(),
    }
}

/// The inspector/timeline label of a content field.
///
/// Gradient stop parts read as "Stop 0 Offset" / "Stop 0 Colour"; every other
/// field has a fixed sentence-case label.
#[must_use]
pub fn content_label(field: ContentField) -> String {
    if let Some((_, part, index)) = field.stop() {
        let part = match part {
            StopPart::Offset => "Offset",
            StopPart::Color => "Colour",
        };
        return format!("Stop {index} {part}");
    }
    match field {
        ContentField::Width => "Width",
        ContentField::Height => "Height",
        ContentField::Radius => "Radius",
        ContentField::StrokeWidth => "Stroke width",
        ContentField::Size => "Size",
        ContentField::Weight => "Weight",
        ContentField::Tracking => "Tracking",
        ContentField::LineHeight => "Line height",
        ContentField::TrimStart => "Trim Start",
        ContentField::TrimEnd => "Trim End",
        ContentField::TrimOffset => "Trim Offset",
        ContentField::Dash => "Dash",
        ContentField::DashOffset => "Dash Offset",
        ContentField::Fill => "Fill",
        ContentField::Stroke => "Stroke",
        ContentField::Color => "Colour",
        ContentField::Text => "Text",
        ContentField::Align => "Align",
        ContentField::Cap => "Cap",
        ContentField::Join => "Join",
        ContentField::Source => "Source",
        ContentField::Fit => "Fit",
        ContentField::InPoint => "In point",
        ContentField::OutPoint => "Out point",
        ContentField::Gain => "Gain",
        ContentField::Pan => "Pan",
        ContentField::Start => "Start",
        ContentField::FadeIn => "Fade in",
        ContentField::FadeOut => "Fade out",
        ContentField::FillStopOffset(_)
        | ContentField::FillStopColor(_)
        | ContentField::StrokeStopOffset(_)
        | ContentField::StrokeStopColor(_) => unreachable!("handled above"),
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_width_and_height_match_the_inspector() {
        assert_eq!(content_label(ContentField::Width), "Width");
        assert_eq!(content_label(ContentField::Height), "Height");
    }

    #[test]
    fn every_content_field_has_a_non_empty_label() {
        for (field, name) in ContentField::ALL {
            let label = content_label(field);
            assert!(!label.is_empty(), "{name}");
            assert_ne!(label, name, "{name} still shows its identity name");
        }
    }

    #[test]
    fn gradient_stops_read_like_the_inspector() {
        assert_eq!(
            content_label(ContentField::stop_field(
                motioner_core::PaintSlot::Fill,
                StopPart::Offset,
                2
            )),
            "Stop 2 Offset"
        );
        assert_eq!(
            content_label(ContentField::stop_field(
                motioner_core::PaintSlot::Stroke,
                StopPart::Color,
                0
            )),
            "Stop 0 Colour"
        );
    }

    #[test]
    fn transform_labels_match_the_inspector() {
        assert_eq!(transform_label(Property::Position, Some(Axis::X)), "Pos X");
        assert_eq!(transform_label(Property::Scale, Some(Axis::Y)), "Scale Y");
        assert_eq!(transform_label(Property::Rotation, None), "Rotation");
        assert_eq!(transform_label(Property::Opacity, None), "Opacity");
    }
}
