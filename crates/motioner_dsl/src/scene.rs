//! The 2D scene API: `comp`, node constructors, node properties and the
//! `key` / `add` methods (spec section 7, v0 subset).
//!
//! This is the only place where MDF values become document mutations, and
//! every mutation goes through the `motioner_core::Document` API.

use std::rc::Rc;

use motioner_core::{
    BlendMode, Color, Composition, CoreError, Document, Ease, FrameRate, NodeContent, NodeId,
    Paint, Property, PropertyValue, Stroke, TextAlign, TextContent, MAX_CANVAS_SIZE,
};

use crate::diagnostic::ErrorCategory;
use crate::host::{as_integer, as_vec2, bind, vec2_value, Bound, CallArgs, HostError, HostResult};
use crate::value::{Builtin, NodeMethod, Value};

/// NTSC-style frame rates written as decimals, mapped to exact rationals.
const NTSC_RATES: [(f64, u32); 3] = [(23.976, 24_000), (29.97, 30_000), (59.94, 60_000)];

/// Highest integer frame rate accepted by `comp()`.
const MAX_FPS: i64 = 1000;

/// Builds the document while a program runs.
pub(crate) struct Scene {
    document: Option<Document>,
    max_nodes: usize,
}

impl Scene {
    pub fn new(max_nodes: usize) -> Self {
        Self {
            document: None,
            max_nodes,
        }
    }

    /// The finished document, if `comp()` was called.
    pub fn into_document(self) -> Option<Document> {
        self.document
    }

    /// Calls a scene constructor builtin.
    pub fn call(&mut self, builtin: Builtin, args: CallArgs) -> HostResult<Value> {
        match builtin {
            Builtin::Comp => self.comp(args),
            Builtin::Group => {
                bind("group", &[], 0, args)?;
                self.create(NodeContent::Group)
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
                self.create(content)
            }
            Builtin::Circle => {
                let params = ["radius", "fill", "stroke", "stroke_width"];
                let bound = bind("circle", &params, 1, args)?;
                let content = NodeContent::Circle {
                    radius: bound.number(0)?,
                    paint: paint(&bound, 1)?,
                };
                self.create(content)
            }
            Builtin::Text => self.text(args),
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
            frame_rate.seconds_to_frame(duration),
            bound.color_or(5, Color::BLACK)?,
        )
        .map_err(|error| core_error(&error))?;
        self.document = Some(Document::new(composition));
        Ok(Value::None)
    }

    fn text(&mut self, args: CallArgs) -> HostResult<Value> {
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
        self.create(content)
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

    fn create(&mut self, content: NodeContent) -> HostResult<Value> {
        let max_nodes = self.max_nodes;
        let document = self.document_mut()?;
        if document.len() >= max_nodes {
            return Err(HostError::limit(format!(
                "node limit of {max_nodes} exceeded"
            )));
        }
        document
            .add_node(content)
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
            _ => return Err(unknown_attribute(name)),
        };
        Ok(value)
    }

    /// Writes `node.name = value`.
    pub fn set_attribute(&mut self, id: NodeId, name: &str, value: &Value) -> HostResult<()> {
        if name == "blend" {
            let blend = blend_mode(value)?;
            return self
                .document_mut()?
                .set_blend(id, blend)
                .map_err(|error| core_error(&error));
        }
        let property = Property::from_name(name).ok_or_else(|| unknown_attribute(name))?;
        let value = property_value(property, value)?;
        self.document_mut()?
            .set_property(id, property, value)
            .map_err(|error| core_error(&error))
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
        }
    }

    fn key(&mut self, id: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("key", &["property", "time", "value", "ease"], 3, args)?;
        let property_name = bound.string_or(0, "")?;
        let property = Property::from_name(&property_name).ok_or_else(|| {
            HostError::value_error(format!(
                "key(): unknown property {property_name:?}; animatable properties are {}",
                quoted_names(Property::ALL.iter().map(|(_, n)| *n))
            ))
        })?;
        let time = bound.number(1)?;
        if !time.is_finite() {
            return Err(HostError::value_error(
                "key(): 'time' must be a finite number of seconds",
            ));
        }
        let value = property_value(property, bound.get(2).unwrap_or(&Value::None))?;
        let ease_name = bound.string_or(3, "linear")?;
        let ease = Ease::from_name(&ease_name).ok_or_else(|| {
            HostError::value_error(format!(
                "key(): unknown ease {ease_name:?}; available eases are {}",
                quoted_names(Ease::ALL.iter().map(|(_, n)| *n))
            ))
        })?;
        let document = self.document_mut()?;
        let frame = document.composition().frame_rate().seconds_to_frame(time);
        document
            .add_keyframe(id, property, frame, value, ease)
            .map_err(|error| core_error(&error))?;
        Ok(Value::None)
    }

    fn add_children(&mut self, id: NodeId, args: &CallArgs) -> HostResult<Value> {
        if !args.keywords.is_empty() {
            return Err(HostError::type_error("add() takes no keyword arguments"));
        }
        let document = self.document_mut()?;
        for child in &args.positional {
            let Value::Node(child) = child else {
                return Err(HostError::type_error(format!(
                    "add() expects nodes, got {}",
                    child.type_name()
                )));
            };
            document
                .reparent(*child, id)
                .map_err(|error| core_error(&error))?;
        }
        Ok(Value::None)
    }
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
            "nodes have no attribute '{name}'; available are pos, scale, rotation, anchor, opacity, blend, key, add"
        ),
    )
}

fn quoted_names<'a>(names: impl Iterator<Item = &'a str>) -> String {
    names
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
