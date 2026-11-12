//! Machine-readable views of the editor: `query` (the scene at a frame) and
//! `state` (the editor around it). Both are JSON with a fixed field order,
//! nodes in outline order and channels in display order, so the same state
//! always prints the same bytes (D-0024).

use motioner_core::{Document, Frame, NodeId, OverrideValue};

use super::channels::{self, channel_name, CHANNELS};
use super::Editor;
use crate::command::{CommandError, Panel};
use crate::json::Json;
use crate::timeline_model::{content_fields, is_visible_at};

impl Editor {
    /// The scene at the playhead as JSON: composition, nodes (identity,
    /// kind, name, parent, visible/locked, every channel's value and keys,
    /// content), overrides, orphans and diagnostics. `node` limits the nodes
    /// to one identity.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NoDocument`](crate::ErrorKind::NoDocument) without a document, [`ErrorKind::UnknownNode`](crate::ErrorKind::UnknownNode)
    /// for an unknown `node`.
    pub fn query_json(&self, node: Option<&str>) -> Result<String, CommandError> {
        let document = self.require_document()?;
        if let Some(id) = node {
            channels::node_of(document, id)?;
        }
        let frame = self.frame();
        let rate = document.composition().frame_rate();
        let mut j = Json::new();
        j.begin_object();
        j.key("file")
            .string(&self.session.path().display().to_string());
        j.key("frame").int(frame);
        j.key("time").number(motioner_core::seconds_of(frame, rate));
        composition(&mut j, document);
        j.key("std_version")
            .int(i64::from(motioner_dsl::std_version()));
        j.key("nodes").begin_array();
        for row in &self.outline {
            let identity = self.node_identity(row.node);
            if node.is_some_and(|id| id != identity) {
                continue;
            }
            self.node_json(
                &mut j,
                document,
                row.node,
                &row.name,
                row.kind.name(),
                frame,
            );
        }
        j.end_array();
        j.key("overrides").begin_array();
        let text = self.session.overrides().to_mdf(rate);
        for line in text
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
        {
            j.string(line);
        }
        j.end_array();
        j.key("orphans").begin_array();
        for (entry, suggestions) in self.orphan_suggestions() {
            j.begin_object();
            j.key("id").string(&entry.id);
            j.key("property").string(&entry.property);
            match entry.frame() {
                Some(frame) => j.key("frame").int(frame),
                None => j.key("frame").null(),
            };
            j.key("suggestions").begin_array();
            for suggestion in &suggestions {
                j.string(suggestion);
            }
            j.end_array();
            j.end_object();
        }
        j.end_array();
        self.diagnostics_json(&mut j);
        j.end_object();
        Ok(j.finish())
    }

    #[allow(clippy::too_many_lines)] // one node's fixed JSON field order
    fn node_json(
        &self,
        j: &mut Json,
        document: &Document,
        node: NodeId,
        name: &str,
        kind: &str,
        frame: Frame,
    ) {
        let Some(n) = document.node(node) else {
            return;
        };
        let rate = document.composition().frame_rate();
        j.begin_object();
        j.key("id").string(n.identity().as_str());
        j.key("explicit_id").boolean(n.identity().is_explicit());
        j.key("kind").string(&kind.to_lowercase());
        j.key("name").string(n.name().unwrap_or(name));
        match n.parent().and_then(|parent| document.node(parent)) {
            Some(parent) => j.key("parent").string(parent.identity().as_str()),
            None => j.key("parent").null(),
        };
        j.key("visible").boolean(n.is_visible());
        j.key("drawn").boolean(is_visible_at(document, node, frame));
        j.key("locked").boolean(self.node_locked(node));
        j.key("blend").string(n.blend().name());
        j.key("channels").begin_object();
        for (property, axis) in CHANNELS {
            let channel = channels::Channel::Transform(property, axis);
            channel_json(
                j,
                document,
                node,
                channel,
                &channel_name(property, axis),
                frame,
                rate,
            );
        }
        for channel in channels::node_channels(document, node) {
            if matches!(channel, channels::Channel::Transform(_, _)) {
                continue;
            }
            let name = channel.name();
            channel_json(j, document, node, channel, &name, frame, rate);
        }
        j.end_object();
        j.key("effects").begin_array();
        for effect in n.effects() {
            j.begin_object();
            j.key("kind").string(effect.kind().name());
            j.key("params").begin_object();
            for &param in effect.kind().params() {
                let axes: &[Option<motioner_core::Axis>] = if param.is_vector() {
                    &[Some(motioner_core::Axis::X), Some(motioner_core::Axis::Y)]
                } else {
                    &[None]
                };
                for &axis in axes {
                    let Ok(effect_channel) =
                        motioner_core::EffectChannel::new(effect.kind(), param, axis)
                    else {
                        continue;
                    };
                    let channel = channels::Channel::Effect(effect_channel);
                    let name = channel.name();
                    channel_json(j, document, node, channel, &name, frame, rate);
                }
            }
            j.end_object();
            j.end_object();
        }
        j.end_array();
        match n.mask() {
            Some(mask) => {
                j.key("mask").begin_object();
                match document.node(mask.source) {
                    Some(source) => j.key("source").string(source.identity().as_str()),
                    None => j.key("source").null(),
                };
                j.key("mode").string(mask.mode.name());
                j.key("invert").boolean(mask.invert);
                j.end_object();
            }
            None => {
                j.key("mask").null();
            }
        }
        j.key("text_animators").begin_array();
        for animator in document.text_animators(node).unwrap_or(&[]) {
            j.begin_object();
            j.key("property").string(animator.property.name());
            j.key("from");
            char_value_json(j, animator.from);
            j.key("to");
            char_value_json(j, animator.to);
            j.key("start").int(animator.start);
            j.key("stagger").number(animator.stagger);
            j.key("duration").number(animator.duration);
            j.key("ease").string(animator.ease.name());
            j.key("order").string(animator.order.name());
            j.end_object();
        }
        j.end_array();
        if let motioner_core::NodeContent::Image(image) = n.content() {
            j.key("image").begin_object();
            j.key("source").string(&image.source);
            j.key("fit").string(image.fit.name());
            if let Some(asset) = image.asset {
                j.key("natural_width").int(i64::from(asset.width));
                j.key("natural_height").int(i64::from(asset.height));
                j.key("key").string(&asset.key.to_hex());
            } else {
                j.key("natural_width").null();
                j.key("natural_height").null();
                j.key("key").null();
            }
            j.end_object();
        }
        j.key("content").begin_object();
        for field in content_fields(n.content()) {
            let name = field.name();
            j.key(&name);
            content_value(j, document, node, field);
        }
        j.end_object();
        j.end_object();
    }

    fn diagnostics_json(&self, j: &mut Json) {
        j.key("diagnostics").begin_array();
        for (severity, list) in [
            ("error", self.session.diagnostics()),
            ("warning", self.session.warnings()),
        ] {
            for diagnostic in list {
                j.begin_object();
                j.key("severity").string(severity);
                j.key("line").int(i64::from(diagnostic.line));
                j.key("column").int(i64::from(diagnostic.column));
                j.key("message").string(&diagnostic.message);
                j.end_object();
            }
        }
        j.end_array();
    }

    /// The editor state as JSON: playhead, playback, selection, expanded
    /// rows, selected keys, view toggles, panel sizes, history and whether an
    /// edit is waiting to be saved.
    #[must_use]
    pub fn state_json(&self) -> String {
        let mut j = Json::new();
        j.begin_object();
        j.key("file")
            .string(&self.session.path().display().to_string());
        j.key("loaded").boolean(self.document().is_some());
        j.key("frame").int(self.frame());
        j.key("duration").int(self.transport.duration());
        j.key("playing").boolean(self.transport.is_playing());
        j.key("looping").boolean(self.transport.looping());
        let selected = self.selected.map(|node| self.node_identity(node));
        j.key("selected").opt_string(selected.as_deref());
        j.key("expanded").begin_array();
        for row in &self.expanded {
            if let Some(outline) = self.outline.get(*row) {
                j.string(&self.node_identity(outline.node));
            }
        }
        j.end_array();
        j.key("selected_keys").begin_array();
        for key in &self.selected_keys {
            j.begin_object();
            j.key("id").string(&key.id);
            j.key("property").string(&key.property);
            j.key("frame").int(key.frame);
            j.end_object();
        }
        j.end_array();
        j.key("only_animated").boolean(self.only_animated);
        j.key("motion_blur_preview")
            .boolean(self.motion_blur_preview);
        j.key("filter").string(&self.filter);
        j.key("show_shy").boolean(self.show_shy);
        match self.graph() {
            Some(graph) => {
                j.key("graph").begin_object();
                j.key("id").string(&graph.id);
                j.key("property").string(&graph.property);
                j.key("zoom").number(graph.zoom_x);
                j.key("zoom_y").number(graph.zoom_y);
                j.key("pan").begin_array();
                j.number(graph.pan_x).number(graph.pan_y);
                j.end_array();
                j.end_object();
            }
            None => {
                j.key("graph").null();
            }
        }
        j.key("safe_area").boolean(self.view.safe_area);
        j.key("snap").boolean(self.view.snap);
        match self.view.zoom {
            Some(zoom) => j.key("zoom").number(zoom),
            None => j.key("zoom").null(),
        };
        j.key("pan").begin_array();
        j.number(self.view.pan.0).number(self.view.pan.1);
        j.end_array();
        j.key("panels").begin_object();
        let sizes = self.view.sizes;
        for (panel, size) in [
            (Panel::Layers, sizes.left),
            (Panel::Inspector, sizes.right),
            (Panel::Timeline, sizes.timeline),
        ] {
            j.key(panel.name()).begin_object();
            j.key("size").number(f64::from(size));
            j.key("scroll").number(self.view.scroll(panel));
            j.end_object();
        }
        j.end_object();
        let (undo, redo) = self.history();
        j.key("undo_depth")
            .int(i64::try_from(undo).unwrap_or(i64::MAX));
        j.key("redo_depth")
            .int(i64::try_from(redo).unwrap_or(i64::MAX));
        j.key("dirty").boolean(self.session.is_dirty());
        j.key("errors")
            .int(i64::try_from(self.session.diagnostics().len()).unwrap_or(0));
        j.key("warnings")
            .int(i64::try_from(self.session.warnings().len()).unwrap_or(0));
        j.end_object();
        j.finish()
    }

    /// Fails with [`ErrorKind::NoDocument`](crate::ErrorKind::NoDocument) when nothing is loaded; used by
    /// requests that need a document.
    ///
    /// # Errors
    ///
    /// When the program has an error and no earlier version loaded.
    pub fn ensure_document(&self) -> Result<(), CommandError> {
        self.require_document().map(|_| ())
    }
}

fn composition(j: &mut Json, document: &Document) {
    let c = document.composition();
    let rate = c.frame_rate();
    j.key("composition").begin_object();
    j.key("name").string(c.name());
    j.key("width").int(i64::from(c.width()));
    j.key("height").int(i64::from(c.height()));
    j.key("fps")
        .string(&format!("{}/{}", rate.numerator(), rate.denominator()));
    j.key("duration").int(c.duration());
    j.key("background")
        .string(&crate::inspect::hex(c.background()));
    j.end_object();
}

/// One channel's value and keys, in the fixed field order of `query`.
fn channel_json(
    j: &mut Json,
    document: &Document,
    node: NodeId,
    channel: channels::Channel,
    name: &str,
    frame: Frame,
    rate: motioner_core::FrameRate,
) {
    j.key(name).begin_object();
    value_or_null(j, channels::sampled(document, node, channel, frame));
    j.key("keys").begin_array();
    let frames = channels::keys(document, node, channel);
    let eases = channels::eases(document, node, channel);
    for (index, key) in frames.iter().enumerate() {
        j.begin_object();
        j.key("frame").int(*key);
        j.key("time").number(motioner_core::seconds_of(*key, rate));
        if let Some((value, _)) = channels::key_at(document, node, channel, *key) {
            value_or_null(j, Some(value));
        }
        j.key("ease").string(
            &eases
                .get(index)
                .map_or_else(|| "linear".to_owned(), |ease| ease.mdf_value()),
        );
        j.end_object();
    }
    j.end_array();
    j.end_object();
}

/// One per-character animator endpoint (D-0034).
fn char_value_json(j: &mut Json, value: motioner_core::CharValue) {
    match value {
        motioner_core::CharValue::Number(number) => {
            j.number(number);
        }
        motioner_core::CharValue::Vec2(pair) => {
            j.begin_array();
            j.number(pair.x).number(pair.y);
            j.end_array();
        }
        motioner_core::CharValue::Color(color) => {
            j.string(&color.to_hex());
        }
    }
}

fn value_or_null(j: &mut Json, value: Option<OverrideValue>) {
    match value {
        Some(OverrideValue::Number(value)) => {
            j.key("value").number(value);
        }
        Some(OverrideValue::Vec2(value)) => {
            j.key("value").begin_array();
            j.number(value.x).number(value.y);
            j.end_array();
        }
        Some(OverrideValue::Text(text)) => {
            j.key("value").string(&text);
        }
        None => {
            j.key("value").null();
        }
    }
}

/// The authored static value of one content field.
///
/// The sampled value and its keys live in the `channels` object; `content`
/// reports what the program authored, so a reader sees the document's static
/// values even when the field is animated.
fn content_value(
    j: &mut Json,
    document: &Document,
    node: NodeId,
    field: motioner_core::ContentField,
) {
    match document
        .node(node)
        .and_then(|node| node.content().static_value(field))
    {
        Some(motioner_core::ContentValue::Text(text)) => {
            j.string(&text);
        }
        Some(motioner_core::ContentValue::Align(align)) => {
            j.string(match align {
                motioner_core::TextAlign::Left => "left",
                motioner_core::TextAlign::Center => "center",
                motioner_core::TextAlign::Right => "right",
            });
        }
        Some(motioner_core::ContentValue::Number(number)) => {
            j.number(number);
        }
        Some(motioner_core::ContentValue::Cap(cap)) => {
            j.string(cap.name());
        }
        Some(motioner_core::ContentValue::Join(join)) => {
            j.string(join.name());
        }
        Some(motioner_core::ContentValue::Fit(fit)) => {
            j.string(fit.name());
        }
        Some(motioner_core::ContentValue::Color(color)) => {
            j.opt_string(color.map(motioner_core::Color::to_hex).as_deref());
        }
        Some(motioner_core::ContentValue::Gradient(Some(gradient))) => {
            gradient_json(j, &gradient);
        }
        Some(
            motioner_core::ContentValue::GeometryPoints(_)
            | motioner_core::ContentValue::Gradient(None),
        )
        | None => {
            j.null();
        }
    }
}

/// Writes a gradient as `{"kind": "linear", "start": [x, y], ...}` (D-0030).
fn gradient_json(j: &mut Json, gradient: &motioner_core::Gradient) {
    use motioner_core::GradientKind;
    j.begin_object();
    j.key("kind").string(gradient.kind().name());
    match gradient.kind() {
        GradientKind::Linear { start, end } => {
            j.key("start").begin_array();
            j.number(start.x);
            j.number(start.y);
            j.end_array();
            j.key("end").begin_array();
            j.number(end.x);
            j.number(end.y);
            j.end_array();
        }
        GradientKind::Radial { center, radius } => {
            j.key("center").begin_array();
            j.number(center.x);
            j.number(center.y);
            j.end_array();
            j.key("radius").number(radius);
        }
    }
    j.key("stops").begin_array();
    for stop in gradient.stops() {
        j.begin_array();
        j.number(stop.offset.value());
        j.string(&stop.color.value().to_hex());
        j.end_array();
    }
    j.end_array();
    j.end_object();
}

#[cfg(test)]
mod tests {
    use crate::command::ErrorKind;
    use crate::editor::tests::hello_editor;

    #[test]
    fn query_is_deterministic_and_lists_every_node() {
        let (editor, _dir) = hello_editor("query");
        let first = editor.query_json(None).unwrap();
        let second = editor.query_json(None).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.matches("\"kind\":").count(), 10);
        assert!(first.contains("\"composition\":{\"name\":\"Hello\",\"width\":1920"));
        assert!(first.contains("\"pos.x\":{\"value\":960,\"keys\":[{\"frame\":0"));
        assert!(first.contains("\"ease\":\"cubic_out\""));
    }

    #[test]
    fn query_filters_one_node_and_rejects_unknown_ids() {
        let (editor, _dir) = hello_editor("query_one");
        let one = editor.query_json(Some("@281")).unwrap();
        assert_eq!(one.matches("\"kind\":").count(), 1);
        assert_eq!(
            editor.query_json(Some("nope")).unwrap_err().kind,
            ErrorKind::UnknownNode
        );
    }

    #[test]
    fn state_reports_the_editor() {
        let (editor, _dir) = hello_editor("state");
        let state = editor.state_json();
        assert!(state.contains("\"frame\":0"));
        assert!(state.contains("\"selected\":null"));
        assert!(state.contains("\"undo_depth\":0"));
        assert!(state.contains("\"timeline\":{\"size\":320"));
    }

    #[test]
    fn query_lists_orphans_with_their_suggestions() {
        let (mut editor, dir) = hello_editor("query_orphan");
        std::fs::write(
            dir.0.join("hello.overrides.motion"),
            "override(\"@291\", \"rotation\", 5)\n",
        )
        .unwrap();
        assert!(editor.reload(std::time::Duration::ZERO));
        let json = editor.query_json(None).unwrap();
        assert!(
            json.contains("\"orphans\":[{\"id\":\"@291\",\"property\":\"rotation\""),
            "{json}"
        );
        assert!(json.contains("\"suggestions\":[\"@"), "{json}");
    }
}
