//! Inspector fields: the value a field shows, the text it starts editing
//! with, and the command typing into it commits.

use motioner_core::{
    Axis, BezierEase, ContentField, Ease, EffectParam, NodeContent, OverrideAction, OverrideValue,
    Property, TextAlign,
};

use super::{channel_name, channels, Editor};
use crate::command::Command;
use crate::inspect::{self, BezierPart, FieldTarget};

impl Editor {
    /// The numeric value an inspector field shows (in property units).
    #[must_use]
    pub fn numeric_value(&self, target: FieldTarget) -> Option<f64> {
        let document = self.document()?;
        let node = self.selected?;
        match target {
            FieldTarget::Transform(property, axis) => {
                match channels::sampled(
                    document,
                    node,
                    super::Channel::Transform(property, core_axis(property, axis)),
                    self.frame(),
                )? {
                    OverrideValue::Number(number) => Some(number),
                    OverrideValue::Vec2(vector) => Some(vector.x),
                    OverrideValue::Text(_) => None,
                }
            }
            FieldTarget::Content(field) => {
                channels::sampled_number(document, node, field, self.frame())
            }
            FieldTarget::Effect(channel) => match channels::sampled(
                document,
                node,
                super::Channel::Effect(channel),
                self.frame(),
            )? {
                OverrideValue::Number(number) => Some(number),
                OverrideValue::Vec2(vector) => Some(vector.x),
                OverrideValue::Text(_) => None,
            },
            FieldTarget::Bezier(part) => self.bezier_value(part),
            _ => None,
        }
    }

    /// The shared cubic-bezier ease of the selected keys.
    ///
    /// `None` when nothing is selected, a key uses a named ease, or the
    /// selected keys disagree.
    #[must_use]
    pub fn selected_bezier(&self) -> Option<BezierEase> {
        let mut eases = self.selected_keys.iter().map(|key| self.key_ease(key));
        let Some(Some(Ease::Bezier(first))) = eases.next() else {
            return None;
        };
        eases
            .all(|ease| ease == Some(Ease::Bezier(first)))
            .then_some(first)
    }

    fn bezier_value(&self, part: BezierPart) -> Option<f64> {
        let (x1, y1, x2, y2) = self.selected_bezier()?.points();
        Some(match part {
            BezierPart::X1 => x1,
            BezierPart::Y1 => y1,
            BezierPart::X2 => x2,
            BezierPart::Y2 => y2,
        })
    }

    /// The text an inspector field starts editing with.
    #[must_use]
    pub fn field_text(&self, target: FieldTarget) -> String {
        if let Some(value) = self.numeric_value(target) {
            let shown = (value * display_factor(target) * 10.0).round() / 10.0;
            return crate::outline::number(shown);
        }
        if matches!(target, FieldTarget::Filter) {
            return self.filter.clone();
        }
        let Some(node) = self.selected else {
            return String::new();
        };
        let Some(document) = self.document() else {
            return String::new();
        };
        match target {
            FieldTarget::Content(field @ (ContentField::Fill | ContentField::Stroke)) => {
                match channels::sampled_colour(document, node, field, self.frame()) {
                    channels::SampledColour::Colour(color) => inspect::hex(color),
                    _ => "none".to_owned(),
                }
            }
            FieldTarget::Content(ContentField::Color) => {
                match channels::sampled_colour(document, node, ContentField::Color, self.frame()) {
                    channels::SampledColour::Colour(color) => inspect::hex(color),
                    _ => String::new(),
                }
            }
            FieldTarget::Content(
                field @ (ContentField::FillStopColor(_) | ContentField::StrokeStopColor(_)),
            ) => match channels::sampled_colour(document, node, field, self.frame()) {
                channels::SampledColour::Colour(color) => inspect::hex(color),
                _ => String::new(),
            },
            FieldTarget::Effect(channel) => match channels::sampled(
                document,
                node,
                super::Channel::Effect(channel),
                self.frame(),
            ) {
                Some(OverrideValue::Text(text)) => text,
                _ => String::new(),
            },
            FieldTarget::Content(ContentField::Text) | FieldTarget::Text => {
                match document.node(node).map(motioner_core::Node::content) {
                    Some(NodeContent::Text(text)) => text.text.replace('\n', "\\n"),
                    _ => String::new(),
                }
            }
            FieldTarget::Font => match document.node(node).map(motioner_core::Node::content) {
                Some(NodeContent::Text(text)) => text.font.clone(),
                _ => String::new(),
            },
            FieldTarget::Weight => match document.node(node).map(motioner_core::Node::content) {
                Some(NodeContent::Text(text)) => text.weight.to_string(),
                _ => String::new(),
            },
            FieldTarget::Align => match document.node(node).map(motioner_core::Node::content) {
                Some(NodeContent::Text(text)) => align_name(text.align).to_owned(),
                _ => String::new(),
            },
            FieldTarget::Content(ContentField::Cap) => {
                match document.node(node).map(motioner_core::Node::content) {
                    Some(NodeContent::Path { geometry, .. }) => geometry.cap().name().to_owned(),
                    _ => String::new(),
                }
            }
            FieldTarget::Content(ContentField::Join) => {
                match document.node(node).map(motioner_core::Node::content) {
                    Some(NodeContent::Path { geometry, .. }) => geometry.join().name().to_owned(),
                    _ => String::new(),
                }
            }
            FieldTarget::Name => self
                .session
                .overrides()
                .get(
                    document.node(node).map_or("", |n| n.identity().as_str()),
                    motioner_core::NAME,
                    None,
                )
                .and_then(|entry| match &entry.action {
                    OverrideAction::Set(OverrideValue::Text(text)) => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// The command typing `text` into a field commits, or `None` when the
    /// text does not parse (the field stays red).
    #[must_use]
    #[allow(clippy::too_many_lines)] // one arm per field kind, kept flat
    pub fn field_command(&self, target: FieldTarget, text: &str) -> Option<Command> {
        if matches!(target, FieldTarget::Filter) {
            return Some(Command::FilterLayers(text.to_owned()));
        }
        let id = self.node_identity(self.selected?);
        let trimmed = text.trim();
        let (property, value) = match target {
            FieldTarget::Transform(property, axis) => {
                let shown: f64 = trimmed.parse().ok().filter(|v: &f64| v.is_finite())?;
                let name = channel_name(property, core_axis(property, axis));
                (name, OverrideValue::Number(shown / display_factor(target)))
            }
            FieldTarget::Content(field) if is_numeric_field(field) => {
                let value: f64 = trimmed.parse().ok().filter(|v: &f64| v.is_finite())?;
                (field.name().clone(), OverrideValue::Number(value))
            }
            FieldTarget::Content(field @ (ContentField::Fill | ContentField::Stroke)) => {
                if !trimmed.eq_ignore_ascii_case("none")
                    && motioner_core::Color::parse_hex(trimmed).is_err()
                {
                    return None;
                }
                (
                    field.name().clone(),
                    OverrideValue::Text(trimmed.to_owned()),
                )
            }
            FieldTarget::Content(ContentField::Color) => {
                motioner_core::Color::parse_hex(trimmed).ok()?;
                ("color".to_owned(), OverrideValue::Text(trimmed.to_owned()))
            }
            FieldTarget::Content(
                field @ (ContentField::FillStopColor(_) | ContentField::StrokeStopColor(_)),
            ) => {
                motioner_core::Color::parse_hex(trimmed).ok()?;
                (field.name(), OverrideValue::Text(trimmed.to_owned()))
            }

            FieldTarget::Effect(channel) if channel.param == EffectParam::Color => {
                motioner_core::Color::parse_hex(trimmed).ok()?;
                (channel.name(), OverrideValue::Text(trimmed.to_owned()))
            }
            FieldTarget::Effect(channel) => {
                let value: f64 = trimmed.parse().ok().filter(|v: &f64| v.is_finite())?;
                (channel.name(), OverrideValue::Number(value))
            }
            FieldTarget::Content(ContentField::Text) | FieldTarget::Text => (
                "text".to_owned(),
                // The inspector shows line breaks as `\n`; commit them as real
                // newlines (D-0034).
                OverrideValue::Text(text.replace("\\n", "\n")),
            ),
            FieldTarget::Font => {
                if trimmed.is_empty() {
                    return None;
                }
                ("font".to_owned(), OverrideValue::Text(trimmed.to_owned()))
            }
            FieldTarget::Weight => {
                let value: f64 = trimmed.parse().ok().filter(|v: &f64| v.is_finite())?;
                if !(1.0..=1000.0).contains(&value) || value.fract() != 0.0 {
                    return None;
                }
                ("weight".to_owned(), OverrideValue::Number(value))
            }
            FieldTarget::Align => {
                let align = TextAlign::from_name(trimmed)?;
                (
                    "align".to_owned(),
                    OverrideValue::Text(align_name(align).to_owned()),
                )
            }
            FieldTarget::Content(field @ (ContentField::Cap | ContentField::Join)) => {
                if field == ContentField::Cap {
                    motioner_core::LineCap::from_name(trimmed)?;
                } else {
                    motioner_core::LineJoin::from_name(trimmed)?;
                }
                (
                    field.name().clone(),
                    OverrideValue::Text(trimmed.to_owned()),
                )
            }
            FieldTarget::Name => {
                return Some(Command::Rename {
                    id,
                    name: text.to_owned(),
                })
            }
            FieldTarget::Bezier(part) => {
                let shown: f64 = trimmed.parse().ok().filter(|v: &f64| v.is_finite())?;
                if part.is_x() && !(0.0..=1.0).contains(&shown) {
                    return None;
                }
                let (x1, y1, x2, y2) = self.selected_bezier()?.points();
                let (x1, y1, x2, y2) = match part {
                    BezierPart::X1 => (shown, y1, x2, y2),
                    BezierPart::Y1 => (x1, shown, x2, y2),
                    BezierPart::X2 => (x1, y1, shown, y2),
                    BezierPart::Y2 => (x1, y1, x2, shown),
                };
                let curve = BezierEase::new(x1, y1, x2, y2).ok()?;
                return Some(Command::SetEase(Ease::Bezier(curve)));
            }
            FieldTarget::Content(_) | FieldTarget::Filter | FieldTarget::None => return None,
        };
        Some(Command::Edit {
            id,
            property,
            value,
        })
    }
}

fn align_name(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

/// The core axis an inspector field edits.
fn core_axis(property: Property, axis: inspect::Axis) -> Option<Axis> {
    match axis {
        inspect::Axis::X if property.is_vector() => Some(Axis::X),
        inspect::Axis::Y if property.is_vector() => Some(Axis::Y),
        _ => None,
    }
}

/// Inspector fields show scale and opacity in percent.
fn display_factor(target: FieldTarget) -> f64 {
    match target {
        FieldTarget::Transform(Property::Scale | Property::Opacity, _) => 100.0,
        _ => 1.0,
    }
}

fn is_numeric_field(field: ContentField) -> bool {
    matches!(
        field,
        ContentField::Width
            | ContentField::Height
            | ContentField::Radius
            | ContentField::StrokeWidth
            | ContentField::Size
            | ContentField::Weight
            | ContentField::Tracking
            | ContentField::LineHeight
            | ContentField::TrimStart
            | ContentField::TrimEnd
            | ContentField::TrimOffset
            | ContentField::Dash
            | ContentField::DashOffset
            | ContentField::FillStopOffset(_)
            | ContentField::StrokeStopOffset(_)
    )
}
