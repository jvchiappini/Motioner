//! Keyboard shortcuts as [`Command`]s.
//!
//! The mapping is pure so it can be unit-tested without a window: a key,
//! the modifier state and the current toggles become the command the
//! editor runs, the same command an agent would send.

use ferrous_engine::KeyCode;

use crate::command::Command;

/// Modifier state relevant to the editor shortcuts.
#[allow(clippy::struct_excessive_bools)] // three independent keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Shift is held.
    pub shift: bool,
    /// Ctrl is held.
    pub ctrl: bool,
    /// Alt is held (axis lock on the canvas, fine scrub steps).
    pub alt: bool,
}

/// The on/off state that toggle shortcuts flip.
#[allow(clippy::struct_excessive_bools)] // three independent toggles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Toggles {
    /// Looping playback (L).
    pub looping: bool,
    /// Safe-area guides (G).
    pub safe_area: bool,
    /// Only animated properties in the timeline (U).
    pub only_animated: bool,
}

/// The command a shortcut runs, if any.
#[must_use]
pub fn command_for(key: KeyCode, modifiers: Modifiers, toggles: Toggles) -> Option<Command> {
    let step = if modifiers.shift { 10 } else { 1 };
    if modifiers.ctrl {
        return match key {
            KeyCode::KeyZ if modifiers.shift => Some(Command::Redo),
            KeyCode::KeyZ => Some(Command::Undo),
            KeyCode::KeyY => Some(Command::Redo),
            _ => None,
        };
    }
    Some(match key {
        KeyCode::Space => Command::TogglePlay,
        KeyCode::ArrowLeft => Command::Step(-step),
        KeyCode::ArrowRight => Command::Step(step),
        KeyCode::Home => Command::GoToStart,
        KeyCode::End => Command::GoToEnd,
        KeyCode::KeyL => Command::Loop(!toggles.looping),
        KeyCode::KeyR => Command::Reload,
        KeyCode::ArrowUp => Command::SelectRelative(-1),
        KeyCode::ArrowDown => Command::SelectRelative(1),
        KeyCode::KeyG => Command::SafeArea(!toggles.safe_area),
        KeyCode::KeyK => Command::AddKeys,
        KeyCode::KeyU => Command::OnlyAnimated(!toggles.only_animated),
        KeyCode::KeyF => Command::ZoomFit,
        KeyCode::Digit1 => Command::Zoom(1.0),
        KeyCode::Delete => Command::DeleteKeys,
        KeyCode::Escape => Command::Quit,
        _ => return None,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_shortcut() {
        let plain = Modifiers::default();
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let off = Toggles::default();
        let on = Toggles {
            looping: true,
            safe_area: true,
            only_animated: true,
        };
        let map = |key, modifiers| command_for(key, modifiers, off);
        assert_eq!(map(KeyCode::Space, plain), Some(Command::TogglePlay));
        assert_eq!(map(KeyCode::ArrowLeft, plain), Some(Command::Step(-1)));
        assert_eq!(map(KeyCode::ArrowRight, plain), Some(Command::Step(1)));
        assert_eq!(map(KeyCode::ArrowLeft, shift), Some(Command::Step(-10)));
        assert_eq!(map(KeyCode::ArrowRight, shift), Some(Command::Step(10)));
        assert_eq!(map(KeyCode::Home, plain), Some(Command::GoToStart));
        assert_eq!(map(KeyCode::End, plain), Some(Command::GoToEnd));
        assert_eq!(map(KeyCode::KeyL, plain), Some(Command::Loop(true)));
        assert_eq!(
            command_for(KeyCode::KeyL, plain, on),
            Some(Command::Loop(false))
        );
        assert_eq!(map(KeyCode::KeyR, plain), Some(Command::Reload));
        assert_eq!(map(KeyCode::Escape, plain), Some(Command::Quit));
        assert_eq!(
            map(KeyCode::ArrowUp, plain),
            Some(Command::SelectRelative(-1))
        );
        assert_eq!(
            map(KeyCode::ArrowDown, plain),
            Some(Command::SelectRelative(1))
        );
        assert_eq!(map(KeyCode::KeyG, plain), Some(Command::SafeArea(true)));
        assert_eq!(
            command_for(KeyCode::KeyG, plain, on),
            Some(Command::SafeArea(false))
        );
        assert_eq!(map(KeyCode::KeyK, plain), Some(Command::AddKeys));
        assert_eq!(map(KeyCode::KeyU, plain), Some(Command::OnlyAnimated(true)));
        assert_eq!(map(KeyCode::KeyF, plain), Some(Command::ZoomFit));
        assert_eq!(map(KeyCode::Digit1, plain), Some(Command::Zoom(1.0)));
        assert!(map(KeyCode::Digit2, plain).is_none());
        assert_eq!(map(KeyCode::Delete, plain), Some(Command::DeleteKeys));
        assert_eq!(map(KeyCode::KeyZ, ctrl), Some(Command::Undo));
        assert_eq!(map(KeyCode::KeyY, ctrl), Some(Command::Redo));
        assert_eq!(
            map(
                KeyCode::KeyZ,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: false
                }
            ),
            Some(Command::Redo)
        );
        assert_eq!(map(KeyCode::KeyA, plain), None);
        // Ctrl wins over the plain mapping.
        assert_eq!(map(KeyCode::KeyR, ctrl), None);
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
}
