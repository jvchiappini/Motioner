//! The overrides panel model: one row per pending override, its command text
//! and (when known) the planner's reason.
//!
//! The drawing lives in `panels::overrides`; the conversion action lives on
//! [`Editor::convert_override`](crate::Editor::convert_override). Splitting
//! the model from the drawing keeps the future `GUIMaker` port to one file.

use motioner_core::{
    format_number, quote, OverrideAction, OverrideEntry, OverrideSet, OverrideValue,
};

/// One pending override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideRow {
    /// Node identity the override targets.
    pub id: String,
    /// Property it sets.
    pub property: String,
    /// The equivalent D-0024 command, in its text form.
    pub command: String,
    /// Why it could not be written as code, when the last commit knew.
    pub reason: Option<String>,
}

impl OverrideRow {
    /// Builds a row from an override entry.
    #[must_use]
    pub fn new(entry: &OverrideEntry, reason: Option<String>) -> Self {
        Self {
            id: entry.id.clone(),
            property: entry.property.clone(),
            command: command_text(entry),
            reason,
        }
    }
}

/// Every pending override as a row, in the set's order.
///
/// `reasons` are the last commit's fallback reasons; a row keeps the first
/// reason that mentions its identity or property.
#[must_use]
pub fn rows(set: &OverrideSet, reasons: &[String]) -> Vec<OverrideRow> {
    set.entries()
        .iter()
        .map(|entry| {
            let reason = reasons
                .iter()
                .find(|reason| reason.contains(&entry.id) || reason.contains(&entry.property))
                .cloned();
            OverrideRow::new(entry, reason)
        })
        .collect()
}

/// The command text one override stands for.
#[must_use]
pub fn command_text(entry: &OverrideEntry) -> String {
    let id = quote(&entry.id);
    let property = quote(&entry.property);
    match &entry.action {
        OverrideAction::Set(value) => {
            format!("set({id}, {property}, {})", value_text(value))
        }
        OverrideAction::Key { frame, value, ease } => {
            let ease = if *ease == motioner_core::Ease::Linear {
                String::new()
            } else {
                format!(", ease={}", ease.mdf_literal())
            };
            format!(
                "key({id}, {property}, frame({frame}), {}{ease})",
                value_text(value)
            )
        }
        OverrideAction::Visible(on) => format!("visible({id}, {})", bool_text(*on)),
        OverrideAction::Locked(on) => format!("lock({id}, {})", bool_text(*on)),
        OverrideAction::Unkey { frame } => format!("unkey({id}, {property}, frame({frame}))"),
    }
}

fn bool_text(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn value_text(value: &OverrideValue) -> String {
    match value {
        OverrideValue::Number(number) => format_number(*number),
        OverrideValue::Vec2(v) => format!("({}, {})", format_number(v.x), format_number(v.y)),
        OverrideValue::Text(text) => quote(text),
    }
}

#[cfg(test)]
mod tests {
    use super::{command_text, rows};
    use motioner_core::{Ease, OverrideAction, OverrideEntry, OverrideSet, OverrideValue};

    #[test]
    fn every_entry_has_a_readable_command() {
        let entry = OverrideEntry::new(
            "title".to_owned(),
            "opacity".to_owned(),
            OverrideAction::Key {
                frame: 48,
                value: OverrideValue::Number(0.5),
                ease: Ease::QuadOut,
            },
        );
        assert_eq!(
            command_text(&entry),
            "key(\"title\", \"opacity\", frame(48), 0.5, ease=\"quad_out\")"
        );
        let mut set = OverrideSet::new();
        set.insert(entry);
        let pending = rows(&set, &["the layer is created in a loop".to_owned()]);
        assert_eq!(pending.len(), 1);
        assert!(pending[0].reason.is_none(), "no reason mentions the node");
        let pending = rows(&set, &["title: loop".to_owned()]);
        assert_eq!(pending[0].reason.as_deref(), Some("title: loop"));
    }
}
