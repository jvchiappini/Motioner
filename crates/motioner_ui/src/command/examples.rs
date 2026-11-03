//! One example per command kind, used by the documentation, `--help` and
//! the parity tests.

use motioner_core::{Ease, OverrideValue, Vec2};

use super::{Command, CommandKind, KeySpec, Panel, Time};

impl CommandKind {
    /// One example command of this kind (documentation and parity tests).
    #[must_use]
    #[allow(clippy::too_many_lines)] // one arm per command kind
    pub fn example(self) -> Command {
        let id = || "title".to_owned();
        let channel = || "pos.x".to_owned();
        match self {
            Self::Play => Command::Play,
            Self::Pause => Command::Pause,
            Self::TogglePlay => Command::TogglePlay,
            Self::Step => Command::Step(-10),
            Self::Seek => Command::Seek(90),
            Self::GoToStart => Command::GoToStart,
            Self::GoToEnd => Command::GoToEnd,
            Self::Loop => Command::Loop(false),
            Self::PrevKey => Command::PrevKey {
                id: id(),
                property: channel(),
            },
            Self::NextKey => Command::NextKey {
                id: id(),
                property: channel(),
            },
            Self::Select => Command::Select(id()),
            Self::Deselect => Command::Deselect,
            Self::SelectRelative => Command::SelectRelative(1),
            Self::Expand => Command::Expand {
                id: id(),
                open: true,
            },
            Self::ExpandAll => Command::ExpandAll(false),
            Self::SelectKeys => Command::SelectKeys {
                keys: vec![
                    KeySpec {
                        id: id(),
                        property: channel(),
                        time: Time::Seconds(0.8),
                    },
                    KeySpec {
                        id: id(),
                        property: "opacity".to_owned(),
                        time: Time::Frame(48),
                    },
                ],
                add: true,
            },
            Self::ClearKeys => Command::ClearKeys,
            Self::SafeArea => Command::SafeArea(true),
            Self::Snap => Command::Snap(false),
            Self::OnlyAnimated => Command::OnlyAnimated(true),
            Self::MotionBlurPreview => Command::MotionBlurPreview { on: true },
            Self::Zoom => Command::Zoom(2.0),
            Self::ZoomFit => Command::ZoomFit,
            Self::ZoomBy => Command::ZoomBy {
                steps: 2,
                x: Some(960.0),
                y: Some(540.0),
            },
            Self::Pan => Command::Pan {
                dx: 40.0,
                dy: -25.5,
            },
            Self::PanelSize => Command::PanelSize {
                panel: Panel::Timeline,
                size: 360.0,
            },
            Self::Scroll => Command::Scroll {
                panel: Panel::Inspector,
                offset: 120.0,
            },
            Self::CopyId => Command::CopyId,
            Self::Reload => Command::Reload,
            Self::Quit => Command::Quit,
            Self::Set => Command::Set {
                id: id(),
                property: "fill".to_owned(),
                value: OverrideValue::Text("#00FF88".to_owned()),
            },
            Self::Edit => Command::Edit {
                id: id(),
                property: "pos".to_owned(),
                value: OverrideValue::Vec2(Vec2::new(900.0, -40.5)),
            },
            Self::Key => Command::Key {
                id: id(),
                property: "opacity".to_owned(),
                time: Time::Seconds(1.5),
                value: OverrideValue::Number(0.5),
                ease: Some(Ease::QuadOut),
            },
            Self::Unkey => Command::Unkey {
                id: id(),
                property: channel(),
                time: Time::Frame(48),
            },
            Self::MoveKey => Command::MoveKey {
                id: id(),
                property: channel(),
                from: Time::Seconds(0.8),
                to: Time::Frame(60),
            },
            Self::Ease => Command::Ease {
                id: id(),
                property: channel(),
                time: Time::Seconds(0.8),
                ease: Ease::BackOut,
            },
            Self::SetEase => Command::SetEase(Ease::CubicInOut),
            Self::Stopwatch => Command::Stopwatch {
                id: id(),
                property: "rotation".to_owned(),
            },
            Self::ToggleKey => Command::ToggleKey {
                id: id(),
                property: channel(),
            },
            Self::AddKeys => Command::AddKeys,
            Self::DeleteKeys => Command::DeleteKeys,
            Self::RetimeKeys => Command::RetimeKeys(-12),
            Self::Visible => Command::Visible {
                id: id(),
                visible: false,
            },
            Self::Lock => Command::Lock {
                id: id(),
                locked: true,
            },
            Self::Rename => Command::Rename {
                id: id(),
                name: "Big \"title\"".to_owned(),
            },
            Self::AssignId => Command::AssignId {
                from: "@288".to_owned(),
                to: id(),
            },
            Self::DeleteOrphan => Command::DeleteOrphan {
                id: "gone".to_owned(),
                property: "opacity".to_owned(),
                time: Some(Time::Seconds(0.5)),
            },
            Self::EffectAdd => Command::EffectAdd {
                id: id(),
                kind: motioner_core::EffectKind::Blur,
            },
            Self::EffectRemove => Command::EffectRemove {
                id: id(),
                kind: motioner_core::EffectKind::Glow,
            },
            Self::EffectMove => Command::EffectMove {
                id: id(),
                kind: motioner_core::EffectKind::DropShadow,
                index: 1,
            },
            Self::MaskSet => Command::MaskSet {
                id: id(),
                source: "cutout".to_owned(),
                mode: motioner_core::MaskMode::Alpha,
                invert: false,
            },
            Self::MaskClear => Command::MaskClear { id: id() },
            Self::TextAnimAdd => Command::TextAnimAdd {
                id: id(),
                property: motioner_core::CharProperty::Opacity,
                from: OverrideValue::Number(0.0),
                to: OverrideValue::Number(1.0),
                start: Time::Seconds(0.5),
                stagger: 2.0,
                duration: 12.0,
                ease: Ease::QuadOut,
                order: motioner_core::CharOrder::Forward,
            },
            Self::TextAnimRemove => Command::TextAnimRemove { id: id(), index: 0 },
            Self::Undo => Command::Undo,
            Self::Redo => Command::Redo,
            Self::Bake => Command::Bake(vec![id(), "card".to_owned()]),
        }
    }
}
