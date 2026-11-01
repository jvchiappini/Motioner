//! Keyboard mapping and transport actions.
//!
//! The mapping is pure so it can be unit-tested without a window: a key plus
//! modifier state becomes an [`Action`], and [`apply`] performs it on a
//! [`Transport`] or asks the window for a session-level effect.

use std::time::Duration;

use ferrous_engine::KeyCode;

use crate::Transport;

/// Modifier state relevant to the preview shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Shift is held (step ten frames instead of one).
    pub shift: bool,
}

/// A keyboard shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start or stop playback (Space).
    TogglePlay,
    /// Step by a signed number of frames (arrows).
    Step(i64),
    /// Jump to the first frame (Home).
    GoToStart,
    /// Jump to the last frame (End).
    GoToEnd,
    /// Toggle looping (L).
    ToggleLoop,
    /// Reload the document now (R).
    Reload,
    /// Select the previous (`-1`) or next (`+1`) layer (Up/Down).
    SelectRelative(i64),
    /// Show or hide the safe-area guides (G).
    ToggleSafeArea,
    /// Quit the window (Escape).
    Quit,
}

/// What [`apply`] did beyond the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// The action was fully handled by the transport.
    None,
    /// The caller should reload the session.
    Reload,
    /// The caller should move the layer selection by this many rows.
    SelectRelative(i64),
    /// The caller should toggle the safe-area guides.
    ToggleSafeArea,
    /// The caller should close the window.
    Quit,
}

/// Maps a key and modifiers to an action.
#[must_use]
pub fn action_for(key: KeyCode, modifiers: Modifiers) -> Option<Action> {
    let step = if modifiers.shift { 10 } else { 1 };
    match key {
        KeyCode::Space => Some(Action::TogglePlay),
        KeyCode::ArrowLeft => Some(Action::Step(-step)),
        KeyCode::ArrowRight => Some(Action::Step(step)),
        KeyCode::Home => Some(Action::GoToStart),
        KeyCode::End => Some(Action::GoToEnd),
        KeyCode::KeyL => Some(Action::ToggleLoop),
        KeyCode::KeyR => Some(Action::Reload),
        KeyCode::ArrowUp => Some(Action::SelectRelative(-1)),
        KeyCode::ArrowDown => Some(Action::SelectRelative(1)),
        KeyCode::KeyG => Some(Action::ToggleSafeArea),
        KeyCode::Escape => Some(Action::Quit),
        _ => None,
    }
}

/// The row selected after moving `delta` rows from `current` in a list of
/// `len` rows. With nothing selected, moving down selects the first row and
/// moving up the last; the result is clamped and `None` for an empty list.
#[must_use]
pub fn step_selection(current: Option<usize>, len: usize, delta: i64) -> Option<usize> {
    let last = len.checked_sub(1)?;
    let next = match current {
        Some(row) => {
            let row = i64::try_from(row).unwrap_or(i64::MAX);
            let last = i64::try_from(last).unwrap_or(i64::MAX);
            usize::try_from(row.saturating_add(delta).clamp(0, last)).unwrap_or(0)
        }
        None if delta > 0 => 0,
        None => last,
    };
    Some(next)
}

/// Applies `action` to `transport` and reports session-level effects.
pub fn apply(action: Action, transport: &mut Transport, now: Duration) -> Effect {
    match action {
        Action::TogglePlay => {
            transport.toggle(now);
            Effect::None
        }
        Action::Step(delta) => {
            transport.step(delta);
            Effect::None
        }
        Action::GoToStart => {
            transport.go_to_start();
            Effect::None
        }
        Action::GoToEnd => {
            transport.go_to_end();
            Effect::None
        }
        Action::ToggleLoop => {
            transport.toggle_looping();
            Effect::None
        }
        Action::Reload => Effect::Reload,
        Action::SelectRelative(delta) => Effect::SelectRelative(delta),
        Action::ToggleSafeArea => Effect::ToggleSafeArea,
        Action::Quit => Effect::Quit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::FrameRate;

    fn transport() -> Transport {
        Transport::new(FrameRate::new(60, 1).unwrap(), 100)
    }

    #[test]
    fn maps_every_shortcut() {
        let plain = Modifiers::default();
        let shift = Modifiers { shift: true };
        assert_eq!(action_for(KeyCode::Space, plain), Some(Action::TogglePlay));
        assert_eq!(
            action_for(KeyCode::ArrowLeft, plain),
            Some(Action::Step(-1))
        );
        assert_eq!(
            action_for(KeyCode::ArrowRight, plain),
            Some(Action::Step(1))
        );
        assert_eq!(
            action_for(KeyCode::ArrowLeft, shift),
            Some(Action::Step(-10))
        );
        assert_eq!(
            action_for(KeyCode::ArrowRight, shift),
            Some(Action::Step(10))
        );
        assert_eq!(action_for(KeyCode::Home, plain), Some(Action::GoToStart));
        assert_eq!(action_for(KeyCode::End, plain), Some(Action::GoToEnd));
        assert_eq!(action_for(KeyCode::KeyL, plain), Some(Action::ToggleLoop));
        assert_eq!(action_for(KeyCode::KeyR, plain), Some(Action::Reload));
        assert_eq!(action_for(KeyCode::Escape, plain), Some(Action::Quit));
        assert_eq!(
            action_for(KeyCode::ArrowUp, plain),
            Some(Action::SelectRelative(-1))
        );
        assert_eq!(
            action_for(KeyCode::ArrowDown, plain),
            Some(Action::SelectRelative(1))
        );
        assert_eq!(
            action_for(KeyCode::KeyG, plain),
            Some(Action::ToggleSafeArea)
        );
        assert_eq!(action_for(KeyCode::KeyA, plain), None);
    }

    #[test]
    fn applies_transport_actions() {
        let mut transport = transport();
        transport.set_frame(50);
        assert_eq!(
            apply(Action::Step(10), &mut transport, Duration::ZERO),
            Effect::None
        );
        assert_eq!(transport.frame(), 60);
        apply(Action::GoToStart, &mut transport, Duration::ZERO);
        assert_eq!(transport.frame(), 0);
        apply(Action::GoToEnd, &mut transport, Duration::ZERO);
        assert_eq!(transport.frame(), 99);
        assert!(transport.looping());
        apply(Action::ToggleLoop, &mut transport, Duration::ZERO);
        assert!(!transport.looping());
        apply(Action::TogglePlay, &mut transport, Duration::ZERO);
        assert!(transport.is_playing());
    }

    #[test]
    fn selection_steps_and_clamps() {
        assert_eq!(step_selection(None, 0, 1), None);
        assert_eq!(step_selection(None, 5, 1), Some(0));
        assert_eq!(step_selection(None, 5, -1), Some(4));
        assert_eq!(step_selection(Some(2), 5, 1), Some(3));
        assert_eq!(step_selection(Some(0), 5, -1), Some(0));
        assert_eq!(step_selection(Some(4), 5, 10), Some(4));
    }

    #[test]
    fn session_actions_do_not_touch_the_transport() {
        let mut transport = transport();
        transport.set_frame(7);
        assert_eq!(
            apply(Action::Reload, &mut transport, Duration::ZERO),
            Effect::Reload
        );
        assert_eq!(
            apply(Action::Quit, &mut transport, Duration::ZERO),
            Effect::Quit
        );
        assert_eq!(
            apply(Action::SelectRelative(1), &mut transport, Duration::ZERO),
            Effect::SelectRelative(1)
        );
        assert_eq!(
            apply(Action::ToggleSafeArea, &mut transport, Duration::ZERO),
            Effect::ToggleSafeArea
        );
        assert_eq!(transport.frame(), 7);
    }
}
