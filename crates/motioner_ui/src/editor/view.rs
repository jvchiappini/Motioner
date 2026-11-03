//! Transport, selection and view commands: everything that changes what the
//! editor shows without changing the overrides.

use std::time::Duration;

use motioner_core::NodeId;

use super::{channels, on_off, Editor, Effect, Outcome};
use crate::command::{Command, CommandError, ErrorKind, KeySpec, Panel};
use crate::edit::KeyRef;
use crate::input::step_selection;
use crate::layout::Splitter;

impl Editor {
    pub(super) fn execute_transport(
        &mut self,
        command: &Command,
        now: Duration,
    ) -> Result<Outcome, CommandError> {
        match command {
            Command::Play => {
                self.require_document()?;
                self.transport.play(now);
                Ok(Outcome::done("playing"))
            }
            Command::Pause => {
                self.transport.pause(now);
                Ok(Outcome::done(format!("paused at frame {}", self.frame())))
            }
            Command::TogglePlay => {
                self.require_document()?;
                self.transport.toggle(now);
                let state = if self.transport.is_playing() {
                    "playing"
                } else {
                    "paused"
                };
                Ok(Outcome::done(state))
            }
            Command::Step(delta) => {
                self.transport.step(*delta);
                Ok(self.at_frame())
            }
            Command::Seek(frame) => {
                self.transport.set_frame(*frame);
                Ok(self.at_frame())
            }
            Command::GoToStart => {
                self.transport.go_to_start();
                Ok(self.at_frame())
            }
            Command::GoToEnd => {
                self.transport.go_to_end();
                Ok(self.at_frame())
            }
            Command::Loop(on) => {
                self.transport.set_looping(*on);
                Ok(Outcome::done(format!("loop {}", on_off(*on))))
            }
            Command::PrevKey { id, property } => self.jump_key(id, property, false),
            Command::NextKey { id, property } => self.jump_key(id, property, true),
            other => Err(not_here(other)),
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per view command
    pub(super) fn execute_view(&mut self, command: &Command) -> Result<Outcome, CommandError> {
        match command {
            Command::Select(id) => {
                let node = channels::node_of(self.require_document()?, id)?;
                let row = self
                    .outline
                    .iter()
                    .position(|r| r.node == node)
                    .ok_or_else(|| {
                        CommandError::new(
                            ErrorKind::UnknownNode,
                            format!("{id:?} has no layer row"),
                        )
                    })?;
                self.select_row(row);
                Ok(Outcome::done(format!("selected {id}")))
            }
            Command::Deselect => {
                self.selected = None;
                self.selected_keys.clear();
                self.rebuild_inspector();
                Ok(Outcome::done("nothing selected"))
            }
            Command::SelectRelative(delta) => {
                if let Some(row) = step_selection(self.selected_row(), self.outline.len(), *delta) {
                    self.select_row(row);
                }
                Ok(Outcome::done(self.selection_summary()))
            }
            Command::Expand { id, open } => {
                let row = self.row_of(id)?;
                if *open {
                    self.expanded.insert(row);
                    self.pinned.insert(row);
                } else {
                    self.expanded.remove(&row);
                    self.pinned.remove(&row);
                }
                self.rebuild_timeline();
                Ok(Outcome::done(format!(
                    "{id} {}",
                    if *open { "expanded" } else { "collapsed" }
                )))
            }
            Command::ExpandAll(open) => {
                if *open {
                    self.expanded = (0..self.outline.len()).collect();
                    self.pinned = self.expanded.clone();
                } else {
                    self.expanded.clear();
                    self.pinned.clear();
                }
                self.rebuild_timeline();
                Ok(Outcome::done(if *open {
                    "all expanded"
                } else {
                    "all collapsed"
                }))
            }
            Command::SelectKeys { keys, add } => self.select_keys(keys, *add),
            Command::ClearKeys => {
                self.selected_keys.clear();
                self.rebuild_inspector();
                Ok(Outcome::done("no keys selected"))
            }
            Command::SafeArea(on) => {
                self.view.safe_area = *on;
                Ok(Outcome::done(format!("safe area {}", on_off(*on))))
            }
            Command::Snap(on) => {
                self.view.snap = *on;
                Ok(Outcome::done(format!("snap {}", on_off(*on))))
            }
            Command::OnlyAnimated(on) => {
                self.only_animated = *on;
                self.rebuild_timeline();
                Ok(Outcome::done(format!("only animated {}", on_off(*on))))
            }
            Command::MotionBlurPreview { on } => {
                self.motion_blur_preview = *on;
                Ok(Outcome::done(format!(
                    "motion blur preview {}",
                    on_off(*on)
                )))
            }
            Command::Zoom(factor) => {
                if !factor.is_finite() {
                    return Err(CommandError::new(
                        ErrorKind::BadArgument,
                        "the zoom factor must be a finite number",
                    ));
                }
                let zoom = crate::clamp_zoom(*factor);
                self.view.zoom = Some(zoom);
                Ok(Outcome::done(zoom_summary(zoom)))
            }
            Command::ZoomFit => {
                self.view.zoom = None;
                self.view.pan = (0.0, 0.0);
                Ok(Outcome::done("fit to canvas".to_owned()))
            }
            Command::ZoomBy { steps, x, y } => self.zoom_by(*steps, *x, *y),
            Command::Pan { dx, dy } => {
                if !(dx.is_finite() && dy.is_finite()) {
                    return Err(CommandError::new(
                        ErrorKind::BadArgument,
                        "the pan must be finite",
                    ));
                }
                self.view.pan.0 += dx;
                self.view.pan.1 += dy;
                Ok(Outcome::done(format!(
                    "pan ({}, {})",
                    motioner_core::format_number(self.view.pan.0),
                    motioner_core::format_number(self.view.pan.1)
                )))
            }
            Command::PanelSize { panel, size } => {
                if !size.is_finite() || *size < 0.0 {
                    return Err(CommandError::new(
                        ErrorKind::BadArgument,
                        "size must be positive",
                    ));
                }
                // Same rules (minimums, 4 px grid) as dragging the splitter.
                let current = self.view.sizes;
                #[allow(clippy::cast_possible_truncation)] // logical pixels
                let target = *size as f32;
                let (splitter, delta) = match panel {
                    Panel::Layers => (Splitter::Left, (target - current.left, 0.0)),
                    Panel::Inspector => (Splitter::Right, (current.right - target, 0.0)),
                    Panel::Timeline => (Splitter::Timeline, (0.0, current.timeline - target)),
                };
                self.view.sizes = crate::layout::resize_panels(current, splitter, delta);
                Ok(Outcome::done(format!("{} resized", panel.name())))
            }
            Command::Scroll { panel, offset } => {
                self.view.set_scroll(*panel, *offset);
                Ok(Outcome::done(format!("{} scrolled", panel.name())))
            }
            Command::CopyId => {
                let node = self.require_selection()?;
                let id = self.node_identity(node);
                Ok(Outcome {
                    summary: format!("copied {id}"),
                    effect: Effect::Copy(id),
                })
            }
            other => Err(not_here(other)),
        }
    }

    fn at_frame(&self) -> Outcome {
        Outcome::done(format!("frame {}", self.frame()))
    }

    /// The wheel: multiply the zoom, keeping the composition point under the
    /// pointer fixed when one is given.
    fn zoom_by(
        &mut self,
        steps: i64,
        x: Option<f64>,
        y: Option<f64>,
    ) -> Result<Outcome, CommandError> {
        let (width, height) = {
            let composition = self.require_document()?.composition();
            (
                f64::from(composition.width()),
                f64::from(composition.height()),
            )
        };
        if x.is_some() != y.is_some() {
            return Err(CommandError::new(
                ErrorKind::BadArgument,
                "give both x and y for the zoom anchor, or neither",
            ));
        }
        let base = match self.view.zoom {
            Some(zoom) if zoom.is_finite() && zoom > 0.0 => zoom,
            _ => self.view.fit_zoom,
        };
        let zoom = crate::zoom_by_steps(base, steps);
        if let (Some(x), Some(y)) = (x, y) {
            if !(x.is_finite() && y.is_finite()) {
                return Err(CommandError::new(
                    ErrorKind::BadArgument,
                    "the zoom anchor must be finite",
                ));
            }
            self.view.pan = crate::anchored_pan(
                self.view.pan,
                base,
                zoom,
                motioner_core::Vec2::new(x, y),
                (width, height),
            );
        }
        self.view.zoom = Some(zoom);
        Ok(Outcome::done(zoom_summary(zoom)))
    }

    fn selection_summary(&self) -> String {
        self.selected.map_or_else(
            || "nothing selected".to_owned(),
            |node| format!("selected {}", self.node_identity(node)),
        )
    }

    pub(super) fn require_selection(&self) -> Result<NodeId, CommandError> {
        self.selected
            .ok_or_else(|| CommandError::new(ErrorKind::NoSelection, "no layer is selected"))
    }

    fn row_of(&self, id: &str) -> Result<usize, CommandError> {
        let node = channels::node_of(self.require_document()?, id)?;
        self.outline
            .iter()
            .position(|row| row.node == node)
            .ok_or_else(|| CommandError::new(ErrorKind::UnknownNode, format!("{id:?} has no row")))
    }

    /// Selects an outline row the way a click does: the previous row
    /// collapses unless the user pinned it open, the new one expands.
    pub(super) fn select_row(&mut self, row: usize) {
        let previous = self.selected_row();
        self.selected = self.outline.get(row).map(|r| r.node);
        self.selected_keys.clear();
        if let Some(previous) = previous {
            if Some(previous) != self.selected_row() && !self.pinned.contains(&previous) {
                self.expanded.remove(&previous);
            }
        }
        if let Some(current) = self.selected_row() {
            if !self.pinned.contains(&current) {
                self.expanded.insert(current);
            }
        }
        self.rebuild_timeline();
        self.rebuild_inspector();
    }

    fn jump_key(
        &mut self,
        id: &str,
        property: &str,
        forward: bool,
    ) -> Result<Outcome, CommandError> {
        let document = self.require_document()?;
        let (node, channel) = channels::resolve(document, id, property)?;
        let frame = self.frame();
        let keys = channels::keys(document, node, channel);
        let target = if forward {
            keys.iter().copied().find(|&key| key > frame)
        } else {
            keys.iter().copied().rev().find(|&key| key < frame)
        };
        if let Some(target) = target {
            self.transport.set_frame(target);
        }
        Ok(self.at_frame())
    }

    fn select_keys(&mut self, keys: &[KeySpec], add: bool) -> Result<Outcome, CommandError> {
        let document = self.require_document()?;
        let rate = document.composition().frame_rate();
        let mut refs = Vec::with_capacity(keys.len());
        for key in keys {
            let frame = key.time.frame(rate)?;
            let (node, channel) = channels::resolve(document, &key.id, &key.property)?;
            if !channels::keys(document, node, channel).contains(&frame) {
                return Err(CommandError::new(
                    ErrorKind::NoKey,
                    format!("{} has no key on {} at frame {frame}", key.id, key.property),
                ));
            }
            refs.push(KeyRef {
                id: key.id.clone(),
                property: channel.name(),
                frame,
            });
        }
        if !add {
            self.selected_keys.clear();
        }
        for key in refs {
            if !self.selected_keys.contains(&key) {
                self.selected_keys.push(key);
            }
        }
        self.rebuild_inspector();
        Ok(Outcome::done(format!(
            "{} key(s) selected",
            self.selected_keys.len()
        )))
    }
}

fn not_here(command: &Command) -> CommandError {
    CommandError::new(
        ErrorKind::Refused,
        format!("{} is not handled here", command.kind().name()),
    )
}

/// One-line zoom description: `zoom 200%`.
fn zoom_summary(zoom: f64) -> String {
    format!("zoom {:.0}%", zoom * 100.0)
}
