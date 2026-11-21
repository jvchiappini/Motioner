//! The graph editor: opening one numeric channel and rebuilding its values
//! after every edit.

use std::fmt::Write as _;

use motioner_core::OverrideValue;

use super::{channels, Editor};
use crate::command::{CommandError, ErrorKind};
use crate::graph::{GraphKey, GraphModel};

impl Editor {
    /// The open graph editor, if any.
    #[must_use]
    pub fn graph(&self) -> Option<&GraphModel> {
        self.graph.as_ref()
    }

    /// Opens the graph on one numeric channel.
    pub(super) fn open_graph(&mut self, id: &str, property: &str) -> Result<(), CommandError> {
        let document = self.require_document()?;
        let (node, channel) = channels::resolve(document, id, property)?;
        let value = channels::sampled(document, node, channel, self.frame());
        if !matches!(
            value,
            Some(OverrideValue::Number(_) | OverrideValue::Vec2(_))
        ) {
            return Err(CommandError::new(
                ErrorKind::BadValue,
                format!("{property:?} is not a numeric channel"),
            ));
        }
        self.graph = Some(GraphModel {
            id: id.to_owned(),
            property: channel.name(),
            keys: Vec::new(),
            duration: document.composition().duration(),
            min: 0.0,
            max: 1.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            caption: String::new(),
        });
        self.rebuild_graph();
        if self
            .graph
            .as_ref()
            .is_some_and(|graph| graph.keys.is_empty())
        {
            self.graph = None;
            return Err(CommandError::new(
                ErrorKind::NoKey,
                format!("{property:?} has no keys to edit"),
            ));
        }
        if let Some(graph) = self.graph.as_mut() {
            // Opening fits the key time and value ranges with padding; a later
            // `graph_view` is the user's explicit view and is preserved.
            let (zoom_x, zoom_y, pan_x, pan_y) = graph.fitted_view();
            graph.zoom_x = zoom_x;
            graph.zoom_y = zoom_y;
            graph.pan_x = pan_x;
            graph.pan_y = pan_y;
        }
        Ok(())
    }

    /// Closes the graph editor.
    pub(super) fn close_graph(&mut self) {
        self.graph = None;
    }

    /// Changes the graph view; returns an error for a non-finite or
    /// non-positive zoom.
    pub(super) fn set_graph_view(
        &mut self,
        zoom: f64,
        pan_x: f64,
        pan_y: f64,
        zoom_y: Option<f64>,
    ) -> Result<(), CommandError> {
        let zoom_y = zoom_y.unwrap_or(zoom);
        if !(zoom.is_finite()
            && zoom > 0.0
            && zoom_y.is_finite()
            && zoom_y > 0.0
            && pan_x.is_finite()
            && pan_y.is_finite())
        {
            return Err(CommandError::new(
                ErrorKind::BadArgument,
                "the graph view needs positive zooms and finite pans",
            ));
        }
        let Some(graph) = self.graph.as_mut() else {
            return Err(CommandError::new(
                ErrorKind::Refused,
                "no graph editor is open",
            ));
        };
        graph.zoom_x = zoom.clamp(0.05, 64.0);
        graph.zoom_y = zoom_y.clamp(0.05, 64.0);
        graph.pan_x = pan_x;
        graph.pan_y = pan_y;
        Ok(())
    }

    /// Rebuilds the key list and value bounds of the open graph from the
    /// document, keeping the view.
    pub(super) fn rebuild_graph(&mut self) {
        let Some(mut graph) = self.graph.take() else {
            return;
        };
        let Some(document) = self.document() else {
            return;
        };
        let Ok((node, channel)) = channels::resolve(document, &graph.id, &graph.property) else {
            return;
        };
        graph.duration = document.composition().duration();
        graph.keys.clear();
        for frame in channels::keys(document, node, channel) {
            let Some((value, ease)) = channels::key_at(document, node, channel, frame) else {
                continue;
            };
            let value = match value {
                OverrideValue::Number(number) => number,
                OverrideValue::Vec2(vector) => vector.x,
                OverrideValue::Text(_) => continue,
            };
            graph.keys.push(GraphKey { frame, value, ease });
        }
        let display = document
            .find(&graph.id)
            .and_then(|node| document.node(node))
            .and_then(motioner_core::Node::name)
            .unwrap_or(&graph.id);
        graph.caption.clear();
        let _ = write!(graph.caption, "{display} \u{00b7} {}", graph.property);
        if let (Some(first), Some(last)) = (graph.keys.first(), graph.keys.last()) {
            let mut min = first.value;
            let mut max = first.value;
            for key in &graph.keys {
                min = min.min(key.value);
                max = max.max(key.value);
            }
            graph.min = min;
            graph.max = max;
            if (max - min).abs() < 1e-9 {
                graph.min -= 1.0;
                graph.max += 1.0;
            }
            graph.duration = graph.duration.max(last.frame + 1);
        }
        self.graph = Some(graph);
    }
}
