//! UI overrides: the editable layer on top of a compiled document (D-0022).
//!
//! An [`OverrideSet`] is a pure, ordered list of edits keyed by node identity.
//! [`OverrideSet::apply`] writes them into a [`Document`] and reports every
//! entry it could not apply as an [`OverrideIssue`] (an orphan or a kind
//! mismatch); nothing is ever dropped. [`OverrideSet::to_mdf`] renders the
//! canonical `<name>.overrides.motion` text.
//!
//! Overrides are the only thing the editor ever produces: the MDF program is
//! changed only by `motioner bake`.

use std::fmt::Write as _;

use crate::animation::{Axis, Ease};
use crate::color::Color;
use crate::document::Document;
use crate::math::Vec2;
use crate::node::{ContentField, ContentValue, Property, PropertyValue, TextAlign};
use crate::path::{LineCap, LineJoin};
use crate::time::{Frame, FrameRate};

/// The property name used by visibility overrides.
pub const VISIBLE: &str = "visible";
/// The property name used by lock overrides (editor-only, no render effect).
pub const LOCKED: &str = "locked";
/// The property name used by display-name overrides (editor-only).
pub const NAME: &str = "name";

/// A value written by an override.
#[derive(Debug, Clone, PartialEq)]
pub enum OverrideValue {
    /// A single number.
    Number(f64),
    /// A 2D vector (`(x, y)` in the file).
    Vec2(Vec2),
    /// Text: a colour `#RRGGBB[AA]`, an align name, the literal string and
    /// display names. The target property decides how to read it.
    Text(String),
}

/// What one override does.
#[derive(Debug, Clone, PartialEq)]
pub enum OverrideAction {
    /// Sets the static value of a property or content field.
    Set(OverrideValue),
    /// Adds or replaces the key at `frame`.
    Key {
        /// Composition frame of the key.
        frame: Frame,
        /// Value at that frame.
        value: OverrideValue,
        /// Ease of the segment arriving at the key.
        ease: Ease,
    },
    /// Removes the key at `frame`, if the program has one.
    Unkey {
        /// Composition frame of the key to remove.
        frame: Frame,
    },
    /// Shows or hides the node (and its descendants).
    Visible(bool),
    /// Locks or unlocks the node for editing.
    Locked(bool),
}

/// One override: identity, property name and action.
#[derive(Debug, Clone, PartialEq)]
pub struct OverrideEntry {
    /// Identity of the target node (explicit id or provenance key).
    pub id: String,
    /// Property name (`pos`, `fill`, `visible`, ...).
    pub property: String,
    /// What to do.
    pub action: OverrideAction,
}

impl OverrideEntry {
    /// Creates an entry.
    #[must_use]
    pub fn new(id: impl Into<String>, property: impl Into<String>, action: OverrideAction) -> Self {
        Self {
            id: id.into(),
            property: property.into(),
            action,
        }
    }

    /// The frame this entry is about, or `None` for untimed actions.
    #[must_use]
    pub fn frame(&self) -> Option<Frame> {
        match self.action {
            OverrideAction::Key { frame, .. } | OverrideAction::Unkey { frame } => Some(frame),
            OverrideAction::Set(_) | OverrideAction::Visible(_) | OverrideAction::Locked(_) => None,
        }
    }
}

/// Why an override could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideIssue {
    /// Identity the entry was keyed by.
    pub id: String,
    /// Property the entry set.
    pub property: String,
    /// Human-readable explanation.
    pub message: String,
}

/// An ordered set of overrides.
///
/// Insertion keeps one entry per (id, property, frame): a later write of the
/// same slot replaces the earlier one, which is what editing the same field
/// twice in the UI means. [`OverrideSet::to_mdf`] sorts a copy, so the text is
/// canonical no matter the insertion order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverrideSet {
    entries: Vec<OverrideEntry>,
}

impl OverrideSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The entries in insertion order.
    #[must_use]
    pub fn entries(&self) -> &[OverrideEntry] {
        &self.entries
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry for one slot, if any.
    #[must_use]
    pub fn get(&self, id: &str, property: &str, frame: Option<Frame>) -> Option<&OverrideEntry> {
        self.entries
            .iter()
            .find(|entry| entry.id == id && entry.property == property && entry.frame() == frame)
    }

    /// Inserts an entry, replacing the one in the same slot.
    pub fn insert(&mut self, entry: OverrideEntry) {
        let slot = (entry.id.clone(), entry.property.clone(), entry.frame());
        self.entries.retain(|existing| {
            (
                existing.id.clone(),
                existing.property.clone(),
                existing.frame(),
            ) != slot
        });
        // A static `set` and a key at the same frame are the same slot from
        // the UI's point of view; the key wins and drops the static value.
        if matches!(entry.action, OverrideAction::Key { .. }) {
            self.entries.retain(|e| {
                !(e.id == entry.id && e.property == entry.property && e.frame().is_none())
            });
        }
        self.entries.push(entry);
    }

    /// Appends an entry without merging.
    ///
    /// The overrides-file parser keeps every statement it reads (a later
    /// statement for the same slot wins when applied), so it needs an append
    /// that never removes an earlier line.
    pub fn push(&mut self, entry: OverrideEntry) {
        self.entries.push(entry);
    }

    /// Removes every entry with `id`.
    pub fn remove_id(&mut self, id: &str) {
        self.entries.retain(|entry| entry.id != id);
    }

    /// Removes one exact entry.
    pub fn remove_entry(&mut self, entry: &OverrideEntry) {
        self.entries.retain(|existing| existing != entry);
    }

    /// Removes every entry for one (id, property, frame) slot.
    pub fn remove_slot(&mut self, id: &str, property: &str, frame: Option<Frame>) {
        self.entries.retain(|entry| {
            !(entry.id == id && entry.property == property && entry.frame() == frame)
        });
    }

    /// Removes the entry at `index` (insertion order).
    pub fn remove_index(&mut self, index: usize) -> Option<OverrideEntry> {
        (index < self.entries.len()).then(|| self.entries.remove(index))
    }

    /// Keeps only the entries whose id is in `ids`.
    pub fn retain_ids(&mut self, ids: &[String]) {
        self.entries.retain(|entry| ids.contains(&entry.id));
    }

    /// Writes every override into `document` and reports what failed.
    ///
    /// Orphans (no node with that identity) and kind mismatches are returned
    /// as issues; the entries stay in the set. `locked` and `name` have no
    /// document effect and are never issues.
    pub fn apply(&self, document: &mut Document) -> Vec<OverrideIssue> {
        let mut issues = Vec::new();
        for index in 0..self.entries.len() {
            if let Some(issue) = self.apply_entry(document, index) {
                issues.push(issue);
            }
        }
        issues
    }

    /// Applies one entry by insertion index, returning its issue if it failed.
    ///
    /// The DSL uses this to attach each warning to the span of the statement
    /// that produced the entry.
    pub fn apply_entry(&self, document: &mut Document, index: usize) -> Option<OverrideIssue> {
        let entry = self.entries.get(index)?;
        apply_entry(document, entry)
            .err()
            .map(|message| OverrideIssue {
                id: entry.id.clone(),
                property: entry.property.clone(),
                message,
            })
    }

    /// The canonical `<name>.overrides.motion` text.
    ///
    /// Entries are sorted by identity, then property, then frame, so the same
    /// set always serialises byte-for-byte the same.
    #[must_use]
    pub fn to_mdf(&self, rate: FrameRate) -> String {
        let mut sorted: Vec<&OverrideEntry> = self.entries.iter().collect();
        sorted.sort_by(|a, b| {
            (
                a.id.as_str(),
                a.property.as_str(),
                a.frame().unwrap_or(Frame::MIN),
            )
                .cmp(&(
                    b.id.as_str(),
                    b.property.as_str(),
                    b.frame().unwrap_or(Frame::MIN),
                ))
        });
        let mut out = String::from(
            "# Motioner overrides: edits made in the editor for this program.\n\
             # Generated file. Re-run `motioner bake` to fold them into the program.\n",
        );
        for entry in sorted {
            let _ = writeln!(out, "{}", entry_to_text(entry, rate));
        }
        out
    }
}

fn apply_entry(document: &mut Document, entry: &OverrideEntry) -> Result<(), String> {
    let Some(node) = document.find(&entry.id) else {
        return Err(format!("no node has the identity {:?}", entry.id));
    };
    let (base, axis) = split_property(&entry.property);
    match &entry.action {
        OverrideAction::Visible(visible) => document
            .set_visible(node, *visible)
            .map_err(|error| error.to_string()),
        OverrideAction::Locked(_) => Ok(()),
        OverrideAction::Set(_) if entry.property == LOCKED || entry.property == NAME => Ok(()),
        OverrideAction::Set(value) => set_value(document, node, &entry.property, value),
        OverrideAction::Key { frame, value, ease } => {
            if let Some(axis) = axis {
                let property = vector_property(base, &entry.property)?;
                let OverrideValue::Number(number) = value else {
                    return Err(format!("'{}.{}' expects a number", base, axis.name()));
                };
                document
                    .add_axis_keyframe(node, property, axis, *frame, *number, *ease)
                    .map_err(|error| error.to_string())
            } else if let Some(property) = Property::from_name(base) {
                let value = property_value(property, value).map_err(|reason| {
                    format!(
                        "'{}' cannot take {}: {reason}",
                        entry.property,
                        describe(value)
                    )
                })?;
                document
                    .add_keyframe(node, property, *frame, value, *ease)
                    .map_err(|error| error.to_string())
            } else if let Some(field) = ContentField::from_name(base) {
                let value = content_value(field, value).map_err(|reason| {
                    format!(
                        "'{}' cannot take {}: {reason}",
                        entry.property,
                        describe(value)
                    )
                })?;
                document
                    .add_content_keyframe(node, field, *frame, value, *ease)
                    .map_err(|error| error.to_string())
            } else {
                Err(format!(
                    "'{}' cannot be animated with override_key",
                    entry.property
                ))
            }
        }
        OverrideAction::Unkey { frame } => {
            if let Some(axis) = axis {
                let property = vector_property(base, &entry.property)?;
                document
                    .remove_axis_keyframe(node, property, axis, *frame)
                    .map_err(|error| error.to_string())
            } else if let Some(property) = Property::from_name(base) {
                document
                    .remove_keyframe(node, property, *frame)
                    .map_err(|error| error.to_string())
            } else if let Some(field) = ContentField::from_name(base) {
                document
                    .remove_content_keyframe(node, field, *frame)
                    .map_err(|error| error.to_string())
            } else {
                Err(format!("'{}' has no keyframes to remove", entry.property))
            }
        }
    }
}

/// Splits a property name into its base and optional axis: `pos.x` becomes
/// (`pos`, `Some(X)`), `opacity` becomes (`opacity`, `None`).
#[must_use]
pub fn split_property(property: &str) -> (&str, Option<Axis>) {
    if let Some((base, suffix)) = property.rsplit_once('.') {
        if let Some(axis) = Axis::from_name(suffix) {
            return (base, Some(axis));
        }
    }
    (property, None)
}

fn vector_property(base: &str, full: &str) -> Result<Property, String> {
    let property = Property::from_name(base).ok_or_else(|| format!("unknown property '{full}'"))?;
    if !property.is_vector() {
        return Err(format!("'{full}' has no axes"));
    }
    Ok(property)
}
fn set_value(
    document: &mut Document,
    node: crate::node::NodeId,
    property_name: &str,
    value: &OverrideValue,
) -> Result<(), String> {
    if let Some(property) = Property::from_name(property_name) {
        let value = property_value(property, value).map_err(|reason| {
            format!(
                "'{property_name}' cannot take {}: {reason}",
                describe(value)
            )
        })?;
        return document
            .set_property(node, property, value)
            .map_err(|error| error.to_string());
    }
    let Some(field) = ContentField::from_name(property_name) else {
        return Err(format!("unknown property '{property_name}'"));
    };
    let value = content_value(field, value).map_err(|reason| {
        format!(
            "'{property_name}' cannot take {}: {reason}",
            describe(value)
        )
    })?;
    document
        .set_content(node, field, value)
        .map_err(|error| error.to_string())
}

fn property_value(
    property: Property,
    value: &OverrideValue,
) -> Result<PropertyValue, &'static str> {
    match (property, value) {
        (Property::Position | Property::Anchor, OverrideValue::Vec2(v)) => {
            Ok(PropertyValue::Vec2(*v))
        }
        (Property::Scale, OverrideValue::Vec2(v)) => Ok(PropertyValue::Vec2(*v)),
        (Property::Scale, OverrideValue::Number(n)) => Ok(PropertyValue::Scalar(*n)),
        (Property::Rotation | Property::Opacity, OverrideValue::Number(n)) => {
            Ok(PropertyValue::Scalar(*n))
        }
        (Property::Position | Property::Anchor, _) => Err("expected an (x, y) pair"),
        (Property::Rotation | Property::Opacity, _) => Err("expected a number"),
        (Property::Scale, OverrideValue::Text(_)) => Err("expected a number or an (x, y) pair"),
    }
}

fn content_value(field: ContentField, value: &OverrideValue) -> Result<ContentValue, &'static str> {
    match (field, value) {
        (
            ContentField::Width
            | ContentField::Height
            | ContentField::Radius
            | ContentField::StrokeWidth
            | ContentField::Size
            | ContentField::TrimStart
            | ContentField::TrimEnd
            | ContentField::TrimOffset
            | ContentField::Dash
            | ContentField::DashOffset
            | ContentField::FillStopOffset(_)
            | ContentField::StrokeStopOffset(_),
            OverrideValue::Number(n),
        ) => Ok(ContentValue::Number(*n)),
        (
            ContentField::FillStopColor(_) | ContentField::StrokeStopColor(_),
            OverrideValue::Text(text),
        ) => Color::parse_hex(text)
            .map(|color| ContentValue::Color(Some(color)))
            .map_err(|_| "expected a colour like \"#RRGGBB\""),
        (ContentField::Fill | ContentField::Stroke, OverrideValue::Text(text)) => {
            if text.eq_ignore_ascii_case("none") {
                Ok(ContentValue::Color(None))
            } else {
                Color::parse_hex(text)
                    .map(|color| ContentValue::Color(Some(color)))
                    .map_err(|_| "expected a colour like \"#RRGGBB\" or \"none\"")
            }
        }
        (ContentField::Color, OverrideValue::Text(text)) => Color::parse_hex(text)
            .map(|color| ContentValue::Color(Some(color)))
            .map_err(|_| "expected a colour like \"#RRGGBB\""),
        (ContentField::Text, OverrideValue::Text(text)) => Ok(ContentValue::Text(text.clone())),
        (ContentField::Text, OverrideValue::Number(n)) => Ok(ContentValue::Text(n.to_string())),
        (ContentField::Align, OverrideValue::Text(text)) => TextAlign::from_name(text)
            .map(ContentValue::Align)
            .ok_or("\"left\", \"center\" or \"right\""),
        (ContentField::Cap, OverrideValue::Text(text)) => LineCap::from_name(text)
            .map(ContentValue::Cap)
            .ok_or("\"butt\", \"round\" or \"square\""),
        (ContentField::Join, OverrideValue::Text(text)) => LineJoin::from_name(text)
            .map(ContentValue::Join)
            .ok_or("\"miter\", \"bevel\" or \"round\""),
        (ContentField::Fill | ContentField::Stroke, OverrideValue::Number(_)) => {
            Err("expected a colour string")
        }
        (ContentField::Align, _) => Err("expected an align name"),
        (ContentField::Cap | ContentField::Join, _) => Err("expected a style name"),
        _ => Err("expected a number"),
    }
}

fn describe(value: &OverrideValue) -> &'static str {
    match value {
        OverrideValue::Number(_) => "a number",
        OverrideValue::Vec2(_) => "an (x, y) pair",
        OverrideValue::Text(_) => "a string",
    }
}

/// Renders the canonical text of `text` as an MDF string literal.
#[must_use]
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Formats a number so that reading it back yields the same value.
#[must_use]
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    if value.fract().abs() < 1e-9 && value.abs() < 1e15 {
        return format!("{value:.0}");
    }
    let mut text = format!("{value:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// Seconds of `frame` at `rate`, formatted for the overrides file.
#[must_use]
pub fn seconds_of(frame: Frame, rate: FrameRate) -> f64 {
    // Frames are bounded by `MAX_ABS_FRAME` (1e9), exactly representable.
    #[allow(clippy::cast_precision_loss)]
    let frame = frame as f64;
    frame * f64::from(rate.denominator()) / f64::from(rate.numerator())
}

fn value_text(value: &OverrideValue) -> String {
    match value {
        OverrideValue::Number(n) => format_number(*n),
        OverrideValue::Vec2(v) => format!("({}, {})", format_number(v.x), format_number(v.y)),
        OverrideValue::Text(text) => quote(text),
    }
}

fn entry_to_text(entry: &OverrideEntry, rate: FrameRate) -> String {
    let id = quote(&entry.id);
    let property = quote(&entry.property);
    match &entry.action {
        OverrideAction::Set(value) => {
            format!("override({id}, {property}, {})", value_text(value))
        }
        OverrideAction::Key { frame, value, ease } => format!(
            "override_key({id}, {property}, {}, {}, ease={})",
            format_number(seconds_of(*frame, rate)),
            value_text(value),
            quote(ease.name())
        ),
        OverrideAction::Unkey { frame } => format!(
            "override_unkey({id}, {property}, {})",
            format_number(seconds_of(*frame, rate))
        ),
        OverrideAction::Visible(visible) => {
            format!(
                "override_visible({id}, {})",
                if *visible { "true" } else { "false" }
            )
        }
        OverrideAction::Locked(locked) => {
            format!(
                "override_locked({id}, {})",
                if *locked { "true" } else { "false" }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Composition;
    use crate::node::{NodeContent, Paint};

    fn document() -> Document {
        let rate = FrameRate::new(60, 1).unwrap();
        Document::new(Composition::new("T", 100, 100, rate, 120, Color::BLACK).unwrap())
    }

    fn dot(doc: &mut Document, id: &str) -> crate::node::NodeId {
        let content = NodeContent::circle(
            10.0,
            Paint {
                fill: Some(Color::WHITE),
                stroke: None,
            },
        );
        doc.add_node_with_identity(content, crate::node::NodeIdentity::explicit(id).unwrap())
            .unwrap()
    }

    fn rate() -> FrameRate {
        FrameRate::new(60, 1).unwrap()
    }

    #[test]
    fn an_empty_set_is_a_no_op() {
        let mut doc = document();
        dot(&mut doc, "a");
        let before = doc.clone();
        assert!(OverrideSet::new().apply(&mut doc).is_empty());
        assert_eq!(doc, before);
        assert_eq!(
            OverrideSet::new().to_mdf(rate()),
            "# Motioner overrides: edits made in the editor for this program.\n\
             # Generated file. Re-run `motioner bake` to fold them into the program.\n"
        );
    }

    #[test]
    fn overrides_write_transform_content_and_visibility() {
        let mut doc = document();
        let id = dot(&mut doc, "dot");
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "dot",
            "pos",
            OverrideAction::Set(OverrideValue::Vec2(Vec2::new(10.0, 20.0))),
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "radius",
            OverrideAction::Set(OverrideValue::Number(12.0)),
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "fill",
            OverrideAction::Set(OverrideValue::Text("#112233".to_owned())),
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "visible",
            OverrideAction::Visible(false),
        ));
        assert!(set.apply(&mut doc).is_empty());
        let node = doc.node(id).unwrap();
        assert_eq!(node.transform().position.value(), Vec2::new(10.0, 20.0));
        assert!(!node.is_visible());
        match node.content() {
            NodeContent::Circle { radius, paint } => {
                assert!((radius.value() - 12.0).abs() < 1e-9);
                assert_eq!(paint.fill.value(), Color::parse_hex("#112233").ok());
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn orphans_and_kind_mismatches_are_reported_and_kept() {
        let mut doc = document();
        dot(&mut doc, "dot");
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "ghost",
            "opacity",
            OverrideAction::Set(OverrideValue::Number(0.5)),
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "text",
            OverrideAction::Set(OverrideValue::Text("x".to_owned())),
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "opacity",
            OverrideAction::Set(OverrideValue::Number(0.5)),
        ));
        let issues = set.apply(&mut doc);
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(issues.iter().any(|i| i.id == "ghost"));
        assert!(issues.iter().any(|i| i.property == "text"));
        assert_eq!(set.len(), 3, "nothing was dropped");
    }

    #[test]
    fn keys_replace_static_values_and_unkey_removes_them() {
        let mut doc = document();
        let id = dot(&mut doc, "dot");
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "dot",
            "rotation",
            OverrideAction::Set(OverrideValue::Number(5.0)),
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "rotation",
            OverrideAction::Key {
                frame: 30,
                value: OverrideValue::Number(90.0),
                ease: Ease::CubicOut,
            },
        ));
        assert!(set.apply(&mut doc).is_empty());
        let keys = doc.node(id).unwrap().transform().rotation.keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].ease, Ease::CubicOut);

        let mut remove = OverrideSet::new();
        remove.insert(OverrideEntry::new(
            "dot",
            "rotation",
            OverrideAction::Unkey { frame: 30 },
        ));
        assert!(remove.apply(&mut doc).is_empty());
        assert!(doc.node(id).unwrap().transform().rotation.keys().is_empty());
    }

    #[test]
    fn axis_keys_are_independent() {
        let mut doc = document();
        let id = dot(&mut doc, "dot");
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "dot",
            "pos.x",
            OverrideAction::Key {
                frame: 0,
                value: OverrideValue::Number(3.0),
                ease: Ease::Linear,
            },
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "pos.y",
            OverrideAction::Key {
                frame: 30,
                value: OverrideValue::Number(9.0),
                ease: Ease::Linear,
            },
        ));
        assert!(set.apply(&mut doc).is_empty());
        let position = &doc.node(id).unwrap().transform().position;
        assert_eq!(position.axis(crate::Axis::X).keys().len(), 1);
        assert_eq!(position.axis(crate::Axis::Y).keys().len(), 1);
        assert_eq!(position.axis(crate::Axis::X).keys()[0].frame, 0);
        assert_eq!(position.axis(crate::Axis::Y).keys()[0].frame, 30);
        // A scalar property rejects an axis.
        let mut bad = OverrideSet::new();
        bad.insert(OverrideEntry::new(
            "dot",
            "opacity.x",
            OverrideAction::Key {
                frame: 0,
                value: OverrideValue::Number(1.0),
                ease: Ease::Linear,
            },
        ));
        assert_eq!(bad.apply(&mut doc).len(), 1);
    }

    #[test]
    fn serialisation_is_canonical_and_deterministic() {
        let mut a = OverrideSet::new();
        a.insert(OverrideEntry::new(
            "b",
            "opacity",
            OverrideAction::Key {
                frame: 60,
                value: OverrideValue::Number(1.0),
                ease: Ease::QuadOut,
            },
        ));
        a.insert(OverrideEntry::new(
            "a",
            "pos",
            OverrideAction::Set(OverrideValue::Vec2(Vec2::new(1.5, -2.0))),
        ));
        a.insert(OverrideEntry::new(
            "a",
            "visible",
            OverrideAction::Visible(true),
        ));
        let mut b = OverrideSet::new();
        for entry in a.entries().iter().rev() {
            b.insert(entry.clone());
        }
        assert_eq!(a.to_mdf(rate()), b.to_mdf(rate()));
        assert_eq!(a.to_mdf(rate()), a.to_mdf(rate()));
        let text = a.to_mdf(rate());
        assert!(
            text.contains("override(\"a\", \"pos\", (1.5, -2))\n"),
            "{text}"
        );
        assert!(text.contains("override_visible(\"a\", true)\n"), "{text}");
        assert!(
            text.contains("override_key(\"b\", \"opacity\", 1, 1, ease=\"quad_out\")\n"),
            "{text}"
        );
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(quote("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "t",
            "text",
            OverrideAction::Set(OverrideValue::Text("a\"b".to_owned())),
        ));
        assert!(set.to_mdf(rate()).contains("\"a\\\"b\""));
    }

    #[test]
    fn numbers_read_back_exactly() {
        for value in [0.0, 1.0, -3.5, 0.000_001, 123_456.75, 1e9] {
            let parsed = format_number(value).parse::<f64>().unwrap();
            assert!((parsed - value).abs() < 1e-6, "{value} -> {parsed}");
        }
    }

    #[test]
    fn content_fields_take_keys_and_serialise() {
        let mut doc = document();
        let id = dot(&mut doc, "dot");
        let red = Color::parse_hex("#FF0000").unwrap();
        let green = Color::parse_hex("#00FF00").unwrap();
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "dot",
            "fill",
            OverrideAction::Key {
                frame: 0,
                value: OverrideValue::Text("#FF0000".to_owned()),
                ease: Ease::Linear,
            },
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "fill",
            OverrideAction::Key {
                frame: 30,
                value: OverrideValue::Text("#00FF00".to_owned()),
                ease: Ease::CubicOut,
            },
        ));
        set.insert(OverrideEntry::new(
            "dot",
            "radius",
            OverrideAction::Key {
                frame: 30,
                value: OverrideValue::Number(20.0),
                ease: Ease::Linear,
            },
        ));
        assert!(set.apply(&mut doc).is_empty());
        let content = doc.node(id).unwrap().content();
        assert_eq!(
            content.channel(ContentField::Fill).unwrap().key_frames(),
            vec![0, 30]
        );
        assert_eq!(
            content.channel(ContentField::Radius).unwrap().key_frames(),
            vec![30]
        );
        let fill_keys = match content.channel(ContentField::Fill).unwrap() {
            crate::node::ContentChannel::OptionalColor(value) => value.keys(),
            _ => panic!("fill is an optional colour"),
        };
        assert_eq!(fill_keys[0].value, Some(red));
        assert_eq!(fill_keys[1].value, Some(green));
        let text = set.to_mdf(rate());
        assert!(
            text.contains("override_key(\"dot\", \"fill\", 0, \"#FF0000\", ease=\"linear\")\n"),
            "{text}"
        );
        assert!(
            text.contains(
                "override_key(\"dot\", \"fill\", 0.5, \"#00FF00\", ease=\"cubic_out\")\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("override_key(\"dot\", \"radius\", 0.5, 20, ease=\"linear\")\n"),
            "{text}"
        );
        let mut remove = OverrideSet::new();
        remove.insert(OverrideEntry::new(
            "dot",
            "fill",
            OverrideAction::Unkey { frame: 30 },
        ));
        assert!(remove.apply(&mut doc).is_empty());
        assert_eq!(
            doc.node(id)
                .unwrap()
                .content()
                .channel(ContentField::Fill)
                .unwrap()
                .key_frames(),
            vec![0]
        );
    }

    #[test]
    fn a_discrete_content_field_cannot_be_keyed() {
        let mut doc = document();
        let id = dot(&mut doc, "dot");
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "dot",
            "text",
            OverrideAction::Key {
                frame: 0,
                value: OverrideValue::Text("x".to_owned()),
                ease: Ease::Linear,
            },
        ));
        let issues = set.apply(&mut doc);
        assert_eq!(issues.len(), 1);
        assert_eq!(set.len(), 1, "the override is kept");
        let _ = id;
    }

    #[test]
    fn path_channels_are_overridable_and_round_trip() {
        let mut doc = document();
        let geometry = crate::path::PathGeometry::new(
            vec![
                crate::path::PathPoint::corner(Vec2::new(0.0, 0.0)),
                crate::path::PathPoint::corner(Vec2::new(40.0, 0.0)),
                crate::path::PathPoint::corner(Vec2::new(40.0, 40.0)),
            ],
            false,
            crate::path::LineCap::Butt,
            crate::path::LineJoin::Miter,
        )
        .unwrap();
        let id = doc
            .add_node_with_identity(
                NodeContent::path(
                    geometry,
                    Paint {
                        fill: None,
                        stroke: Some(crate::node::Stroke {
                            color: Color::WHITE,
                            width: 2.0,
                        }),
                    },
                ),
                crate::node::NodeIdentity::explicit("p").unwrap(),
            )
            .unwrap();
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "p",
            "trim_end",
            OverrideAction::Set(OverrideValue::Number(0.5)),
        ));
        set.insert(OverrideEntry::new(
            "p",
            "trim_start",
            OverrideAction::Key {
                frame: 30,
                value: OverrideValue::Number(0.25),
                ease: Ease::QuadOut,
            },
        ));
        set.insert(OverrideEntry::new(
            "p",
            "cap",
            OverrideAction::Set(OverrideValue::Text("round".to_owned())),
        ));
        set.insert(OverrideEntry::new(
            "p",
            "dash",
            OverrideAction::Set(OverrideValue::Number(6.0)),
        ));
        assert!(set.apply(&mut doc).is_empty());
        let NodeContent::Path {
            geometry,
            trim_start,
            trim_end,
            dash,
            ..
        } = doc.node(id).unwrap().content()
        else {
            panic!("a path");
        };
        assert_eq!(geometry.cap(), crate::path::LineCap::Round);
        assert!((trim_end.value() - 0.5).abs() < 1e-9);
        assert_eq!(trim_start.keys().len(), 1);
        assert!((dash.value() - 6.0).abs() < 1e-9);
        let text = set.to_mdf(rate());
        assert!(
            text.contains("override(\"p\", \"trim_end\", 0.5)"),
            "{text}"
        );
        assert!(
            text.contains("override_key(\"p\", \"trim_start\", 0.5, 0.25, ease=\"quad_out\")"),
            "{text}"
        );
        assert!(
            text.contains("override(\"p\", \"cap\", \"round\")"),
            "{text}"
        );
    }
}
