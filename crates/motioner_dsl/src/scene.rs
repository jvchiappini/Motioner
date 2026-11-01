//! The 2D scene API: `comp`, node constructors, node properties and the
//! `key` / `add` methods (spec section 7, v0 subset).
//!
//! This is the only place where MDF values become document mutations, and
//! every mutation goes through the `motioner_core::Document` API.

use std::fmt::Write as _;
use std::rc::Rc;

use motioner_core::{
    BlendMode, Color, Composition, ContentField, ContentValue, CoreError, Document, Ease,
    FrameRate, NodeContent, NodeId, NodeIdentity, Paint, Property, PropertyValue, Stroke,
    TextAlign, TextContent, MAX_CANVAS_SIZE,
};

use crate::diagnostic::{ErrorCategory, Span};
use crate::host::{as_integer, as_vec2, bind, vec2_value, Bound, CallArgs, HostError, HostResult};
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
                let content = NodeContent::Rect {
                    width: bound.number(0)?,
                    height: bound.number(1)?,
                    radius: bound.number_or(2, 0.0)?,
                    paint: paint(&bound, 3)?,
                };
                let node = self.create(content, site)?;
                if let Value::Node(id) = node {
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
                let content = NodeContent::Circle {
                    radius: bound.number(0)?,
                    paint: paint(&bound, 1)?,
                };
                let node = self.create(content, site)?;
                if let Value::Node(id) = node {
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
        let params = ["content", "font", "size", "color", "align"];
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
        let content = NodeContent::Text(TextContent {
            text: content.to_string(),
            font: bound.string_or(1, "Inter")?.to_string(),
            size: bound.number_or(2, 48.0)?,
            color: bound.color_or(3, Color::WHITE)?,
            align,
        });
        let node = self.create(content, site)?;
        if let Value::Node(id) = node {
            for (index, property) in [(0, "text"), (2, "size"), (3, "color"), (4, "align")] {
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
                .is_some_and(|node| node.content().accepts(field));
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
        }
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
        let (base, axis) = split_axis(&property_name);
        let property = Property::from_name(base).ok_or_else(|| {
            HostError::value_error(format!(
                "key(): unknown property {property_name:?}; animatable properties are {} (a vector axis is written \"pos.x\")",
                quoted_names(Property::ALL.iter().map(|(_, n)| *n))
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
        let ease_name = bound.string_or(3, "linear")?;
        let ease = Ease::from_name(&ease_name).ok_or_else(|| {
            HostError::value_error(format!(
                "key(): unknown ease {ease_name:?}; available eases are {}",
                quoted_names(Ease::ALL.iter().map(|(_, n)| *n))
            ))
        })?;
        let document = self.document_mut()?;
        let frame = document
            .composition()
            .frame_rate()
            .seconds_to_frame(time)
            .map_err(|error| core_error(&error))?;
        match (axis_value, value) {
            (Some((axis, number)), _) => document
                .add_axis_keyframe(id, property, axis, frame, number, ease)
                .map_err(|error| core_error(&error))?,
            (None, Some(value)) => document
                .add_keyframe(id, property, frame, value, ease)
                .map_err(|error| core_error(&error))?,
            (None, None) => {}
        }
        let source = bound.source(2);
        let ease_origin = bound
            .source(3)
            .filter(|(_, literal)| *literal == Some(Literal::Text))
            .map(|(span, _)| EaseOrigin {
                span,
                name: ease_name.to_string(),
            });
        if let Some((span, literal)) = source {
            self.origins.record(ValueOrigin {
                node: id,
                property: property_name.to_string(),
                span,
                literal,
                kind: OriginKind::Key(frame),
                ease: ease_origin,
            });
        }
        Ok(Value::None)
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

/// Reads optional `fill`, `stroke` and `stroke_width` starting at `first`.
fn paint(bound: &Bound<'_>, first: usize) -> HostResult<Paint> {
    let fill = bound.optional_color(first, Some(Color::WHITE))?;
    let stroke_color = bound.optional_color(first + 1, None)?;
    let stroke_width = bound.number_or(first + 2, 0.0)?;
    let stroke = stroke_color
        .filter(|_| stroke_width != 0.0)
        .map(|color| Stroke {
            color,
            width: stroke_width,
        });
    Ok(Paint { fill, stroke })
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
            format!("blend mode {name:?} is not supported in MDF v0; use \"normal\""),
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
    if !content.accepts(field) {
        return None;
    }
    let value = match (content, field) {
        (NodeContent::Rect { width, .. }, ContentField::Width) => Value::Number(*width),
        (NodeContent::Rect { height, .. }, ContentField::Height) => Value::Number(*height),
        (
            NodeContent::Rect { radius, .. } | NodeContent::Circle { radius, .. },
            ContentField::Radius,
        ) => Value::Number(*radius),
        (
            NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. },
            ContentField::Fill,
        ) => paint.fill.map_or(Value::None, color_value),
        (
            NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. },
            ContentField::Stroke,
        ) => match paint.stroke {
            Some(stroke) => color_value(stroke.color),
            None => Value::None,
        },
        (
            NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. },
            ContentField::StrokeWidth,
        ) => Value::Number(paint.stroke.map_or(0.0, |stroke| stroke.width)),
        (NodeContent::Text(text), ContentField::Text) => Value::Str(Rc::from(text.text.as_str())),
        (NodeContent::Text(text), ContentField::Size) => Value::Number(text.size),
        (NodeContent::Text(text), ContentField::Color) => color_value(text.color),
        (NodeContent::Text(text), ContentField::Align) => Value::Str(Rc::from(match text.align {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        })),
        _ => return None,
    };
    Some(value)
}

/// A colour as the `#RRGGBB[AA]` string the constructors accept.
fn color_value(color: Color) -> Value {
    let text = if color.a == 255 {
        format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            color.r, color.g, color.b, color.a
        )
    };
    Value::Str(Rc::from(text))
}

/// Converts an MDF value to a content field write.
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
        | ContentField::Size => number(value),
        ContentField::Fill | ContentField::Stroke | ContentField::Color => match value {
            Value::None if field != ContentField::Color => Ok(ContentValue::Color(None)),
            Value::Str(text) => Color::parse_hex(text)
                .map(|color| ContentValue::Color(Some(color)))
                .map_err(|error| HostError::value_error(format!("{}(): {error}", field.name()))),
            other => Err(HostError::type_error(format!(
                "'{}' expects a colour string, got {}",
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
