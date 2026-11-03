//! The canonical text form of commands: MDF calls that parse back to the
//! same [`Command`].

use std::fmt;

use motioner_core::{format_number, quote, OverrideValue};

use super::{Command, Time};

impl Time {
    fn to_text(self) -> String {
        match self {
            Self::Seconds(seconds) => format_number(seconds),
            Self::Frame(frame) => format!("frame({frame})"),
        }
    }
}

impl Command {
    /// The canonical text form; [`crate::parse_commands`] reads it back unchanged.
    #[must_use]
    #[allow(clippy::too_many_lines)] // one arm per command kind
    pub fn to_text(&self) -> String {
        let name = self.kind().name();
        let args: Vec<String> = match self {
            Self::Play
            | Self::Pause
            | Self::TogglePlay
            | Self::GoToStart
            | Self::GoToEnd
            | Self::Deselect
            | Self::ClearKeys
            | Self::CopyId
            | Self::Reload
            | Self::Quit
            | Self::AddKeys
            | Self::DeleteKeys
            | Self::Undo
            | Self::Redo
            | Self::GraphClose
            | Self::ZoomFit => Vec::new(),
            Self::Step(n) | Self::Seek(n) | Self::SelectRelative(n) | Self::RetimeKeys(n) => {
                vec![n.to_string()]
            }
            Self::Loop(on)
            | Self::ExpandAll(on)
            | Self::SafeArea(on)
            | Self::Snap(on)
            | Self::OnlyAnimated(on)
            | Self::MotionBlurPreview { on }
            | Self::ShowShy(on) => vec![bool_text(*on)],
            Self::Zoom(factor) => vec![format_number(*factor)],
            Self::ZoomBy { steps, x, y } => {
                let mut args = vec![steps.to_string()];
                if x.is_some() || y.is_some() {
                    args.push(format_number(x.unwrap_or(0.0)));
                    args.push(format_number(y.unwrap_or(0.0)));
                }
                args
            }
            Self::Pan { dx, dy } => vec![format_number(*dx), format_number(*dy)],
            Self::EffectAdd { id, kind } | Self::EffectRemove { id, kind } => {
                vec![quote(id), quote(kind.name())]
            }
            Self::EffectMove { id, kind, index } => {
                vec![quote(id), quote(kind.name()), index.to_string()]
            }
            Self::MaskSet {
                id,
                source,
                mode,
                invert,
            } => vec![
                quote(id),
                quote(source),
                quote(mode.name()),
                bool_text(*invert),
            ],
            Self::TextAnimAdd {
                id,
                property,
                from,
                to,
                start,
                stagger,
                duration,
                ease,
                order,
            } => vec![
                quote(id),
                quote(property.name()),
                value_text(from),
                value_text(to),
                start.to_text(),
                format_number(*stagger),
                format_number(*duration),
                format!("ease={}", quote(ease.name())),
                format!("order={}", quote(order.name())),
            ],
            Self::TextAnimRemove { id, index } => vec![quote(id), index.to_string()],
            Self::PrevKey { id, property }
            | Self::NextKey { id, property }
            | Self::Stopwatch { id, property }
            | Self::ToggleKey { id, property }
            | Self::GraphOpen { id, property } => vec![quote(id), quote(property)],
            Self::Select(id) | Self::MaskClear { id } => vec![quote(id)],
            Self::Expand { id, open } => vec![quote(id), bool_text(*open)],
            Self::SelectKeys { keys, add } => {
                let list: Vec<String> = keys
                    .iter()
                    .map(|key| {
                        format!(
                            "({}, {}, {})",
                            quote(&key.id),
                            quote(&key.property),
                            key.time.to_text()
                        )
                    })
                    .collect();
                vec![
                    format!("[{}]", list.join(", ")),
                    format!("add={}", bool_text(*add)),
                ]
            }
            Self::PanelSize { panel, size } => vec![quote(panel.name()), format_number(*size)],
            Self::Scroll { panel, offset } => vec![quote(panel.name()), format_number(*offset)],
            Self::Set {
                id,
                property,
                value,
            }
            | Self::Edit {
                id,
                property,
                value,
            } => vec![quote(id), quote(property), value_text(value)],
            Self::Key {
                id,
                property,
                time,
                value,
                ease,
            } => {
                let mut args = vec![
                    quote(id),
                    quote(property),
                    time.to_text(),
                    value_text(value),
                ];
                if let Some(ease) = ease {
                    args.push(format!("ease={}", ease.mdf_literal()));
                }
                args
            }
            Self::Unkey { id, property, time } => {
                vec![quote(id), quote(property), time.to_text()]
            }
            Self::MoveKey {
                id,
                property,
                from,
                to,
            } => vec![quote(id), quote(property), from.to_text(), to.to_text()],
            Self::Ease {
                id,
                property,
                time,
                ease,
            } => vec![
                quote(id),
                quote(property),
                time.to_text(),
                ease.mdf_literal(),
            ],
            Self::SetEase(ease) => vec![ease.mdf_literal()],
            Self::Visible { id, visible: on } | Self::Lock { id, locked: on } => {
                vec![quote(id), bool_text(*on)]
            }
            Self::Rename { id, name } => vec![quote(id), quote(name)],
            Self::AssignId { from, to } => vec![quote(from), quote(to)],
            Self::DeleteOrphan { id, property, time } => {
                let mut args = vec![quote(id), quote(property)];
                if let Some(time) = time {
                    args.push(time.to_text());
                }
                args
            }
            Self::Solo { id, on } | Self::Shy { id, on } => vec![quote(id), bool_text(*on)],
            Self::FilterLayers(text) => vec![quote(text)],
            Self::GraphView {
                zoom,
                pan_x,
                pan_y,
                zoom_y,
            } => {
                let mut args = vec![
                    format_number(*zoom),
                    format_number(*pan_x),
                    format_number(*pan_y),
                ];
                if let Some(zoom_y) = zoom_y {
                    args.push(format!("zoom_y={}", format_number(*zoom_y)));
                }
                args
            }
            Self::MoveKeyValue {
                id,
                property,
                from,
                to,
                value,
            } => vec![
                quote(id),
                quote(property),
                from.to_text(),
                to.to_text(),
                value_text(value),
            ],
            Self::Bake(ids) => ids.iter().map(|id| quote(id)).collect(),
        };
        format!("{name}({})", args.join(", "))
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_text())
    }
}

fn bool_text(value: bool) -> String {
    if value { "true" } else { "false" }.to_owned()
}

fn value_text(value: &OverrideValue) -> String {
    match value {
        OverrideValue::Number(n) => format_number(*n),
        OverrideValue::Vec2(v) => format!("({}, {})", format_number(v.x), format_number(v.y)),
        OverrideValue::Text(text) => quote(text),
    }
}
