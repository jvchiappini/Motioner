//! Inspector fields: the value a field shows, the text it starts editing
//! with, and the command typing into it commits.

use motioner_core::{
    Axis, ContentField, NodeContent, OverrideAction, OverrideValue, Property, TextAlign,
};

use super::{channel_name, channels, Editor};
use crate::command::Command;
use crate::inspect::{self, FieldTarget};

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
                    property,
                    core_axis(property, axis),
                    self.frame(),
                )? {
                    OverrideValue::Number(number) => Some(number),
                    OverrideValue::Vec2(vector) => Some(vector.x),
                    OverrideValue::Text(_) => None,
                }
            }
            FieldTarget::Content(field) => content_number(document.node(node)?.content(), field),
            _ => None,
        }
    }

    /// The text an inspector field starts editing with.
    #[must_use]
    pub fn field_text(&self, target: FieldTarget) -> String {
        if let Some(value) = self.numeric_value(target) {
            let shown = (value * display_factor(target) * 10.0).round() / 10.0;
            return crate::outline::number(shown);
        }
        let Some(node) = self.selected.and_then(|n| self.document()?.node(n)) else {
            return String::new();
        };
        match target {
            FieldTarget::Content(ContentField::Fill) => match node.content() {
                NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. } => {
                    paint.fill.map_or_else(|| "none".to_owned(), inspect::hex)
                }
                _ => "none".to_owned(),
            },
            FieldTarget::Content(ContentField::Stroke) => match node.content() {
                NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. } => paint
                    .stroke
                    .map_or_else(|| "none".to_owned(), |stroke| inspect::hex(stroke.color)),
                _ => "none".to_owned(),
            },
            FieldTarget::Content(ContentField::Color) => match node.content() {
                NodeContent::Text(text) => inspect::hex(text.color),
                _ => String::new(),
            },
            FieldTarget::Content(ContentField::Text) | FieldTarget::Text => match node.content() {
                NodeContent::Text(text) => text.text.clone(),
                _ => String::new(),
            },
            FieldTarget::Align => match node.content() {
                NodeContent::Text(text) => align_name(text.align).to_owned(),
                _ => String::new(),
            },
            FieldTarget::Name => self
                .session
                .overrides()
                .get(node.identity().as_str(), motioner_core::NAME, None)
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
    pub fn field_command(&self, target: FieldTarget, text: &str) -> Option<Command> {
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
                (field.name().to_owned(), OverrideValue::Number(value))
            }
            FieldTarget::Content(field @ (ContentField::Fill | ContentField::Stroke)) => {
                if !trimmed.eq_ignore_ascii_case("none")
                    && motioner_core::Color::parse_hex(trimmed).is_err()
                {
                    return None;
                }
                (
                    field.name().to_owned(),
                    OverrideValue::Text(trimmed.to_owned()),
                )
            }
            FieldTarget::Content(ContentField::Color) => {
                motioner_core::Color::parse_hex(trimmed).ok()?;
                ("color".to_owned(), OverrideValue::Text(trimmed.to_owned()))
            }
            FieldTarget::Content(ContentField::Text) | FieldTarget::Text => {
                ("text".to_owned(), OverrideValue::Text(text.to_owned()))
            }
            FieldTarget::Align => {
                let align = TextAlign::from_name(trimmed)?;
                (
                    "align".to_owned(),
                    OverrideValue::Text(align_name(align).to_owned()),
                )
            }
            FieldTarget::Name => {
                return Some(Command::Rename {
                    id,
                    name: text.to_owned(),
                })
            }
            FieldTarget::Content(_) | FieldTarget::None => return None,
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
    )
}

/// The numeric value a content field holds, when it is a number.
fn content_number(content: &NodeContent, field: ContentField) -> Option<f64> {
    match (content, field) {
        (NodeContent::Rect { width, .. }, ContentField::Width) => Some(*width),
        (NodeContent::Rect { height, .. }, ContentField::Height) => Some(*height),
        (
            NodeContent::Rect { radius, .. } | NodeContent::Circle { radius, .. },
            ContentField::Radius,
        ) => Some(*radius),
        (
            NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. },
            ContentField::StrokeWidth,
        ) => Some(paint.stroke.map_or(0.0, |stroke| stroke.width)),
        (NodeContent::Text(text), ContentField::Size) => Some(text.size),
        _ => None,
    }
}
