//! The 2D scene API: `comp`, node constructors, node properties and the
//! `key` / `add` methods (spec section 7, v0 subset).
//!
//! This is the only place where MDF values become document mutations, and
//! every mutation goes through the `motioner_core::Document` API.

use std::fmt::Write as _;
use std::rc::Rc;

use motioner_core::{
    BlendMode, CharOrder, CharProperty, CharValue, Color, Composition, ContentField, ContentValue,
    CoreError, Document, Ease, Effect, EffectChannel, EffectKind, EffectParam, EffectValueInput,
    FrameRate, Gradient, ImageContent, ImageFit, LineCap, LineJoin, MaskMode, NodeContent, NodeId,
    NodeIdentity, Paint, PathGeometry, PathPoint, Property, PropertyValue, Stroke, TextAlign,
    TextAnimator, TextContent, Vec2, MAX_CANVAS_SIZE,
};

use crate::diagnostic::{ErrorCategory, Span};
use crate::host::{
    as_integer, as_vec2, bind, vec2_value, Bound, CallArgs, HostError, HostResult, PaintInput,
};
use crate::origins::{EaseOrigin, Literal, OriginKind, ValueOrigin, ValueOrigins};
use crate::value::{Builtin, NodeMethod, Value};

/// NTSC-style frame rates written as decimals, mapped to exact rationals.
const NTSC_RATES: [(f64, u32); 3] = [(23.976, 24_000), (29.97, 30_000), (59.94, 60_000)];

/// Highest integer frame rate accepted by `comp()`.
const MAX_FPS: i64 = 1000;

/// Where a call that creates nodes came from (D-0022 provenance).
///
/// The key is `@<call start>` followed by `/c<call-site start>` for every user
/// function frame (outermost first) and `/i<index>` for every enclosing loop.
/// The same program always produces the same sites.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CreationSite {
    /// Byte offset of the creating call's span start.
    pub call_start: usize,
    /// Span starts of the user-function call sites on the stack.
    pub callers: Vec<usize>,
    /// Iteration index of every enclosing `for` loop, outermost first.
    pub loops: Vec<i64>,
}

impl CreationSite {
    /// The stable provenance key this site renders as.
    pub fn key(&self) -> String {
        let mut key = format!("@{}", self.call_start);
        for caller in &self.callers {
            let _ = write!(key, "/c{caller}");
        }
        for index in &self.loops {
            let _ = write!(key, "/i{index}");
        }
        key
    }
}

/// The frame, ease and ease source of one `key(...)` call.
#[derive(Debug, Clone)]
struct KeyTiming {
    frame: motioner_core::Frame,
    ease: Ease,
    ease_origin: Option<EaseOrigin>,
}

/// Builds the document while a program runs.
pub(crate) struct Scene {
    document: Option<Document>,
    max_nodes: usize,
    origins: ValueOrigins,
}

impl Scene {
    pub fn new(max_nodes: usize) -> Self {
        Self {
            document: None,
            max_nodes,
            origins: ValueOrigins::new(),
        }
    }

    /// The value origins recorded so far.
    pub fn origins(&self) -> &ValueOrigins {
        &self.origins
    }

    /// The finished document, if `comp()` was called.
    pub fn into_document(self) -> Option<Document> {
        self.document
    }

    /// Records where a node property value came from, when the source is
    /// known (an argument that was actually written in the program).
    fn record(&mut self, node: NodeId, property: &str, source: Option<(Span, Option<Literal>)>) {
        if let Some((span, literal)) = source {
            self.origins.record(ValueOrigin {
                node,
                property: property.to_owned(),
                span,
                literal,
                kind: OriginKind::Set,
                ease: None,
            });
        }
    }

    /// Calls a scene constructor builtin.
    pub fn call(
        &mut self,
        builtin: Builtin,
        args: CallArgs,
        site: &CreationSite,
    ) -> HostResult<Value> {
        match builtin {
            Builtin::Comp => self.comp(args),
            Builtin::Group => {
                bind("group", &[], 0, args)?;
                self.create(NodeContent::Group, site)
            }
            Builtin::Rect => {
                let params = [
                    "width",
                    "height",
                    "radius",
                    "fill",
                    "stroke",
                    "stroke_width",
                ];
                let bound = bind("rect", &params, 2, args)?;
                let paint = paint(&bound, 3)?;
                let content = NodeContent::rect(
                    bound.number(0)?,
                    bound.number(1)?,
                    bound.number_or(2, 0.0)?,
                    paint.paint,
                );
                let node = self.create(content, site)?;
                if let Value::Node(id) = node {
                    self.apply_gradients(id, paint)?;
                    for (index, property) in [
                        (0, "width"),
                        (1, "height"),
                        (2, "radius"),
                        (3, "fill"),
                        (4, "stroke"),
                        (5, "stroke_width"),
                    ] {
                        self.record(id, property, bound.source(index));
                    }
                }
                Ok(node)
            }
            Builtin::Circle => {
                let params = ["radius", "fill", "stroke", "stroke_width"];
                let bound = bind("circle", &params, 1, args)?;
                let paint = paint(&bound, 1)?;
                let content = NodeContent::circle(bound.number(0)?, paint.paint);
                let node = self.create(content, site)?;
                if let Value::Node(id) = node {
                    self.apply_gradients(id, paint)?;
                    for (index, property) in [
                        (0, "radius"),
                        (1, "fill"),
                        (2, "stroke"),
                        (3, "stroke_width"),
                    ] {
                        self.record(id, property, bound.source(index));
                    }
                }
                Ok(node)
            }
            Builtin::Text => self.text(args, site),
            Builtin::Image => self.image(args, site),
            Builtin::Path => self.path(args, site),
            Builtin::Polygon => self.polygon(args, site),
            Builtin::Star => self.star(args, site),
            Builtin::Arc => self.arc(args, site),
            _ => Err(HostError::type_error(format!(
                "{}() is not a scene constructor",
                builtin.name()
            ))),
        }
    }

    fn comp(&mut self, args: CallArgs) -> HostResult<Value> {
        if self.document.is_some() {
            return Err(HostError::value_error("comp() can only be called once"));
        }
        let params = ["name", "width", "height", "fps", "duration", "background"];
        let bound = bind("comp", &params, 1, args)?;
        let max_side = i64::from(MAX_CANVAS_SIZE);
        let side = |index, default| -> HostResult<u32> {
            let value = bound.integer_in(index, default, 1, max_side)?;
            u32::try_from(value).map_err(|_| HostError::value_error("canvas size out of range"))
        };
        let frame_rate = frame_rate(bound.number_or(3, 60.0)?)?;
        let duration = bound.number_or(4, 5.0)?;
        if !(duration.is_finite() && duration > 0.0) {
            return Err(HostError::value_error(
                "comp(): 'duration' must be a positive number of seconds",
            ));
        }
        let composition = Composition::new(
            bound.string_or(0, "")?.as_ref(),
            side(1, 1920)?,
            side(2, 1080)?,
            frame_rate,
            frame_rate
                .seconds_to_frame(duration)
                .map_err(|error| core_error(&error))?,
            bound.color_or(5, Color::BLACK)?,
        )
        .map_err(|error| core_error(&error))?;
        self.document = Some(Document::new(composition));
        Ok(Value::None)
    }

    fn text(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = [
            "content",
            "font",
            "size",
            "color",
            "align",
            "weight",
            "tracking",
            "line_height",
        ];
        let bound = bind("text", &params, 1, args)?;
        let Some(Value::Str(content)) = bound.get(0) else {
            let got = bound.get(0).map_or("nothing", Value::type_name);
            return Err(HostError::type_error(format!(
                "text(): 'content' must be a string, got {got}; use str() to convert"
            )));
        };
        let align_name = bound.string_or(4, "left")?;
        let align = TextAlign::from_name(&align_name).ok_or_else(|| {
            HostError::value_error(format!(
                "text(): unknown align {align_name:?}; use \"left\", \"center\" or \"right\""
            ))
        })?;
        let color = bound.optional_paint(3, Some(PaintInput::Color(Color::WHITE)))?;
        let (color, gradient) = split_paint(color);
        let content = NodeContent::Text(TextContent::new(
            content.to_string(),
            // Roboto is the historical default: documents that never asked for
            // a family keep the exact advances they were authored with.
            bound.string_or(1, "Roboto")?.to_string(),
            bound.number_or(2, 48.0)?,
            color.unwrap_or(Color::WHITE),
            align,
        ));
        let node = self.create(content, site)?;
        if let Value::Node(id) = node {
            if let Some(gradient) = gradient {
                self.document_mut()?
                    .set_content(
                        id,
                        ContentField::Color,
                        ContentValue::Gradient(Some(gradient)),
                    )
                    .map_err(|error| core_error(&error))?;
            }
            let weight = bound.number_or(5, 400.0)?;
            let tracking = bound.number_or(6, 0.0)?;
            let line_height = bound.number_or(7, 1.0)?;
            let document = self.document_mut()?;
            for (field, value) in [
                (ContentField::Weight, weight),
                (ContentField::Tracking, tracking),
                (ContentField::LineHeight, line_height),
            ] {
                document
                    .set_content(id, field, ContentValue::Number(value))
                    .map_err(|error| core_error(&error))?;
            }
            for (index, property) in [
                (0, "text"),
                (2, "size"),
                (3, "color"),
                (4, "align"),
                (5, "weight"),
                (6, "tracking"),
                (7, "line_height"),
            ] {
                self.record(id, property, bound.source(index));
            }
        }
        Ok(node)
    }

    /// `image(source, width=None, height=None, fit="contain")` (D-0035).
    ///
    /// `source` is a project-relative path; the loader resolves its key and
    /// natural size after the program runs.  `width`/`height` are the
    /// destination box in pixels; `None` or `0` keeps the natural size/aspect.
    fn image(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = ["source", "width", "height", "fit"];
        let bound = bind("image", &params, 1, args)?;
        let source = bound.string_or(0, "")?;
        if source.is_empty() {
            return Err(HostError::value_error(
                "image(): 'source' must be a non-empty project-relative path",
            ));
        }
        let fit_name = bound.string_or(3, "contain")?;
        let fit = ImageFit::from_name(&fit_name).ok_or_else(|| {
            HostError::value_error(format!(
                "image(): unknown fit {fit_name:?}; use \"contain\", \"cover\" or \"fill\""
            ))
        })?;
        let mut content = ImageContent::new(source.to_string());
        content.fit = fit;
        let dimension = |index: usize, name: &str| -> HostResult<f64> {
            match bound.get(index) {
                None | Some(Value::None) => Ok(0.0),
                Some(Value::Number(number)) => Ok(*number),
                Some(other) => Err(HostError::type_error(format!(
                    "image(): '{name}' must be a number or None, got {}",
                    other.type_name()
                ))),
            }
        };
        content.width = motioner_core::Animatable::new(dimension(1, "width")?);
        content.height = motioner_core::Animatable::new(dimension(2, "height")?);
        let node = self.create(NodeContent::Image(content), site)?;
        if let Value::Node(id) = node {
            for (index, property) in [(0, "source"), (1, "width"), (2, "height"), (3, "fit")] {
                self.record(id, property, bound.source(index));
            }
        }
        Ok(node)
    }

    /// `path(points, closed=False, ...)` (D-0029).
    fn path(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = [
            "points",
            "closed",
            "fill",
            "stroke",
            "stroke_width",
            "cap",
            "join",
            "trim_start",
            "trim_end",
            "trim_offset",
            "dash",
            "dash_offset",
        ];
        let bound = bind("path", &params, 1, args)?;
        let points = path_points(bound.get(0).unwrap_or(&Value::None))?;
        let closed = match bound.get(1) {
            None | Some(Value::None) => false,
            Some(Value::Bool(closed)) => *closed,
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "path(): 'closed' expects True or False, got {}",
                    other.type_name()
                )))
            }
        };
        let geometry = PathGeometry::new(points, closed, LineCap::Round, LineJoin::Round)
            .map_err(|error| core_error(&error))?;
        let node = self.path_node(geometry, &bound, 2, 5, 6, site)?;
        if let Value::Node(id) = node {
            let channels = [
                (ContentField::TrimStart, 7, 0.0),
                (ContentField::TrimEnd, 8, 1.0),
                (ContentField::TrimOffset, 9, 0.0),
                (ContentField::Dash, 10, 0.0),
                (ContentField::DashOffset, 11, 0.0),
            ];
            for (field, index, default) in channels {
                let value = bound.number_or(index, default)?;
                self.document_mut()?
                    .set_content(id, field, ContentValue::Number(value))
                    .map_err(|error| core_error(&error))?;
                self.record(id, &field.name(), bound.source(index));
            }
        }
        Ok(node)
    }

    /// `polygon(sides, radius, ...)`.
    fn polygon(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = [
            "sides",
            "radius",
            "fill",
            "stroke",
            "stroke_width",
            "cap",
            "join",
        ];
        let bound = bind("polygon", &params, 2, args)?;
        let sides = bound.integer_in(0, 3, 3, 1000)?;
        let sides = u32::try_from(sides)
            .map_err(|_| HostError::value_error("polygon(): 'sides' is out of range"))?;
        let geometry =
            PathGeometry::polygon(sides, bound.number(1)?).map_err(|error| core_error(&error))?;
        self.path_node(geometry, &bound, 2, 5, 6, site)
    }

    /// `star(points, inner, outer, ...)`.
    fn star(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = [
            "points",
            "inner",
            "outer",
            "fill",
            "stroke",
            "stroke_width",
            "cap",
            "join",
        ];
        let bound = bind("star", &params, 3, args)?;
        let spikes = bound.integer_in(0, 5, 2, 1000)?;
        let spikes = u32::try_from(spikes)
            .map_err(|_| HostError::value_error("star(): 'points' is out of range"))?;
        let geometry = PathGeometry::star(spikes, bound.number(1)?, bound.number(2)?)
            .map_err(|error| core_error(&error))?;
        self.path_node(geometry, &bound, 3, 6, 7, site)
    }

    /// `arc(radius, start=0, end=360, ...)`.
    fn arc(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = [
            "radius",
            "start",
            "end",
            "fill",
            "stroke",
            "stroke_width",
            "cap",
            "join",
        ];
        let bound = bind("arc", &params, 1, args)?;
        let geometry = PathGeometry::arc(
            bound.number(0)?,
            bound.number_or(1, 0.0)?,
            bound.number_or(2, 360.0)?,
        )
        .map_err(|error| core_error(&error))?;
        self.path_node(geometry, &bound, 3, 6, 7, site)
    }

    /// Creates a path node from a geometry and the paint/cap/join arguments
    /// starting at `paint_at`.
    #[allow(clippy::too_many_arguments)] // the constructor's flat argument slots
    fn path_node(
        &mut self,
        mut geometry: PathGeometry,
        bound: &Bound<'_>,
        paint_at: usize,
        cap_at: usize,
        join_at: usize,
        site: &CreationSite,
    ) -> HostResult<Value> {
        let cap_name = bound.string_or(cap_at, "round")?;
        let cap = LineCap::from_name(&cap_name).ok_or_else(|| {
            HostError::value_error(format!(
                "unknown cap {cap_name:?}; use \"butt\", \"round\" or \"square\""
            ))
        })?;
        let join_name = bound.string_or(join_at, "round")?;
        let join = LineJoin::from_name(&join_name).ok_or_else(|| {
            HostError::value_error(format!(
                "unknown join {join_name:?}; use \"miter\", \"bevel\" or \"round\""
            ))
        })?;
        geometry.set_cap(cap);
        geometry.set_join(join);
        let paint = path_paint(bound, paint_at)?;
        let node = self.create(NodeContent::path(geometry, paint.paint), site)?;
        if let Value::Node(id) = node {
            self.apply_gradients(id, paint)?;
            for (index, property) in [
                (paint_at, "fill"),
                (paint_at + 1, "stroke"),
                (paint_at + 2, "stroke_width"),
                (cap_at, "cap"),
                (join_at, "join"),
            ] {
                self.record(id, property, bound.source(index));
            }
        }
        Ok(node)
    }

    fn document_mut(&mut self) -> HostResult<&mut Document> {
        self.document
            .as_mut()
            .ok_or_else(|| HostError::value_error("call comp(...) before creating nodes"))
    }

    fn document(&self) -> HostResult<&Document> {
        self.document
            .as_ref()
            .ok_or_else(|| HostError::value_error("call comp(...) before using nodes"))
    }

    /// Attaches the gradients a constructor received (D-0030).
    fn apply_gradients(&mut self, id: NodeId, args: PaintArgs) -> HostResult<()> {
        if let Some(gradient) = args.fill_gradient {
            self.document_mut()?
                .set_content(
                    id,
                    ContentField::Fill,
                    ContentValue::Gradient(Some(gradient)),
                )
                .map_err(|error| core_error(&error))?;
        }
        if let Some(gradient) = args.stroke_gradient {
            self.document_mut()?
                .set_content(
                    id,
                    ContentField::Stroke,
                    ContentValue::Gradient(Some(gradient)),
                )
                .map_err(|error| core_error(&error))?;
        }
        Ok(())
    }

    fn create(&mut self, content: NodeContent, site: &CreationSite) -> HostResult<Value> {
        let max_nodes = self.max_nodes;
        let document = self.document_mut()?;
        if document.len() >= max_nodes {
            return Err(HostError::limit(format!(
                "node limit of {max_nodes} exceeded"
            )));
        }
        document
            .add_node_with_identity(content, NodeIdentity::provenance(site.key()))
            .map(Value::Node)
            .map_err(|error| core_error(&error))
    }

    /// Reads `node.name`.
    pub fn get_attribute(&self, id: NodeId, name: &str) -> HostResult<Value> {
        if let Some(method) = NodeMethod::from_name(name) {
            return Ok(Value::Method(id, method));
        }
        let node = self
            .document()?
            .node(id)
            .ok_or_else(|| HostError::value_error(format!("unknown node {id}")))?;
        let transform = node.transform();
        let value = match name {
            "pos" => vec2_value(transform.position.value()),
            "scale" => vec2_value(transform.scale.value()),
            "anchor" => vec2_value(transform.anchor.value()),
            "rotation" => Value::Number(transform.rotation.value()),
            "opacity" => Value::Number(transform.opacity.value()),
            "blend" => Value::Str(Rc::from(node.blend().name())),
            "visible" => Value::Bool(node.is_visible()),
            _ => content_value_of(node.content(), name).ok_or_else(|| unknown_attribute(name))?,
        };
        Ok(value)
    }

    /// Writes `node.name = value` and records where the value came from.
    pub fn set_attribute(
        &mut self,
        id: NodeId,
        name: &str,
        value: &Value,
        span: Span,
        literal: Option<Literal>,
    ) -> HostResult<()> {
        let result = if name == "blend" {
            let blend = blend_mode(value)?;
            self.document_mut()?
                .set_blend(id, blend)
                .map_err(|error| core_error(&error))
        } else if name == "visible" {
            let Value::Bool(visible) = value else {
                return Err(HostError::type_error(format!(
                    "'visible' expects True or False, got {}",
                    value.type_name()
                )));
            };
            self.document_mut()?
                .set_visible(id, *visible)
                .map_err(|error| core_error(&error))
        } else if let Some(field) = ContentField::from_name(name) {
            let accepts = self
                .document()?
                .node(id)
                .is_some_and(|node| node.content().field_exists(field));
            if !accepts {
                return Err(unknown_attribute(name));
            }
            let value = content_value(field, value)?;
            self.document_mut()?
                .set_content(id, field, value)
                .map_err(|error| core_error(&error))
        } else {
            let property = Property::from_name(name).ok_or_else(|| unknown_attribute(name))?;
            let value = property_value(property, value)?;
            self.document_mut()?
                .set_property(id, property, value)
                .map_err(|error| core_error(&error))
        };
        result?;
        self.record(id, name, Some((span, literal)));
        Ok(())
    }

    /// Calls `node.method(args)`.
    pub fn call_method(
        &mut self,
        id: NodeId,
        method: NodeMethod,
        args: CallArgs,
    ) -> HostResult<Value> {
        match method {
            NodeMethod::Key => self.key(id, args),
            NodeMethod::Add => self.add_children(id, &args),
            NodeMethod::Id => self.assign_id(id, args),
            NodeMethod::Effect => self.effect(id, args),
            NodeMethod::EffectStack => self.effect_stack(id, args),
            NodeMethod::Mask => self.mask(id, args),
            NodeMethod::AnimateChars => self.animate_chars(id, args),
            NodeMethod::ClearChars => self.clear_chars(id),
        }
    }

    /// `node.clear_chars()`: removes every per-character animator.
    ///
    /// The replacement form `bake` uses before writing a new animator list.
    fn clear_chars(&mut self, id: NodeId) -> HostResult<Value> {
        self.document_mut()?
            .set_text_animators(id, Vec::new())
            .map_err(|error| core_error(&error))?;
        Ok(Value::None)
    }

    /// `node.animate_chars(property, from, to, start, stagger, duration,
    /// ease="linear", order="forward")`: appends a per-character animator
    /// (D-0034).
    fn animate_chars(&mut self, id: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = [
            "property", "from", "to", "start", "stagger", "duration", "ease", "order",
        ];
        let bound = bind("animate_chars", &params, 6, args)?;
        let property_name = match bound.get(0) {
            Some(Value::Str(name)) => name.clone(),
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "animate_chars(): 'property' must be a string, got {}",
                    other.type_name()
                )))
            }
            None => return Err(HostError::type_error("animate_chars(): missing 'property'")),
        };
        let property = CharProperty::from_name(&property_name).ok_or_else(|| {
            HostError::value_error(format!(
                "animate_chars(): unknown property {property_name:?}; use opacity, offset, \
                 scale, rotation or color"
            ))
        })?;
        let from = char_value(property, bound.get(1), "from")?;
        let to = char_value(property, bound.get(2), "to")?;
        let start_seconds = bound.number(3)?;
        let stagger_seconds = bound.number(4)?;
        let duration_seconds = bound.number(5)?;
        let ease_name = bound.string_or(6, "linear")?;
        let ease = Ease::from_name(&ease_name).ok_or_else(|| {
            HostError::value_error(format!("animate_chars(): unknown ease {ease_name:?}"))
        })?;
        let order_name = bound.string_or(7, "forward")?;
        let order = CharOrder::from_name(&order_name).ok_or_else(|| {
            HostError::value_error(format!(
                "animate_chars(): unknown order {order_name:?}; use forward, reverse or center"
            ))
        })?;
        let rate = self.document()?.composition().frame_rate().as_f64();
        let start = self
            .document()?
            .composition()
            .frame_rate()
            .seconds_to_frame(start_seconds)
            .map_err(|error| core_error(&error))?;
        let stagger = stagger_seconds * rate;
        let duration = (duration_seconds * rate).max(1.0);
        let animator = TextAnimator::new(property, from, to, start, stagger, duration, ease, order)
            .map_err(|error| core_error(&error))?;
        self.document_mut()?
            .add_text_animator(id, animator)
            .map_err(|error| core_error(&error))?;
        Ok(Value::None)
    }

    /// `node.effect(kind, **params)`: adds the effect if absent and writes the
    /// parameters that were supplied (D-0028 section 3).
    ///
    /// The call is idempotent, so bake can append `node.effect("blur",
    /// radius=12)` without duplicating the effect.  Parameters are animatable
    /// channels: `node.key("blur.radius", time, value)` keys them.
    ///
    /// Supported arguments: `drop_shadow` takes `color`, `offset`, `blur` and
    /// `opacity`; `blur` takes `radius`; `glow` takes `color`, `radius` and
    /// `intensity`.
    fn effect(&mut self, id: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = [
            "kind",
            "color",
            "offset",
            "blur",
            "opacity",
            "radius",
            "intensity",
        ];
        let bound = bind("effect", &params, 1, args)?;
        let kind_name = bound.string_or(0, "")?;
        let kind = EffectKind::from_name(&kind_name).ok_or_else(|| {
            HostError::new(
                ErrorCategory::Unsupported,
                format!("unknown effect kind {kind_name:?}; expected drop_shadow, blur or glow"),
            )
        })?;
        let supplied = [
            (1_usize, EffectParam::Color),
            (2, EffectParam::Offset),
            (3, EffectParam::Blur),
            (4, EffectParam::Opacity),
            (5, EffectParam::Radius),
            (6, EffectParam::Intensity),
        ];
        for (index, param) in supplied {
            if bound.get(index).is_some() && !kind.accepts(param) {
                return Err(HostError::type_error(format!(
                    "effect(): '{}' does not take '{}'",
                    kind.name(),
                    param.name()
                )));
            }
        }
        if self.document()?.node(id).is_none() {
            return Err(HostError::value_error(format!("unknown node {id}")));
        }
        let exists = self
            .document()?
            .node(id)
            .is_some_and(|node| node.effect(kind).is_some());
        if !exists {
            self.document_mut()?
                .push_effect(id, Effect::new(kind))
                .map_err(|error| core_error(&error))?;
        }
        if let (true, Some(source)) = (bound.get(1).is_some(), bound.source(1)) {
            let color = bound.color_or(1, Color::BLACK)?;
            let channel = EffectChannel {
                kind,
                param: EffectParam::Color,
                axis: None,
            };
            self.write_effect_param(id, channel, EffectValueInput::Color(color), source)?;
        }
        if let (true, Some(source)) = (bound.get(2).is_some(), bound.source(2)) {
            let offset = as_vec2(bound.get(2).unwrap())
                .ok_or_else(|| HostError::type_error("effect(): 'offset' expects two numbers"))?;
            for (axis, value) in [
                (motioner_core::Axis::X, offset.x),
                (motioner_core::Axis::Y, offset.y),
            ] {
                let channel = EffectChannel {
                    kind,
                    param: EffectParam::Offset,
                    axis: Some(axis),
                };
                self.write_effect_param(id, channel, EffectValueInput::Number(value), source)?;
            }
        }
        for (index, param) in [
            (3_usize, EffectParam::Blur),
            (4, EffectParam::Opacity),
            (5, EffectParam::Radius),
            (6, EffectParam::Intensity),
        ] {
            if let (true, Some(source)) = (bound.get(index).is_some(), bound.source(index)) {
                let number = bound.number(index)?;
                let channel = EffectChannel {
                    kind,
                    param,
                    axis: None,
                };
                self.write_effect_param(id, channel, EffectValueInput::Number(number), source)?;
            }
        }
        Ok(Value::Node(id))
    }

    /// `node.mask(source, mode="alpha", invert=False)`: masks this node with
    /// the drawn subtree of `source` (D-0033).
    ///
    /// The source is not drawn on its own; it becomes the coverage of this
    /// node's offscreen layer.  `source` is a node or its identity string;
    /// `mode` is `"alpha"` (the default) or `"luma"`, and `invert` swaps the
    /// coverage.  `node.mask(None)` clears the mask, which is what `bake`
    /// writes for a cleared override.
    fn mask(&mut self, id: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("mask", &["source", "mode", "invert"], 1, args)?;
        if self.document()?.node(id).is_none() {
            return Err(HostError::value_error(format!("unknown node {id}")));
        }
        if matches!(bound.get(0), Some(Value::None)) {
            self.document_mut()?
                .clear_mask(id)
                .map_err(|error| core_error(&error))?;
            return Ok(Value::Node(id));
        }
        let source = match bound.get(0) {
            Some(Value::Node(node)) => *node,
            Some(Value::Str(name)) => self.document()?.find(name).ok_or_else(|| {
                HostError::value_error(format!("mask(): no node has the identity {name:?}"))
            })?,
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "mask(): 'source' expects a node, got {}",
                    other.type_name()
                )))
            }
            None => {
                return Err(HostError::type_error(
                    "mask(): missing required argument 'source'",
                ))
            }
        };
        let mode_name = bound.string_or(1, MaskMode::Alpha.name())?;
        let mode = MaskMode::from_name(&mode_name).ok_or_else(|| {
            HostError::value_error(format!(
                "mask(): unknown mode {mode_name:?}; expected alpha or luma"
            ))
        })?;
        let invert = match bound.get(2) {
            None => false,
            Some(Value::Bool(flag)) => *flag,
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "mask(): 'invert' expects True or False, got {}",
                    other.type_name()
                )))
            }
        };
        self.document_mut()?
            .set_mask(id, source, mode, invert)
            .map_err(|error| core_error(&error))?;
        Ok(Value::Node(id))
    }

    /// `node.effect_stack("blur,glow")`: rebuilds the stack in that order.
    ///
    /// The bulk form of [`Scene::effect`], used by `bake` and by agents: kinds
    /// missing from the list are removed, new ones are added with their
    /// defaults, and the parameters of the ones that stay are preserved.
    fn effect_stack(&mut self, id: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("effect_stack", &["kinds"], 1, args)?;
        let list = bound.string_or(0, "")?;
        let mut kinds = Vec::new();
        for name in list
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let kind = EffectKind::from_name(name).ok_or_else(|| {
                HostError::value_error(format!(
                    "effect_stack(): unknown effect kind {name:?}; expected drop_shadow, blur or glow"
                ))
            })?;
            if kinds.contains(&kind) {
                return Err(HostError::value_error(format!(
                    "effect_stack(): {name:?} appears twice"
                )));
            }
            kinds.push(kind);
        }
        self.document_mut()?
            .set_effect_stack(id, &kinds)
            .map_err(|error| core_error(&error))?;
        Ok(Value::Node(id))
    }

    /// Writes one static effect parameter and records where it came from.
    fn write_effect_param(
        &mut self,
        id: NodeId,
        channel: EffectChannel,
        value: EffectValueInput,
        source: (Span, Option<Literal>),
    ) -> HostResult<()> {
        self.document_mut()?
            .set_effect(id, channel, value)
            .map_err(|error| core_error(&error))?;
        self.record(id, &channel.name(), Some(source));
        Ok(())
    }

    /// Gives a node an explicit identity: `node.id("name")`.
    fn assign_id(&mut self, id: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("id", &["name"], 1, args)?;
        let name = bound.string_or(0, "")?;
        self.document_mut()?
            .assign_id(id, &name)
            .map_err(|error| core_error(&error))?;
        // Chainable: `card = rect(1, 1).id("card")` must bind the node.
        Ok(Value::Node(id))
    }

    fn key(&mut self, id: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("key", &["property", "time", "value", "ease"], 3, args)?;
        let property_name = bound.string_or(0, "")?;
        if let Some(channel) = EffectChannel::from_name(&property_name) {
            return self.key_effect(id, channel, &property_name, &bound);
        }
        if let Some((kind, rest)) = property_name.split_once('.') {
            if EffectKind::from_name(kind).is_some() && rest.ends_with(".offset") {
                return Err(HostError::value_error(format!(
                    "key(): '{property_name}' needs an axis ('{property_name}.x' or '{property_name}.y')"
                )));
            }
        }
        let (base, axis) = split_axis(&property_name);
        if let Some(field) = ContentField::from_name(base) {
            return self.key_content(id, field, &property_name, axis, &bound);
        }
        let property = Property::from_name(base).ok_or_else(|| {
            HostError::value_error(format!(
                "key(): unknown property {property_name:?}; animatable properties are {} and the content fields {} (a vector axis is written \"pos.x\")",
                quoted_names(Property::ALL.iter().map(|(_, n)| *n)),
                quoted_names(
                    ContentField::ALL
                        .iter()
                        .filter(|(field, _)| field.is_animatable())
                        .map(|(_, n)| *n)
                )
            ))
        })?;
        if !property.is_vector() && axis.is_some() {
            return Err(HostError::value_error(format!(
                "key(): '{}' has no axes",
                property.name()
            )));
        }
        let time = bound.number(1)?;
        if !time.is_finite() {
            return Err(HostError::value_error(
                "key(): 'time' must be a finite number of seconds",
            ));
        }
        let axis_value = match axis {
            Some(axis) => {
                let Some(Value::Number(number)) = bound.get(2) else {
                    return Err(HostError::type_error("key(): an axis key expects a number"));
                };
                if !number.is_finite() {
                    return Err(HostError::value_error(
                        "key(): the value must be a finite number",
                    ));
                }
                Some((axis, *number))
            }
            None => None,
        };
        let value = match axis_value {
            Some(_) => None,
            None => Some(property_value(
                property,
                bound.get(2).unwrap_or(&Value::None),
            )?),
        };
        let timing = self.key_timing(&bound, time)?;
        let document = self.document_mut()?;
        match (axis_value, value) {
            (Some((axis, number)), _) => document
                .add_axis_keyframe(id, property, axis, timing.frame, number, timing.ease)
                .map_err(|error| core_error(&error))?,
            (None, Some(value)) => document
                .add_keyframe(id, property, timing.frame, value, timing.ease)
                .map_err(|error| core_error(&error))?,
            (None, None) => {}
        }
        self.record_key(id, &property_name, &timing, bound.source(2));
        Ok(Value::None)
    }

    /// `node.key("fill", time, "#RRGGBB", ease=...)` on a content field.
    fn key_content(
        &mut self,
        id: NodeId,
        field: ContentField,
        property_name: &str,
        axis: Option<motioner_core::Axis>,
        bound: &Bound<'_>,
    ) -> HostResult<Value> {
        if axis.is_some() {
            return Err(HostError::value_error(format!(
                "key(): '{property_name}' has no axes"
            )));
        }
        if !field.is_animatable() {
            return Err(HostError::value_error(format!(
                "key(): '{}' is discrete and cannot be animated",
                field.name()
            )));
        }
        let time = bound.number(1)?;
        if !time.is_finite() {
            return Err(HostError::value_error(
                "key(): 'time' must be a finite number of seconds",
            ));
        }
        let value = content_value(field, bound.get(2).unwrap_or(&Value::None))?;
        let timing = self.key_timing(bound, time)?;
        self.document_mut()?
            .add_content_keyframe(id, field, timing.frame, value, timing.ease)
            .map_err(|error| core_error(&error))?;
        self.record_key(id, property_name, &timing, bound.source(2));
        Ok(Value::None)
    }

    /// The frame and ease of a `key(...)` call, with the ease's source span.
    /// `node.key("glow.radius", time, value)` on an effect parameter.
    fn key_effect(
        &mut self,
        id: NodeId,
        channel: EffectChannel,
        property_name: &str,
        bound: &Bound<'_>,
    ) -> HostResult<Value> {
        let time = bound.number(1)?;
        if !time.is_finite() {
            return Err(HostError::value_error(
                "key(): 'time' must be a finite number of seconds",
            ));
        }
        let value = match channel.param {
            EffectParam::Color => {
                let color = bound.color_or(2, Color::BLACK)?;
                EffectValueInput::Color(color)
            }
            EffectParam::Offset if channel.axis.is_none() => {
                return Err(HostError::value_error(format!(
                    "key(): '{property_name}' needs an axis ('{property_name}.x' or '{property_name}.y')"
                )))
            }
            _ => EffectValueInput::Number(bound.number(2)?),
        };
        let timing = self.key_timing(bound, time)?;
        let Some(node) = self.document()?.node(id) else {
            return Err(HostError::value_error(format!("unknown node {id}")));
        };
        if node.effect(channel.kind).is_none() {
            return Err(HostError::value_error(format!(
                "key(): '{property_name}' needs a '{}' effect on this layer",
                channel.kind.name()
            )));
        }
        self.document_mut()?
            .add_effect_keyframe(id, channel, timing.frame, value, timing.ease)
            .map_err(|error| core_error(&error))?;
        self.record_key(id, property_name, &timing, bound.source(2));
        Ok(Value::None)
    }

    fn key_timing(&self, bound: &Bound<'_>, time: f64) -> HostResult<KeyTiming> {
        let ease_name = bound.string_or(3, "linear")?;
        let ease = Ease::from_name(&ease_name).ok_or_else(|| {
            HostError::value_error(format!(
                "key(): unknown ease {ease_name:?}; available eases are {}",
                quoted_names(Ease::ALL.iter().map(|(_, n)| *n))
            ))
        })?;
        let frame = self
            .document()?
            .composition()
            .frame_rate()
            .seconds_to_frame(time)
            .map_err(|error| core_error(&error))?;
        let ease_origin = bound
            .source(3)
            .filter(|(_, literal)| *literal == Some(Literal::Text))
            .map(|(span, _)| EaseOrigin {
                span,
                name: ease_name.to_string(),
            });
        Ok(KeyTiming {
            frame,
            ease,
            ease_origin,
        })
    }

    /// Records where a keyed value came from, so bake can edit it in place.
    fn record_key(
        &mut self,
        id: NodeId,
        property_name: &str,
        timing: &KeyTiming,
        source: Option<(Span, Option<Literal>)>,
    ) {
        if let Some((span, literal)) = source {
            self.origins.record(ValueOrigin {
                node: id,
                property: property_name.to_string(),
                span,
                literal,
                kind: OriginKind::Key(timing.frame),
                ease: timing.ease_origin.clone(),
            });
        }
    }

    fn add_children(&mut self, id: NodeId, args: &CallArgs) -> HostResult<Value> {
        if !args.keywords.is_empty() {
            return Err(HostError::type_error("add() takes no keyword arguments"));
        }
        let document = self.document_mut()?;
        for child in &args.positional {
            let Value::Node(child) = &child.value else {
                return Err(HostError::type_error(format!(
                    "add() expects nodes, got {}",
                    child.value.type_name()
                )));
            };
            document
                .reparent(*child, id)
                .map_err(|error| core_error(&error))?;
        }
        Ok(Value::None)
    }
}

/// Splits `pos.x` into (`pos`, `Some(X)`); a plain name has no axis.
fn split_axis(property: &str) -> (&str, Option<motioner_core::Axis>) {
    if let Some((base, suffix)) = property.rsplit_once('.') {
        if let Some(axis) = motioner_core::Axis::from_name(suffix) {
            return (base, Some(axis));
        }
    }
    (property, None)
}

/// Paint arguments: solid fallbacks plus the gradients they replace (D-0030).
struct PaintArgs {
    paint: Paint,
    fill_gradient: Option<Gradient>,
    stroke_gradient: Option<Gradient>,
}

/// Reads optional `fill`, `stroke` and `stroke_width` starting at `first`.
fn paint(bound: &Bound<'_>, first: usize) -> HostResult<PaintArgs> {
    let fill = bound.optional_paint(first, Some(PaintInput::Color(Color::WHITE)))?;
    let stroke_color = bound.optional_paint(first + 1, None)?;
    let stroke_width = bound.number_or(first + 2, 0.0)?;
    let (fill, fill_gradient) = split_paint(fill);
    let (stroke_color, stroke_gradient) = split_paint(stroke_color);
    let stroke = stroke_color
        .filter(|_| stroke_width != 0.0)
        .map(|color| Stroke {
            color,
            width: stroke_width,
        });
    Ok(PaintArgs {
        paint: Paint { fill, stroke },
        fill_gradient,
        stroke_gradient,
    })
}

/// Splits a paint argument into its solid fallback and its gradient.
fn split_paint(input: Option<PaintInput>) -> (Option<Color>, Option<Gradient>) {
    match input {
        None => (None, None),
        Some(PaintInput::Color(color)) => (Some(color), None),
        Some(PaintInput::Gradient(gradient)) => {
            (Some(gradient.fallback_color()), Some((*gradient).clone()))
        }
    }
}

/// Reads `path()` points: each is `(x, y)` or
/// `(x, y, in_dx, in_dy, out_dx, out_dy)`.
fn path_points(value: &Value) -> HostResult<Vec<PathPoint>> {
    let items: Vec<Value> = match value {
        Value::List(items) | Value::Tuple(items) => items.borrow().clone(),
        other => {
            return Err(HostError::type_error(format!(
                "path(): 'points' must be a list of (x, y) points, got {}",
                other.type_name()
            )))
        }
    };
    if items.len() < 2 {
        return Err(HostError::value_error("path(): needs at least two points"));
    }
    items
        .iter()
        .map(|item| {
            let numbers: Vec<f64> = match item {
                Value::List(values) | Value::Tuple(values) => values
                    .borrow()
                    .iter()
                    .map_while(|value| match value {
                        Value::Number(number) => Some(*number),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            match numbers.as_slice() {
                [x, y] => Ok(PathPoint::corner(Vec2::new(*x, *y))),
                [x, y, in_x, in_y, out_x, out_y] => Ok(PathPoint::curved(
                    Vec2::new(*x, *y),
                    Vec2::new(*in_x, *in_y),
                    Vec2::new(*out_x, *out_y),
                )),
                _ => Err(HostError::value_error(
                    "path(): a point is (x, y) or (x, y, in_dx, in_dy, out_dx, out_dy)",
                )),
            }
        })
        .collect()
}

/// Reads optional `fill`, `stroke` and `stroke_width` starting at `first`,
/// with the path defaults (no fill, white 2 px stroke).
fn path_paint(bound: &Bound<'_>, first: usize) -> HostResult<PaintArgs> {
    let fill = bound.optional_paint(first, None)?;
    let stroke_color = bound.optional_paint(first + 1, Some(PaintInput::Color(Color::WHITE)))?;
    let stroke_width = bound.number_or(first + 2, 2.0)?;
    let (fill, fill_gradient) = split_paint(fill);
    let (stroke_color, stroke_gradient) = split_paint(stroke_color);
    let stroke = stroke_color
        .filter(|_| stroke_width != 0.0)
        .map(|color| Stroke {
            color,
            width: stroke_width,
        });
    Ok(PaintArgs {
        paint: Paint { fill, stroke },
        fill_gradient,
        stroke_gradient,
    })
}

/// Maps a decimal or integer fps to an exact rational frame rate.
fn frame_rate(fps: f64) -> HostResult<FrameRate> {
    let integer = as_integer(fps)
        .filter(|n| (1..=MAX_FPS).contains(n))
        .and_then(|n| u32::try_from(n).ok());
    let rate = if let Some(n) = integer {
        FrameRate::new(n, 1)
    } else {
        let ntsc = NTSC_RATES
            .iter()
            .find(|(approx, _)| (fps - approx).abs() < 1e-3)
            .ok_or_else(|| {
                HostError::value_error(format!(
                    "comp(): 'fps' must be a whole number from 1 to {MAX_FPS} or one of 23.976, 29.97, 59.94"
                ))
            })?;
        FrameRate::new(ntsc.1, 1001)
    };
    rate.map_err(|error| core_error(&error))
}

fn blend_mode(value: &Value) -> HostResult<BlendMode> {
    let Value::Str(name) = value else {
        return Err(HostError::type_error(format!(
            "'blend' expects a string, got {}",
            value.type_name()
        )));
    };
    BlendMode::from_name(name).ok_or_else(|| {
        HostError::new(
            ErrorCategory::Unsupported,
            format!(
                "unknown blend mode {name:?}; expected normal, add, multiply, screen, overlay, darken or lighten"
            ),
        )
    })
}

/// Converts an MDF value to a property value of the expected shape.
fn property_value(property: Property, value: &Value) -> HostResult<PropertyValue> {
    if let Value::Number(n) = value {
        return Ok(PropertyValue::Scalar(*n));
    }
    if let Some(v) = as_vec2(value) {
        return Ok(PropertyValue::Vec2(v));
    }
    Err(HostError::type_error(format!(
        "'{}' expects a number or an (x, y) pair, got {}",
        property.name(),
        value.type_name()
    )))
}

/// Reads a content field as an MDF value, or `None` when the kind has no such
/// field.
fn content_value_of(content: &NodeContent, name: &str) -> Option<Value> {
    let field = ContentField::from_name(name)?;
    Some(match content.static_value(field)? {
        ContentValue::Number(number) => Value::Number(number),
        ContentValue::Color(Some(color)) => Value::Str(Rc::from(color.to_hex())),
        ContentValue::Color(None) | ContentValue::Gradient(None) => Value::None,
        ContentValue::Text(text) => Value::Str(Rc::from(text)),
        ContentValue::Align(align) => Value::Str(Rc::from(match align {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        })),
        ContentValue::Cap(cap) => Value::Str(Rc::from(cap.name())),
        ContentValue::Join(join) => Value::Str(Rc::from(join.name())),
        ContentValue::Fit(fit) => Value::Str(Rc::from(fit.name())),
        ContentValue::Gradient(Some(gradient)) => Value::Gradient(Rc::new(gradient)),
    })
}

/// Converts an MDF value to a content field write.
/// Converts one `animate_chars` endpoint to the property's value kind.
fn char_value(property: CharProperty, value: Option<&Value>, role: &str) -> HostResult<CharValue> {
    let Some(value) = value else {
        return Err(HostError::type_error(format!(
            "animate_chars(): missing '{role}'"
        )));
    };
    match property {
        CharProperty::Opacity | CharProperty::Scale | CharProperty::Rotation => match value {
            Value::Number(number) => Ok(CharValue::Number(*number)),
            other => Err(HostError::type_error(format!(
                "animate_chars(): '{role}' for {} must be a number, got {}",
                property.name(),
                other.type_name()
            ))),
        },
        CharProperty::Offset => as_vec2(value).map(CharValue::Vec2).ok_or_else(|| {
            HostError::type_error(format!(
                "animate_chars(): '{role}' for offset must be an (x, y) pair"
            ))
        }),
        CharProperty::Color => match value {
            Value::Str(text) => Color::parse_hex(text)
                .map(CharValue::Color)
                .map_err(|error| {
                    HostError::value_error(format!("animate_chars(): '{role}': {error}"))
                }),
            other => Err(HostError::type_error(format!(
                "animate_chars(): '{role}' for color must be a colour string, got {}",
                other.type_name()
            ))),
        },
    }
}

#[allow(clippy::too_many_lines)] // one arm per content field, kept together
fn content_value(field: ContentField, value: &Value) -> HostResult<ContentValue> {
    let number = |value: &Value| -> HostResult<ContentValue> {
        match value {
            Value::Number(n) => Ok(ContentValue::Number(*n)),
            other => Err(HostError::type_error(format!(
                "'{}' expects a number, got {}",
                field.name(),
                other.type_name()
            ))),
        }
    };
    match field {
        ContentField::Width
        | ContentField::Height
        | ContentField::Radius
        | ContentField::StrokeWidth
        | ContentField::Size
        | ContentField::Weight
        | ContentField::Tracking
        | ContentField::LineHeight
        | ContentField::TrimStart
        | ContentField::TrimEnd
        | ContentField::TrimOffset
        | ContentField::Dash
        | ContentField::DashOffset
        | ContentField::FillStopOffset(_)
        | ContentField::StrokeStopOffset(_) => number(value),
        ContentField::FillStopColor(_) | ContentField::StrokeStopColor(_) => match value {
            Value::Str(text) => Color::parse_hex(text)
                .map(|color| ContentValue::Color(Some(color)))
                .map_err(|error| HostError::value_error(format!("{}(): {error}", field.name()))),
            other => Err(HostError::type_error(format!(
                "'{}' expects a colour string, got {}",
                field.name(),
                other.type_name()
            ))),
        },
        ContentField::Fill | ContentField::Stroke | ContentField::Color => match value {
            Value::None if field != ContentField::Color => Ok(ContentValue::Color(None)),
            Value::None => Ok(ContentValue::Gradient(None)),
            Value::Str(text) => Color::parse_hex(text)
                .map(|color| ContentValue::Color(Some(color)))
                .map_err(|error| HostError::value_error(format!("{}(): {error}", field.name()))),
            Value::Gradient(gradient) => Ok(ContentValue::Gradient(Some((**gradient).clone()))),
            other => Err(HostError::type_error(format!(
                "'{}' expects a colour string or a gradient, got {}",
                field.name(),
                other.type_name()
            ))),
        },
        ContentField::Text => match value {
            Value::Str(text) => Ok(ContentValue::Text(text.to_string())),
            other => Err(HostError::type_error(format!(
                "'text' expects a string, got {}",
                other.type_name()
            ))),
        },
        ContentField::Align => match value {
            Value::Str(text) => TextAlign::from_name(text)
                .map(ContentValue::Align)
                .ok_or_else(|| {
                    HostError::value_error(format!(
                        "'align' must be \"left\", \"center\" or \"right\", got {text:?}"
                    ))
                }),
            other => Err(HostError::type_error(format!(
                "'align' expects a string, got {}",
                other.type_name()
            ))),
        },
        ContentField::Cap => match value {
            Value::Str(text) => LineCap::from_name(text)
                .map(ContentValue::Cap)
                .ok_or_else(|| {
                    HostError::value_error(format!(
                        "'cap' must be \"butt\", \"round\" or \"square\", got {text:?}"
                    ))
                }),
            other => Err(HostError::type_error(format!(
                "'cap' expects a string, got {}",
                other.type_name()
            ))),
        },
        ContentField::Join => match value {
            Value::Str(text) => LineJoin::from_name(text)
                .map(ContentValue::Join)
                .ok_or_else(|| {
                    HostError::value_error(format!(
                        "'join' must be \"miter\", \"bevel\" or \"round\", got {text:?}"
                    ))
                }),
            other => Err(HostError::type_error(format!(
                "'join' expects a string, got {}",
                other.type_name()
            ))),
        },
        ContentField::Source => match value {
            Value::Str(text) => Ok(ContentValue::Text(text.to_string())),
            other => Err(HostError::type_error(format!(
                "'source' expects a project-relative path string, got {}",
                other.type_name()
            ))),
        },
        ContentField::Fit => match value {
            Value::Str(text) => ImageFit::from_name(text)
                .map(ContentValue::Fit)
                .ok_or_else(|| {
                    HostError::value_error(format!(
                        "'fit' must be \"contain\", \"cover\" or \"fill\", got {text:?}"
                    ))
                }),
            other => Err(HostError::type_error(format!(
                "'fit' expects a string, got {}",
                other.type_name()
            ))),
        },
    }
}

fn core_error(error: &CoreError) -> HostError {
    let category = match error {
        CoreError::PropertyType { .. } => ErrorCategory::Type,
        _ => ErrorCategory::Value,
    };
    HostError::new(category, error.to_string())
}

fn unknown_attribute(name: &str) -> HostError {
    HostError::new(
        ErrorCategory::Name,
        format!(
            "nodes have no attribute '{name}'; use a transform property, a content field, or key/add/id"
        ),
    )
}

fn quoted_names<'a>(names: impl Iterator<Item = &'a str>) -> String {
    names
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
