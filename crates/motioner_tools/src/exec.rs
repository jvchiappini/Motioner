//! Executors for the catalogue. One match per tool, typed errors, no panics.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)] // tool indices, frame numbers and image sizes; all bounded by the document

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use motioner_analysis::{
    analyze_audio, analyze_image, analyze_video, check_design, code_for_node, contrast_ratio,
    describe_frame, frame_elements, hit_test, layout_report, legibility_report,
    motion_report_range, nodes_for_code, sample_property, BackgroundSample, Box2, CheckOptions,
    ElementKind, LineExtent, MediaOptions, RgbaImage, Rules, TextMeasure,
};
use motioner_core::{
    Color, ContentField, Document, Ease, NodeContent, NodeId, Property, RigInstanceManifest,
    RigValue, TextAlign, TextMetrics, TextRequest, Vec2,
};
use motioner_dsl::{RigEdit, RigSide};
use motioner_eval::{sample, sample_vec2};
use motioner_render::tools::{
    self, CompareMode, ImageFrame, ImageRect, Overlay, OverlayBox, OverlayCaption, OverlaySpec,
    TextlessFrame,
};
use motioner_render::{FrameRenderer, ImageSet};
use serde_json::{json, Value};

use crate::error::{host_error, ToolError};
use crate::host::Host;

/// State that a call may leave for the next one (set-of-marks numbers).
#[derive(Debug, Default)]
pub struct Runtime {
    /// `#n` from the last marks overlay.
    pub marks: HashMap<usize, NodeId>,
}

/// What a tool produced.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// The JSON result.
    pub result: Value,
    /// Images written by the call.
    pub images: Vec<PathBuf>,
}

impl ToolOutput {
    fn json(result: Value) -> Self {
        Self {
            result,
            images: Vec::new(),
        }
    }
}

struct RenderTextMeasure<'a>(&'a dyn TextMetrics);

impl TextMeasure for RenderTextMeasure<'_> {
    fn line(&self, text: &str, size: f64) -> Option<LineExtent> {
        let request = TextRequest {
            text,
            size,
            font: "Roboto",
            weight: 400,
            tracking: 0.0,
            line_height: 1.0,
            align: TextAlign::Left,
        };
        let metrics = self.0.measure(&request)?;
        Some(LineExtent {
            advance: metrics
                .line_advances
                .first()
                .copied()
                .unwrap_or(metrics.width),
            ascender: -metrics.ink.top,
            descender: metrics.ink.bottom,
        })
    }
}

fn arg_object(args: &Value) -> Result<&serde_json::Map<String, Value>, ToolError> {
    args.as_object()
        .ok_or_else(|| ToolError::InvalidArgs("arguments must be a JSON object".to_owned()))
}

fn arg_i64(args: &Value, name: &str) -> Result<Option<i64>, ToolError> {
    match arg_object(args)?.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| ToolError::InvalidArgs(format!("`{name}` must be an integer"))),
    }
}

fn required_i64(args: &Value, name: &str) -> Result<i64, ToolError> {
    arg_i64(args, name)?.ok_or_else(|| ToolError::InvalidArgs(format!("`{name}` is required")))
}

fn arg_f64(args: &Value, name: &str) -> Result<Option<f64>, ToolError> {
    match arg_object(args)?.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| ToolError::InvalidArgs(format!("`{name}` must be a number"))),
    }
}

fn arg_str<'a>(args: &'a Value, name: &str) -> Result<Option<&'a str>, ToolError> {
    match arg_object(args)?.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| ToolError::InvalidArgs(format!("`{name}` must be a string"))),
    }
}

fn required_str<'a>(args: &'a Value, name: &str) -> Result<&'a str, ToolError> {
    arg_str(args, name)?.ok_or_else(|| ToolError::InvalidArgs(format!("`{name}` is required")))
}

fn arg_bool(args: &Value, name: &str) -> Result<bool, ToolError> {
    match arg_object(args)?.get(name) {
        None | Some(Value::Null) => Ok(false),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| ToolError::InvalidArgs(format!("`{name}` must be a boolean"))),
    }
}

fn arg_ids(args: &Value, name: &str) -> Result<Option<Vec<String>>, ToolError> {
    match arg_object(args)?.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| ToolError::InvalidArgs(format!("`{name}` must be strings")))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(ToolError::InvalidArgs(format!("`{name}` must be an array"))),
    }
}

fn rig_of<'a>(document: &'a Document, name: &str) -> Result<&'a RigInstanceManifest, ToolError> {
    document
        .rig_manifest()
        .rigs
        .iter()
        .find(|rig| rig.name == name || rig.id == name)
        .ok_or_else(|| ToolError::InvalidArgs(format!("no rig named {name:?}")))
}

fn side_of(args: &Value) -> Result<RigSide, ToolError> {
    match arg_str(args, "side")?.unwrap_or("both") {
        "both" => Ok(RigSide::Both),
        "left" => Ok(RigSide::Left),
        "right" => Ok(RigSide::Right),
        other => Err(ToolError::InvalidArgs(format!(
            "unknown side {other:?}; use left, right or both"
        ))),
    }
}

fn ease_of(args: &Value) -> Result<Ease, ToolError> {
    match arg_str(args, "ease")? {
        None => Ok(Ease::Linear),
        Some(name) => Ease::from_name(name)
            .ok_or_else(|| ToolError::InvalidArgs(format!("unknown ease {name:?}"))),
    }
}

fn value_of(
    instance: &RigInstanceManifest,
    property: &str,
    value: &Value,
) -> Result<RigValue, ToolError> {
    let definition = instance
        .properties
        .iter()
        .find(|item| item.name == property)
        .ok_or_else(|| ToolError::InvalidArgs(format!("no property named {property:?}")))?;
    match definition.kind {
        motioner_core::RigValueKind::Float | motioner_core::RigValueKind::Int => value
            .as_f64()
            .map(RigValue::Number)
            .ok_or_else(|| ToolError::InvalidArgs(format!("`{property}` needs a number"))),
        motioner_core::RigValueKind::Bool => value
            .as_bool()
            .map(RigValue::Bool)
            .ok_or_else(|| ToolError::InvalidArgs(format!("`{property}` needs a boolean"))),
        motioner_core::RigValueKind::Enum => value
            .as_str()
            .map(|text| RigValue::Text(text.to_owned()))
            .ok_or_else(|| ToolError::InvalidArgs(format!("`{property}` needs a variant name"))),
        motioner_core::RigValueKind::Color => value
            .as_str()
            .and_then(|text| Color::parse_hex(text).ok())
            .map(RigValue::Color)
            .ok_or_else(|| ToolError::InvalidArgs(format!("`{property}` needs \"#rrggbb\""))),
        motioner_core::RigValueKind::Vec2 => {
            let items = value
                .as_array()
                .ok_or_else(|| ToolError::InvalidArgs(format!("`{property}` needs [x, y]")))?;
            let x = items.first().and_then(Value::as_f64);
            let y = items.get(1).and_then(Value::as_f64);
            match (x, y) {
                (Some(x), Some(y)) => Ok(RigValue::Vec2(Vec2::new(x, y))),
                _ => Err(ToolError::InvalidArgs(format!("`{property}` needs [x, y]"))),
            }
        }
    }
}

fn rig_value_json(value: &RigValue) -> Value {
    match value {
        RigValue::Number(number) => json!(number),
        RigValue::Bool(flag) => json!(flag),
        RigValue::Text(text) => json!(text),
        RigValue::Vec2(pair) => json!([pair.x, pair.y]),
        RigValue::Color(color) => json!(color.to_hex()),
    }
}

fn rig_manifest_json(instance: &RigInstanceManifest) -> Value {
    let properties: Vec<Value> = instance
        .properties
        .iter()
        .map(|property| {
            json!({
                "name": property.name,
                "kind": property.kind.name(),
                "angle": property.angle,
                "default": rig_value_json(&property.default),
                "min": property.min,
                "max": property.max,
                "step": property.step,
                "values": property.values,
                "group": property.group,
                "mirror": property.mirror.as_ref().map(|mirror| json!({
                    "group": mirror.group,
                    "side": mirror.side.name(),
                    "flip": mirror.flip,
                })),
                "keys": property.keys.iter().map(|key| json!({
                    "frame": key.frame,
                    "value": rig_value_json(&key.value),
                    "ease": key.ease,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "id": instance.id,
        "name": instance.name,
        "parts": instance.parts.iter().map(|part| json!({"id": part.id, "node": part.node})).collect::<Vec<_>>(),
        "properties": properties,
        "poses": instance.poses.iter().map(|pose| json!({
            "name": pose.name,
            "reset": pose.reset,
            "values": pose.values.iter().map(|value| json!({
                "property": value.property,
                "value": rig_value_json(&value.value),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "pose_tracks": instance.pose_tracks.iter().map(|track| json!({
            "pose": track.pose, "start": track.start, "duration": track.duration, "ease": track.ease,
        })).collect::<Vec<_>>(),
        "clips": instance.clips.iter().map(|clip| json!({
            "name": clip.name,
            "length": clip.length,
            "keys": clip.keys.iter().map(|key| json!({
                "property": key.property, "frame": key.frame,
                "value": rig_value_json(&key.value), "ease": key.ease,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "plays": instance.plays.iter().map(|play| json!({
            "clip": play.clip, "start": play.start, "loops": play.loops, "speed": play.speed,
        })).collect::<Vec<_>>(),
        "layers": instance.layers.iter().map(|layer| json!({
            "kind": layer.kind, "start": layer.start, "period": layer.period,
            "jitter": layer.jitter, "duration": layer.duration, "seed": layer.seed,
        })).collect::<Vec<_>>(),
        "aims": instance.aims.iter().map(|aim| json!({
            "part": aim.part, "property": aim.property, "radius": aim.radius,
        })).collect::<Vec<_>>(),
        "machine": instance.machine.as_ref().map(|machine| json!({
            "default": machine.default,
            "inputs": machine.inputs.iter().map(|input| json!({
                "name": input.name, "kind": input.kind.name(), "values": input.values,
            })).collect::<Vec<_>>(),
            "states": machine.states.iter().map(|state| json!({
                "name": state.name, "pose": state.pose, "clip": state.clip,
                "speed": state.speed, "looping": state.looping,
            })).collect::<Vec<_>>(),
            "transitions": machine.transitions.iter().map(|transition| json!({
                "from": transition.from, "to": transition.to, "input": transition.input,
                "compare": transition.compare, "value": rig_value_json(&transition.value),
                "duration": transition.duration,
            })).collect::<Vec<_>>(),
            "events": machine.events.iter().map(|event| json!({
                "from": event.from, "to": event.to,
            })).collect::<Vec<_>>(),
        })),
    })
}

fn resolve_id(document: &Document, runtime: &Runtime, text: &str) -> Result<NodeId, ToolError> {
    if let Some(number) = text.strip_prefix('#') {
        let number: usize = number
            .parse()
            .map_err(|_| ToolError::InvalidArgs(format!("bad mark number {text:?}")))?;
        return runtime
            .marks
            .get(&number)
            .copied()
            .ok_or_else(|| ToolError::InvalidArgs(format!("{text} is not from the last overlay")));
    }
    document
        .find(text)
        .ok_or_else(|| ToolError::InvalidArgs(format!("no node named {text:?}")))
}

fn box_json(bounds: Option<Box2>) -> Value {
    match bounds {
        Some(bounds) => json!([bounds.min_x, bounds.min_y, bounds.width(), bounds.height()]),
        None => Value::Null,
    }
}

fn rect_json(rect: Box2) -> Value {
    json!({
        "x": rect.min_x, "y": rect.min_y,
        "width": rect.width(), "height": rect.height(),
    })
}

fn image_rect(rect: Box2) -> ImageRect {
    ImageRect::new(rect.min_x, rect.min_y, rect.width(), rect.height())
}

fn overlay_name(name: &str) -> Result<Overlay, ToolError> {
    match name {
        "none" => Ok(Overlay::None),
        "marks" => Ok(Overlay::Marks),
        "safe" => Ok(Overlay::Safe),
        "grid" => Ok(Overlay::Grid),
        "bounds" => Ok(Overlay::Bounds),
        other => Err(ToolError::InvalidArgs(format!(
            "unknown overlay {other:?}; use none, marks, safe, grid or bounds"
        ))),
    }
}

fn write_image(dir: &Path, name: &str, image: &ImageFrame) -> Result<PathBuf, ToolError> {
    std::fs::create_dir_all(dir).map_err(|error| host_error("tool", error))?;
    let path = dir.join(name);
    motioner_io::write_png(&path, image.width, image.height, &image.rgba8)
        .map_err(|error| host_error("tool", error))?;
    Ok(path)
}

fn make_renderer(host: &mut dyn Host, frame: i64) -> Result<FrameRenderer, ToolError> {
    let width = host.document().composition().width();
    let height = host.document().composition().height();
    let mut renderer = FrameRenderer::with_fonts(width, height, host.fonts().clone())
        .map_err(|error| ToolError::Host(format!("cannot create the renderer: {error}")))?;
    renderer.set_images(host.images().clone());
    for image in host.video_images(frame)? {
        renderer.set_image(image);
    }
    Ok(renderer)
}

fn render_frame(host: &mut dyn Host, frame: i64) -> Result<ImageFrame, ToolError> {
    let mut renderer = make_renderer(host, frame)?;
    tools::render_full(&mut renderer, host.document(), frame)
        .map_err(|error| ToolError::Host(format!("render failed: {error}")))
}

fn marks_overlay(
    host: &dyn Host,
    frame: i64,
    ids: Option<&[String]>,
    runtime: &mut Runtime,
) -> OverlaySpec {
    let measure = RenderTextMeasure(host.text_metrics());
    let plan = tools::frame_plan(host.document(), frame);
    let scene = frame_elements(host.document(), &plan, frame, &measure);
    let mut wanted: Vec<(usize, NodeId, String, Option<String>)> = scene
        .elements
        .iter()
        .filter(|element| element.kind != ElementKind::Group)
        .filter(|element| match ids {
            None => true,
            Some(ids) => ids.iter().any(|id| {
                host.document()
                    .find(id)
                    .is_some_and(|resolved| resolved == element.id)
            }),
        })
        .map(|element| {
            (
                element.z,
                element.id,
                element.identity.clone(),
                element.name.clone(),
            )
        })
        .collect();
    wanted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.raw().cmp(&b.1.raw())));
    runtime.marks.clear();
    let colors = [
        Color::rgb(255, 90, 110),
        Color::rgb(90, 200, 255),
        Color::rgb(255, 200, 80),
        Color::rgb(120, 230, 150),
        Color::rgb(200, 140, 255),
        Color::rgb(255, 150, 60),
    ];
    let mut boxes = Vec::new();
    let mut legend = Vec::new();
    for (index, (z, id, identity, name)) in wanted.iter().enumerate() {
        let number = index + 1;
        runtime.marks.insert(number, *id);
        let color = colors[number % colors.len()];
        let bounds = scene
            .elements
            .iter()
            .find(|element| element.id == *id)
            .and_then(|element| element.bounds);
        let line = match name {
            Some(name) if name != identity => format!("#{number} → {identity}, {name}"),
            _ => format!("#{number} → {identity}"),
        };
        if let Some(bounds) = bounds {
            boxes.push(OverlayBox {
                rect: image_rect(bounds),
                number,
                label: line.clone(),
                color,
            });
        }
        legend.push(line);
        let _ = z;
    }
    OverlaySpec {
        boxes,
        legend,
        captions: Vec::new(),
        safe: false,
        grid: false,
    }
}

fn overlay_for(
    host: &dyn Host,
    frame: i64,
    overlay: Overlay,
    ids: Option<&[String]>,
    runtime: &mut Runtime,
) -> OverlaySpec {
    let mut spec = match overlay {
        Overlay::None => OverlaySpec::default(),
        Overlay::Marks | Overlay::Bounds => marks_overlay(host, frame, ids, runtime),
        Overlay::Safe => OverlaySpec {
            safe: true,
            ..OverlaySpec::default()
        },
        Overlay::Grid => OverlaySpec {
            safe: true,
            grid: true,
            ..OverlaySpec::default()
        },
    };
    if overlay == Overlay::Bounds {
        spec.legend.clear();
    }
    spec
}

fn outline_json(document: &Document, depth: Option<usize>, filter: Option<&str>) -> Value {
    let filter = filter.map(str::to_lowercase);
    let duration = document.composition().duration();
    let mut nodes = Vec::new();
    let mut stack: Vec<(NodeId, usize)> = document
        .roots()
        .iter()
        .rev()
        .map(|id| (*id, 0_usize))
        .collect();
    while let Some((id, level)) = stack.pop() {
        let Some(node) = document.node(id) else {
            continue;
        };
        let name = node
            .name()
            .map_or_else(|| node.identity().to_string(), str::to_owned);
        let matches = filter
            .as_ref()
            .is_none_or(|filter| name.to_lowercase().contains(filter));
        if matches && depth.is_none_or(|max| level <= max) {
            let mut start = 0_i64;
            let mut end = duration;
            let transform = node.transform();
            for axis in [motioner_core::Axis::X, motioner_core::Axis::Y] {
                for channel in [
                    transform.position.axis(axis),
                    transform.scale.axis(axis),
                    transform.anchor.axis(axis),
                ] {
                    if let (Some(first), Some(last)) = (
                        channel.keys().first().map(|key| key.frame),
                        channel.keys().last().map(|key| key.frame),
                    ) {
                        start = start.min(first);
                        end = end.max(last);
                    }
                }
            }
            for channel in [transform.rotation.keys(), transform.opacity.keys()] {
                if let (Some(first), Some(last)) = (
                    channel.first().map(|key| key.frame),
                    channel.last().map(|key| key.frame),
                ) {
                    start = start.min(first);
                    end = end.max(last);
                }
            }
            nodes.push(json!({
                "id": id.to_string(),
                "identity": node.identity().to_string(),
                "name": name,
                "kind": node.content().kind_name(),
                "depth": level,
                "parent": node.parent().map(|parent| parent.to_string()),
                "visible": node.is_visible(),
                "start": start,
                "end": end,
            }));
        }
        if depth.is_none_or(|max| level < max) {
            for child in node.children().iter().rev() {
                stack.push((*child, level + 1));
            }
        }
    }
    json!({ "nodes": nodes })
}

fn property_json(document: &Document, id: NodeId, frame: i64) -> Value {
    let Some(node) = document.node(id) else {
        return Value::Null;
    };
    let transform = node.transform();
    let channels = [
        ("pos.x", transform.position.axis(motioner_core::Axis::X)),
        ("pos.y", transform.position.axis(motioner_core::Axis::Y)),
        ("scale.x", transform.scale.axis(motioner_core::Axis::X)),
        ("scale.y", transform.scale.axis(motioner_core::Axis::Y)),
        ("rotation", &transform.rotation),
        ("opacity", &transform.opacity),
    ];
    let mut list = Vec::new();
    for (name, channel) in channels {
        let value = match name {
            "pos.x" => sample_vec2(&transform.position, frame).x,
            "pos.y" => sample_vec2(&transform.position, frame).y,
            "scale.x" => sample_vec2(&transform.scale, frame).x,
            "scale.y" => sample_vec2(&transform.scale, frame).y,
            "rotation" => sample(&transform.rotation, frame),
            _ => sample(&transform.opacity, frame),
        };
        list.push(json!({
            "name": name,
            "value": value,
            "keys": channel.keys().iter().map(|key| json!({
                "frame": key.frame, "value": key.value, "ease": key.ease.name(),
            })).collect::<Vec<_>>(),
        }));
    }
    let mut content = Vec::new();
    for (field, name) in ContentField::ALL {
        if let Some(value) = node.content().static_value(field) {
            content.push(json!({"name": name, "value": format!("{value:?}")}));
        }
    }
    json!({
        "id": id.to_string(),
        "identity": node.identity().to_string(),
        "name": node.name(),
        "kind": node.content().kind_name(),
        "visible": node.is_visible(),
        "channels": list,
        "content": content,
    })
}

fn describe_json(description: &motioner_analysis::FrameDescription) -> Value {
    json!({
        "frame": description.frame,
        "width": description.width,
        "height": description.height,
        "background": description.background.to_hex(),
        "elements": description.elements.iter().map(|element| json!({
            "id": element.id.to_string(),
            "identity": element.identity,
            "name": element.name,
            "kind": element.kind.name(),
            "box": box_json(element.bounds),
            "opacity": element.opacity,
            "z": element.z,
            "color": element.color.map(motioner_core::Color::to_hex),
            "text": element.text.as_ref().map(|text| json!({
                "text": text.text, "size": text.size, "weight": text.weight,
                "words": text.words, "align": format!("{:?}", text.align).to_lowercase(),
            })),
        })).collect::<Vec<_>>(),
    })
}

fn check_json(host: &dyn Host, options: &CheckOptions) -> Value {
    let width = host.document().composition().width();
    let height = host.document().composition().height();
    // Contrast needs pixels; a headless host without a GPU still gets every
    // other rule instead of failing the call.
    let sampler = FrameRenderer::with_fonts(width, height, host.fonts().clone())
        .ok()
        .map(|mut renderer| {
            renderer.set_images(host.images().clone());
            RenderSampler {
                renderer: std::cell::RefCell::new(renderer),
                document: host.document(),
                cache: std::cell::RefCell::new(HashMap::new()),
            }
        });
    let measure = RenderTextMeasure(host.text_metrics());
    let background = sampler
        .as_ref()
        .map(|sampler| sampler as &dyn BackgroundSample);
    let report = check_design(host.document(), options, &measure, background);
    json!({
        "errors": report.errors,
        "warnings": report.warnings,
        "infos": report.infos,
        "frames_checked": report.frames_checked,
        "start": report.start,
        "end": report.end,
        "issues": report.issues.iter().map(|issue| json!({
            "rule": issue.rule,
            "severity": issue.severity.name(),
            "nodes": issue.nodes.iter().map(NodeId::to_string).collect::<Vec<_>>(),
            "frames": issue.frames,
            "occurrences": issue.occurrences,
            "measured": issue.measured,
            "threshold": issue.threshold,
            "fix": issue.fix,
        })).collect::<Vec<_>>(),
    })
}

/// A background sampler that renders a textless frame on demand and caches it.
struct RenderSampler<'a> {
    renderer: std::cell::RefCell<FrameRenderer>,
    document: &'a Document,
    cache: std::cell::RefCell<HashMap<i64, TextlessFrame>>,
}

impl BackgroundSample for RenderSampler<'_> {
    fn behind(&self, frame: i64, rect: Box2) -> Option<Color> {
        let mut cache = self.cache.borrow_mut();
        if let std::collections::hash_map::Entry::Vacant(entry) = cache.entry(frame) {
            let mut renderer = self.renderer.borrow_mut();
            let textless = TextlessFrame::render(&mut renderer, self.document, frame).ok()?;
            entry.insert(textless);
        }
        cache.get(&frame)?.behind(image_rect(rect))
    }
}

fn render_still(
    host: &mut dyn Host,
    args: &Value,
    runtime: &mut Runtime,
    out_dir: &Path,
) -> Result<ToolOutput, ToolError> {
    let document_frame = host.document().composition().frame_rate();
    let frame = match arg_i64(args, "frame")? {
        Some(frame) => frame,
        None => match arg_f64(args, "time")? {
            Some(seconds) => document_frame
                .seconds_to_frame(seconds)
                .map_err(|error| ToolError::InvalidArgs(error.to_string()))?,
            None => 0,
        },
    };
    let size = arg_i64(args, "size")?.unwrap_or(768).clamp(64, 1568) as u32;
    let overlay = overlay_name(arg_str(args, "overlay")?.unwrap_or("none"))?;
    let ids = arg_ids(args, "ids")?;
    let specs = if overlay == Overlay::None {
        OverlaySpec::default()
    } else {
        overlay_for(host, frame, overlay, ids.as_deref(), runtime)
    };
    let full = render_frame(host, frame)?;
    let scale = f64::from(size) / f64::from(full.width.max(full.height).max(1));
    let mut image = full.resized_to_long_edge(size);
    if !specs.is_empty() {
        let spec = specs.scaled(scale);
        let overlay_image = tools::render_overlay(image.width, image.height, &spec, (0.05, 0.10))
            .map_err(|error| ToolError::Host(error.to_string()))?;
        image.composite(&overlay_image, 0, 0);
    }
    let path = write_image(out_dir, &format!("still_{frame}.png"), &image)?;
    Ok(ToolOutput {
        result: json!({
            "frame": frame,
            "size": [image.width, image.height],
            "overlay": overlay.name(),
            "legend": specs.legend,
            "images": [path.display().to_string()],
        }),
        images: vec![path],
    })
}

#[allow(clippy::too_many_lines)] // strip and sheet share the sampling loop
fn render_sequence(
    host: &mut dyn Host,
    args: &Value,
    kind: &str,
    out_dir: &Path,
) -> Result<ToolOutput, ToolError> {
    let composition = host.document().composition();
    let duration = composition.duration().max(1);
    let (start, end, count, columns) = if kind == "filmstrip" {
        let start = required_i64(args, "start")?.clamp(0, duration - 1);
        let end = required_i64(args, "end")?.clamp(start, duration - 1);
        let count = arg_i64(args, "count")?.unwrap_or(6).clamp(1, 8) as usize;
        (start, end, count, count as u32)
    } else {
        let count = arg_i64(args, "count")?.unwrap_or(9).clamp(1, 12) as usize;
        (0, duration - 1, count, 3)
    };
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let frame = if count == 1 {
            start
        } else {
            start
                + (end - start) * i64::try_from(index).unwrap_or(0)
                    / (i64::try_from(count).unwrap_or(1) - 1)
        };
        frames.push((frame, render_frame(host, frame)?));
    }
    let images: Vec<ImageFrame> = frames.iter().map(|(_, image)| image.clone()).collect();
    let mut composed = if kind == "filmstrip" {
        tools::compose_strip(&images, 224, 8)
    } else {
        tools::compose_sheet(&images, columns, 320, 8)
    };
    // Frame labels under every cell.
    let cell_width = if kind == "filmstrip" { 224.0 } else { 320.0 };
    let columns = if kind == "filmstrip" {
        images.len() as u32
    } else {
        columns
    };
    let rows = (images.len() as u32).div_ceil(columns.max(1));
    let cell_height = (composed.height / rows.max(1)) as f64;
    let mut captions = Vec::new();
    for (index, (frame, _)) in frames.iter().enumerate() {
        let column = (index as u32) % columns.max(1);
        let row = (index as u32) / columns.max(1);
        captions.push(OverlayCaption {
            position: (
                column as f64 * (cell_width + 8.0) + 6.0,
                row as f64 * (cell_height + 8.0) + 18.0,
            ),
            text: format!("f{frame}"),
            size: 13.0,
            color: Color::rgb(255, 255, 255),
        });
    }
    let spec = OverlaySpec {
        captions,
        ..OverlaySpec::default()
    };
    let overlay = tools::render_overlay(composed.width, composed.height, &spec, (0.05, 0.10))
        .map_err(|error| ToolError::Host(error.to_string()))?;
    composed.composite(&overlay, 0, 0);
    let name = format!(
        "{kind}_{}_{}.png",
        frames.first().map_or(0, |(f, _)| *f),
        frames.last().map_or(0, |(f, _)| *f)
    );
    let path = write_image(out_dir, &name, &composed)?;
    Ok(ToolOutput {
        result: json!({
            "frames": frames.iter().map(|(frame, _)| *frame).collect::<Vec<_>>(),
            "images": [path.display().to_string()],
        }),
        images: vec![path],
    })
}

fn render_zoom(host: &mut dyn Host, args: &Value, out_dir: &Path) -> Result<ToolOutput, ToolError> {
    let frame = required_i64(args, "frame")?;
    let region = arg_object(args)?
        .get("region")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::InvalidArgs("`region` must be [x, y, w, h]".to_owned()))?;
    if region.len() != 4 {
        return Err(ToolError::InvalidArgs(
            "`region` must have four numbers".to_owned(),
        ));
    }
    let numbers: Vec<f64> = region
        .iter()
        .map(|value| {
            value
                .as_f64()
                .ok_or_else(|| ToolError::InvalidArgs("region values must be numbers".to_owned()))
        })
        .collect::<Result<_, _>>()?;
    let size = arg_i64(args, "size")?.unwrap_or(768).clamp(64, 1568) as u32;
    let full = render_frame(host, frame)?;
    let cropped = full.cropped(ImageRect::new(
        numbers[0], numbers[1], numbers[2], numbers[3],
    ));
    let image = cropped.resized_to_long_edge(size);
    let path = write_image(out_dir, &format!("zoom_{frame}.png"), &image)?;
    Ok(ToolOutput {
        result: json!({
            "frame": frame,
            "region": numbers,
            "size": [image.width, image.height],
            "images": [path.display().to_string()],
        }),
        images: vec![path],
    })
}

fn compare_frames(
    host: &mut dyn Host,
    args: &Value,
    out_dir: &Path,
) -> Result<ToolOutput, ToolError> {
    let a = required_i64(args, "a")?;
    let b = required_i64(args, "b")?;
    let mode = match arg_str(args, "mode")?.unwrap_or("side") {
        "side" => CompareMode::Side,
        "diff" => CompareMode::Diff,
        other => {
            return Err(ToolError::InvalidArgs(format!(
                "unknown compare mode {other:?}; use side or diff"
            )))
        }
    };
    let left = render_frame(host, a)?;
    let right = render_frame(host, b)?;
    let comparison = tools::compare(&left, &right, mode);
    let path = write_image(out_dir, &format!("compare_{a}_{b}.png"), &comparison.image)?;
    Ok(ToolOutput {
        result: json!({
            "a": a,
            "b": b,
            "mode": mode.name(),
            "mean_change": comparison.mean_change,
            "changed_regions": comparison.changed.iter().map(|rect| json!({
                "x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height,
            })).collect::<Vec<_>>(),
            "images": [path.display().to_string()],
        }),
        images: vec![path],
    })
}

/// Runs one tool by name.
///
/// # Errors
///
/// Returns [`ToolError`] for unknown tools, bad arguments, harness-only tools
/// and host failures.
#[allow(clippy::too_many_lines)] // one arm per tool, intentionally flat
pub fn execute(
    name: &str,
    args: &Value,
    runtime: &mut Runtime,
    host: &mut dyn Host,
    out_dir: &Path,
) -> Result<ToolOutput, ToolError> {
    let measure = RenderTextMeasure(host.text_metrics());
    match name {
        "project_summary" => Ok(ToolOutput::json(host.summary())),
        "rig_list" => {
            let rigs: Vec<Value> = host
                .document()
                .rig_manifest()
                .rigs
                .iter()
                .map(|rig| {
                    json!({
                        "id": rig.id,
                        "name": rig.name,
                        "parts": rig.parts.len(),
                        "properties": rig.properties.len(),
                        "poses": rig.poses.len(),
                        "clips": rig.clips.len(),
                        "states": rig.machine.as_ref().map_or(0, |machine| machine.states.len()),
                    })
                })
                .collect();
            Ok(ToolOutput::json(json!({
                "version": host.document().rig_manifest().version,
                "rigs": rigs,
            })))
        }
        "rig_manifest" => {
            let manifest = host.document().rig_manifest();
            let rigs: Vec<&RigInstanceManifest> = match arg_str(args, "instance")? {
                Some(name) => vec![rig_of(host.document(), name)?],
                None => manifest.rigs.iter().collect(),
            };
            Ok(ToolOutput::json(json!({
                "version": manifest.version,
                "rigs": rigs.iter().map(|rig| rig_manifest_json(rig)).collect::<Vec<_>>(),
            })))
        }
        "rig_set" => {
            let rig = required_str(args, "rig")?;
            let property = required_str(args, "property")?;
            let value = arg_object(args)?
                .get("value")
                .ok_or_else(|| ToolError::InvalidArgs("`value` is required".to_owned()))?;
            let instance = rig_of(host.document(), rig)?;
            let edit = RigEdit::Set {
                rig: rig.to_owned(),
                property: property.to_owned(),
                side: side_of(args)?,
                value: value_of(instance, property, value)?,
            };
            Ok(ToolOutput::json(host.rig_edit(&edit)?))
        }
        "rig_key" | "rig_key_input" => {
            let rig = required_str(args, "rig")?;
            let property = required_str(args, "property")?;
            let time = arg_f64(args, "time")?
                .ok_or_else(|| ToolError::InvalidArgs("`time` is required".to_owned()))?;
            let value = arg_object(args)?
                .get("value")
                .ok_or_else(|| ToolError::InvalidArgs("`value` is required".to_owned()))?;
            let instance = rig_of(host.document(), rig)?;
            if name == "rig_key_input"
                && !instance
                    .machine
                    .as_ref()
                    .is_some_and(|machine| machine.inputs.iter().any(|input| input.name == property))
            {
                return Err(ToolError::InvalidArgs(format!(
                    "{property:?} is not a machine input of {rig:?}"
                )));
            }
            let edit = RigEdit::Key {
                rig: rig.to_owned(),
                property: property.to_owned(),
                side: side_of(args)?,
                time,
                value: value_of(instance, property, value)?,
                ease: ease_of(args)?,
            };
            Ok(ToolOutput::json(host.rig_edit(&edit)?))
        }
        "rig_apply_pose" => {
            let rig = required_str(args, "rig")?;
            let pose = required_str(args, "pose")?;
            let at = arg_f64(args, "at")?
                .ok_or_else(|| ToolError::InvalidArgs("`at` is required".to_owned()))?;
            let edit = RigEdit::ApplyPose {
                rig: rig.to_owned(),
                pose: pose.to_owned(),
                at,
                duration: arg_f64(args, "dur")?.unwrap_or(0.3),
                ease: ease_of(args)?,
            };
            Ok(ToolOutput::json(host.rig_edit(&edit)?))
        }
        "rig_play_clip" => {
            let rig = required_str(args, "rig")?;
            let clip = required_str(args, "clip")?;
            let loops = arg_i64(args, "loops")?.unwrap_or(1);
            let edit = RigEdit::Play {
                rig: rig.to_owned(),
                clip: clip.to_owned(),
                at: arg_f64(args, "at")?.unwrap_or(0.0),
                loops: u32::try_from(loops.max(0)).unwrap_or(u32::MAX),
                speed: arg_f64(args, "speed")?.unwrap_or(1.0),
            };
            Ok(ToolOutput::json(host.rig_edit(&edit)?))
        }
        "rig_save_pose" => {
            let rig = required_str(args, "rig")?;
            let pose = required_str(args, "pose")?;
            let instance = rig_of(host.document(), rig)?;
            let values = match arg_object(args)?.get("values") {
                Some(Value::Object(map)) => map
                    .iter()
                    .map(|(property, value)| {
                        Ok((property.clone(), value_of(instance, property, value)?))
                    })
                    .collect::<Result<Vec<_>, ToolError>>()?,
                Some(value) if !value.is_null() => {
                    return Err(ToolError::InvalidArgs(
                        "`values` must be an object of property values".to_owned(),
                    ))
                }
                _ => instance
                    .properties
                    .iter()
                    .map(|property| (property.name.clone(), property.default.clone()))
                    .collect(),
            };
            let edit = RigEdit::SavePose {
                rig: rig.to_owned(),
                pose: pose.to_owned(),
                reset: arg_bool(args, "reset")?,
                values,
            };
            Ok(ToolOutput::json(host.rig_edit(&edit)?))
        }
        "outline" => {
            let depth = arg_i64(args, "depth")?.map(|value| value.max(0) as usize);
            let filter = arg_str(args, "filter")?;
            Ok(ToolOutput::json(outline_json(host.document(), depth, filter)))
        }
        "get_node" => {
            let id = resolve_id(host.document(), runtime, required_str(args, "id")?)?;
            let frame = arg_i64(args, "frame")?.unwrap_or(0);
            let plan = tools::frame_plan(host.document(), frame);
            let scene = frame_elements(host.document(), &plan, frame, &measure);
            let bounds = scene
                .elements
                .iter()
                .find(|element| element.id == id)
                .and_then(|element| element.bounds);
            let mut json = property_json(host.document(), id, frame);
            if let Value::Object(map) = &mut json {
                map.insert("bounds".to_owned(), box_json(bounds));
            }
            Ok(ToolOutput::json(json))
        }
        "read_program" => {
            let start = arg_i64(args, "start_line")?.unwrap_or(1).max(1);
            let end = arg_i64(args, "end_line")?.unwrap_or(i64::MAX);
            let lines: Vec<Value> = host
                .program_source()
                .lines()
                .enumerate()
                .filter(|(index, _)| {
                    let line = i64::try_from(*index).unwrap_or(0) + 1;
                    line >= start && line <= end
                })
                .map(|(index, text)| json!({"line": index + 1, "text": text}))
                .collect();
            Ok(ToolOutput::json(json!({
                "file": host.program_path().display().to_string(),
                "lines": lines,
            })))
        }
        "search_code" => {
            let query = required_str(args, "query")?;
            if arg_bool(args, "regex")? {
                return Err(ToolError::Unsupported(
                    "regex search needs the regex crate; use a substring query".to_owned(),
                ));
            }
            let needle = query.to_lowercase();
            let matches: Vec<Value> = host
                .program_source()
                .lines()
                .enumerate()
                .filter(|(_, text)| text.to_lowercase().contains(&needle))
                .map(|(index, text)| json!({
                    "file": host.program_path().display().to_string(),
                    "line": index + 1,
                    "text": text,
                }))
                .collect();
            Ok(ToolOutput::json(json!({"matches": matches})))
        }
        "selection" => Ok(ToolOutput::json(json!({
            "ids": [],
            "frame": 0,
        }))),
        "std_reference" => {
            let name = required_str(args, "name")?;
            let source = motioner_dsl::prelude::source();
            let mut signature = String::new();
            let mut parameters = Vec::new();
            for line in source.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with(&format!("def {name}(")) {
                    signature.clear();
                    signature.push_str(trimmed.trim_end_matches(':'));
                    let inside = signature
                        .split_once('(')
                        .and_then(|(_, rest)| rest.split_once(')'))
                        .map_or("", |(params, _)| params);
                    for part in inside.split(',').map(str::trim).filter(|part| !part.is_empty()) {
                        let mut pieces = part.splitn(2, '=');
                        parameters.push(json!({
                            "name": pieces.next().unwrap_or("").trim(),
                            "default": pieces.next().map(str::trim),
                        }));
                    }
                }
            }
            if signature.is_empty() && motioner_dsl::theme::by_name(name).is_none() {
                return Err(ToolError::InvalidArgs(format!(
                    "no std name {name:?}; try `std_reference(\"rise\")`"
                )));
            }
            Ok(ToolOutput::json(json!({
                "name": name,
                "signature": signature,
                "parameters": parameters,
                "theme": motioner_dsl::theme::by_name(name).is_some(),
            })))
        }
        "find_examples" => {
            let query = required_str(args, "query")?.to_lowercase();
            let root = host.program_path().parent().unwrap_or(Path::new("."));
            let mut examples = Vec::new();
            for candidate in [root.join("examples"), root.parent().unwrap_or(root).join("examples")] {
                if !candidate.is_dir() {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(&candidate) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|ext| ext.to_str()) != Some("motion") {
                            continue;
                        }
                        let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
                        let brief_path = path.with_file_name(format!("{stem}.prompt.md"));
                        let brief = std::fs::read_to_string(&brief_path).unwrap_or_default();
                        if stem.to_lowercase().contains(&query) || brief.to_lowercase().contains(&query) {
                            examples.push(json!({
                                "path": path.display().to_string(),
                                "brief": brief.lines().find(|line| !line.trim().is_empty()).unwrap_or(""),
                            }));
                        }
                    }
                }
            }
            let limit = arg_i64(args, "limit")?.unwrap_or(5).clamp(1, 50) as usize;
            examples.truncate(limit);
            Ok(ToolOutput::json(json!({"examples": examples})))
        }
        "run_commands" => {
            let commands = arg_ids(args, "commands")?
                .ok_or_else(|| ToolError::InvalidArgs("`commands` is required".to_owned()))?;
            Ok(ToolOutput::json(host.run_commands(&commands)?))
        }
        "apply_code_edit" => {
            let file = required_str(args, "file")?;
            let edits = arg_object(args)?
                .get("edits")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::InvalidArgs("`edits` must be an array".to_owned()))?;
            Ok(ToolOutput::json(host.apply_code_edit(file, edits)?))
        }
        "write_file" => {
            let file = required_str(args, "file")?;
            let contents = required_str(args, "contents")?;
            Ok(ToolOutput::json(host.write_file(file, contents)?))
        }
        "run_program" => Ok(ToolOutput::json(host.run_program()?)),
        "format" => Ok(ToolOutput::json(host.format(arg_str(args, "file")?)?)),
        "lint" => Ok(ToolOutput::json(host.lint(arg_str(args, "file")?)?)),
        "checkpoint" => {
            let name = required_str(args, "name")?;
            let checkpoint = host.checkpoint(name)?;
            Ok(ToolOutput::json(json!({"checkpoint": checkpoint})))
        }
        "restore" => {
            let checkpoint = required_str(args, "checkpoint")?;
            Ok(ToolOutput::json(host.restore(checkpoint)?))
        }
        "undo" => {
            let steps = arg_i64(args, "steps")?.unwrap_or(1).clamp(1, 100) as u32;
            Ok(ToolOutput::json(host.undo(steps)?))
        }
        "redo" => {
            let steps = arg_i64(args, "steps")?.unwrap_or(1).clamp(1, 100) as u32;
            Ok(ToolOutput::json(host.redo(steps)?))
        }
        "render_still" => render_still(host, args, runtime, out_dir),
        "render_filmstrip" => render_sequence(host, args, "filmstrip", out_dir),
        "render_contact_sheet" => render_sequence(host, args, "sheet", out_dir),
        "render_zoom" => render_zoom(host, args, out_dir),
        "compare" => compare_frames(host, args, out_dir),
        "view_asset" => {
            let key = required_str(args, "key")?;
            let Some((name, image)) = host.asset_image(key)? else {
                return Err(ToolError::InvalidArgs(format!("no image asset {key:?}")));
            };
            let image = ImageFrame {
                width: image.width,
                height: image.height,
                rgba8: image.rgba8,
            };
            let size = arg_i64(args, "size")?.unwrap_or(768).clamp(64, 1568) as u32;
            let image = image.resized_to_long_edge(size);
            let path = write_image(out_dir, &format!("asset_{name}.png"), &image)?;
            Ok(ToolOutput {
                result: json!({
                    "key": key,
                    "name": name,
                    "size": [image.width, image.height],
                    "images": [path.display().to_string()],
                }),
                images: vec![path],
            })
        }
        "render_variants" => Err(ToolError::Unsupported(
            "render_variants runs on throwaway project copies and is wired by the prompt 14 harness"
                .to_owned(),
        )),
        "view_editor" => Err(ToolError::RequiresHarness(
            "view_editor needs the open window".to_owned(),
        )),
        "describe_frame" => {
            let frame = required_i64(args, "frame")?;
            let ids: Option<Vec<NodeId>> = arg_ids(args, "ids")?
                .map(|ids| {
                    ids.iter()
                        .map(|id| resolve_id(host.document(), runtime, id))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let plan = tools::frame_plan(host.document(), frame);
            let description =
                describe_frame(host.document(), &plan, frame, ids.as_deref(), &measure);
            Ok(ToolOutput::json(describe_json(&description)))
        }
        "check_design" => {
            let start = arg_i64(args, "start")?;
            let end = arg_i64(args, "end")?;
            let rules = arg_ids(args, "rules")?;
            let options = CheckOptions {
                start,
                end,
                rules: Rules::default(),
            };
            let report = check_json(host, &options);
            let filtered = match rules {
                None => report,
                Some(rules) => {
                    let mut report = report;
                    if let Some(issues) = report.get_mut("issues").and_then(Value::as_array_mut) {
                        issues.retain(|issue| {
                            issue
                                .get("rule")
                                .and_then(Value::as_str)
                                .is_some_and(|rule| rules.iter().any(|wanted| wanted == rule))
                        });
                    }
                    report
                }
            };
            Ok(ToolOutput::json(filtered))
        }
        "motion_report" => {
            let ids: Option<Vec<NodeId>> = arg_ids(args, "ids")?
                .map(|ids| {
                    ids.iter()
                        .map(|id| resolve_id(host.document(), runtime, id))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let duration = host.document().composition().duration();
            let start = arg_i64(args, "start")?.unwrap_or(0);
            let end = arg_i64(args, "end")?.unwrap_or(duration.saturating_sub(1));
            let report = motion_report_range(host.document(), start, end, ids.as_deref(), &measure);
            Ok(ToolOutput::json(json!({
                "elements": report.elements.iter().map(|element| json!({
                    "id": element.id.to_string(),
                    "identity": element.identity,
                    "name": element.name,
                    "first_frame": element.first_frame,
                    "last_frame": element.last_frame,
                    "duration_frames": element.duration_frames,
                    "onset": element.first_frame,
                    "peak_speed": element.peak_speed,
                    "peak_relative": element.peak_relative,
                    "peak_frame": element.peak_frame,
                    "eases": element.eases,
                    "keys": element.keys,
                    "jumps": element.jumps.iter().map(|jump| json!({
                        "property": jump.property, "frame": jump.frame,
                        "incoming": jump.incoming, "outgoing": jump.outgoing,
                        "delta": jump.delta,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "stagger": {
                    "span_frames": report.stagger.span_frames,
                    "distinct": report.stagger.distinct,
                    "simultaneous": report.stagger.simultaneous,
                    "onsets": report.stagger.onsets.iter().map(|(id, frame)| json!({
                        "id": id.to_string(), "frame": frame,
                    })).collect::<Vec<_>>(),
                },
                "dead_time_frames": report.dead_time_frames,
                "dead_time_start": report.dead_time_start,
                "overload_peak": report.overload_peak,
                "overload_frames": report.overload_frames,
            })))
        }
        "layout_report" => {
            let frame = required_i64(args, "frame")?;
            let plan = tools::frame_plan(host.document(), frame);
            let scene = frame_elements(host.document(), &plan, frame, &measure);
            let report = layout_report(&scene, &Rules::default());
            Ok(ToolOutput::json(json!({
                "frame": frame,
                "text_overlaps": report.text_overlaps.iter().map(|overlap| json!({
                    "a": overlap.a.to_string(), "b": overlap.b.to_string(),
                    "fraction": overlap.fraction,
                })).collect::<Vec<_>>(),
                "busy_overlaps": report.busy_overlaps.iter().map(|overlap| json!({
                    "text": overlap.text.to_string(), "busy": overlap.busy.to_string(),
                    "fraction": overlap.fraction,
                })).collect::<Vec<_>>(),
                "safe_violations": report.safe_violations.iter().map(|violation| json!({
                    "id": violation.id.to_string(), "outside_pixels": violation.outside_pixels,
                })).collect::<Vec<_>>(),
                "off_screen": report.off_screen.iter().map(|off| json!({
                    "id": off.id.to_string(), "text": off.text,
                })).collect::<Vec<_>>(),
                "alignment_groups": report.alignment_groups.iter().map(|group| json!({
                    "edge": group.edge, "coordinate": group.coordinate,
                    "nodes": group.nodes.iter().map(NodeId::to_string).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "type_levels": report.type_levels.iter().map(|level| json!({
                    "size": level.size, "count": level.count,
                })).collect::<Vec<_>>(),
            })))
        }
        "legibility_report" => {
            let duration = host.document().composition().duration();
            let start = arg_i64(args, "start")?.unwrap_or(0);
            let end = arg_i64(args, "end")?.unwrap_or(duration.saturating_sub(1));
            let width = host.document().composition().width();
            let height = host.document().composition().height();
            let sampler = FrameRenderer::with_fonts(width, height, host.fonts().clone())
                .ok()
                .map(|mut renderer| {
                    renderer.set_images(host.images().clone());
                    RenderSampler {
                        renderer: std::cell::RefCell::new(renderer),
                        document: host.document(),
                        cache: std::cell::RefCell::new(HashMap::new()),
                    }
                });
            let background = sampler
                .as_ref()
                .map(|sampler| sampler as &dyn BackgroundSample);
            let report = legibility_report(
                host.document(),
                start,
                end,
                &Rules::default(),
                &measure,
                background,
            );
            Ok(ToolOutput::json(json!({
                "fps": report.fps,
                "texts": report.texts.iter().map(|text| json!({
                    "id": text.id.to_string(),
                    "identity": text.identity,
                    "text": text.text,
                    "words": text.words,
                    "visible_seconds": text.visible_seconds,
                    "required_seconds": text.required_seconds,
                    "contrast": text.contrast,
                    "required_contrast": text.required_contrast,
                    "sample_frame": text.sample_frame,
                    "sample_box": text.sample_box.map(rect_json),
                    "ok": text.holds_long_enough() && text.contrast_ok(),
                })).collect::<Vec<_>>(),
            })))
        }
        "hit_test" => {
            let frame = required_i64(args, "frame")?;
            let x = arg_f64(args, "x")?
                .ok_or_else(|| ToolError::InvalidArgs("`x` is required".to_owned()))?;
            let y = arg_f64(args, "y")?
                .ok_or_else(|| ToolError::InvalidArgs("`y` is required".to_owned()))?;
            let plan = tools::frame_plan(host.document(), frame);
            let scene = frame_elements(host.document(), &plan, frame, &measure);
            let hits = hit_test(&scene, Vec2::new(x, y));
            Ok(ToolOutput::json(json!({
                "hits": hits.iter().map(|hit| json!({
                    "id": hit.id.to_string(), "kind": hit.kind.name(), "z": hit.z,
                })).collect::<Vec<_>>(),
            })))
        }
        "code_for_node" => {
            let id = resolve_id(host.document(), runtime, required_str(args, "id")?)?;
            let Some(origins) = host.origins() else {
                return Err(ToolError::Unsupported(
                    "this host did not keep value origins".to_owned(),
                ));
            };
            let location = code_for_node(origins, id)
                .ok_or_else(|| ToolError::InvalidArgs("the node has no recorded origin".to_owned()))?;
            Ok(ToolOutput::json(json!({
                "statement": format!("line {}", location.statement_line),
                "line": location.statement_line,
                "creation_line": location.creation_line,
                "column": location.statement_column,
            })))
        }
        "node_for_code" => {
            let line = required_i64(args, "line")?.max(1) as u32;
            let Some(origins) = host.origins() else {
                return Err(ToolError::Unsupported(
                    "this host did not keep value origins".to_owned(),
                ));
            };
            let ids = nodes_for_code(origins, line);
            Ok(ToolOutput::json(json!({
                "ids": ids.iter().map(NodeId::to_string).collect::<Vec<_>>(),
            })))
        }
        "sample" => {
            let id = resolve_id(host.document(), runtime, required_str(args, "id")?)?;
            let property = required_str(args, "property")?;
            let frames = arg_object(args)?
                .get("frames")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::InvalidArgs("`frames` must be an array".to_owned()))?;
            let mut values = Vec::new();
            for frame in frames {
                let frame = frame
                    .as_i64()
                    .ok_or_else(|| ToolError::InvalidArgs("frames must be integers".to_owned()))?;
                values.push(json!({
                    "frame": frame,
                    "value": sample_property(host.document(), id, property, frame),
                }));
            }
            Ok(ToolOutput::json(json!({"values": values})))
        }
        "analyze_image" => {
            let key = required_str(args, "key")?;
            let Some((name, image)) = host.asset_image(key)? else {
                return Err(ToolError::InvalidArgs(format!("no image asset {key:?}")));
            };
            let analysis = analyze_image(&RgbaImage {
                width: image.width,
                height: image.height,
                rgba8: image.rgba8,
            }, &MediaOptions::default());
            Ok(ToolOutput::json(json!({
                "key": key,
                "name": name,
                "width": analysis.width,
                "height": analysis.height,
                "palette": analysis.palette.iter().map(|color| json!({
                    "color": color.color.to_hex(), "fraction": color.fraction,
                })).collect::<Vec<_>>(),
                "salient": analysis.salient.map(rect_json),
                "empty_regions": analysis.empty_regions.iter().map(|region| json!({
                    "rect": rect_json(region.rect),
                    "luminance": region.luminance,
                    "busyness": region.busyness,
                    "score": region.score,
                })).collect::<Vec<_>>(),
                "luminance_grid": analysis.luminance_grid,
            })))
        }
        "analyze_video" => {
            let key = required_str(args, "key")?;
            let Some(frames) = host.asset_video(key, 96)? else {
                return Err(ToolError::InvalidArgs(format!("no video asset {key:?}")));
            };
            let analysis = analyze_video(&frames, 0.35);
            Ok(ToolOutput::json(json!({
                "key": key,
                "frames": frames.len(),
                "cuts": analysis.cuts,
                "motion": analysis.motion,
                "calm_segments": analysis.calm_segments,
            })))
        }
        "analyze_audio" => {
            let key = required_str(args, "key")?;
            let Some((rate, samples)) = host.asset_audio(key)? else {
                return Err(ToolError::InvalidArgs(format!("no audio asset {key:?}")));
            };
            let analysis = analyze_audio(&samples, rate);
            Ok(ToolOutput::json(json!({
                "key": key,
                "duration": analysis.duration,
                "bpm": analysis.bpm,
                "beats": analysis.beats,
                "downbeats": analysis.downbeats,
                "sections": analysis.sections,
                "loudness": analysis.loudness,
            })))
        }
        "list_fonts" => {
            let mut families: Vec<String> = host
                .fonts()
                .faces()
                .map(|(family, weight)| format!("{family}:{weight}"))
                .collect();
            families.sort();
            families.dedup();
            Ok(ToolOutput::json(json!({"families": families})))
        }
        "list_themes" => {
            let themes: Vec<Value> = motioner_dsl::theme::names()
                .map(|name| {
                    let theme = motioner_dsl::theme::by_name(name);
                    json!({
                        "name": name,
                        "background": theme.and_then(|theme| theme.color("bg")).map(Color::to_hex),
                        "text": theme.and_then(|theme| theme.color("text")).map(Color::to_hex),
                        "accent": theme.and_then(|theme| theme.color("accent")).map(Color::to_hex),
                    })
                })
                .collect();
            Ok(ToolOutput::json(json!({"themes": themes})))
        }
        "search_icons" => {
            let _query = required_str(args, "query")?;
            Ok(ToolOutput::json(json!({"icons": []})))
        }
        "brand_kit" => {
            let action = required_str(args, "action")?;
            let values = arg_object(args)?.get("values");
            Ok(ToolOutput::json(host.brand_kit(action, values)?))
        }
        "render_preview" => {
            let duration = host.document().composition().duration();
            let start = arg_i64(args, "start")?.unwrap_or(0);
            let end = arg_i64(args, "end")?.unwrap_or(duration.saturating_sub(1));
            Ok(ToolOutput::json(host.render_preview(start, end)?))
        }
        "export" => {
            let path = arg_str(args, "path")?;
            Ok(ToolOutput::json(host.export(path)?))
        }
        "ask_user" | "ask_user_visual" | "show_plan" | "request_approval"
        | "remember_preference" => Err(ToolError::RequiresHarness(format!(
            "{name} is executed by the prompt 14 harness"
        ))),
        "import_asset" | "place_asset" => Err(ToolError::RequiresHarness(format!(
            "{name} needs the conversation's attachment list (prompt 14)"
        ))),
        other => Err(ToolError::Unknown(format!("unknown tool {other:?}"))),
    }
}

/// Runs the tools without touching a host: used to validate the catalogue.
///
/// # Errors
///
/// Returns [`ToolError::Unknown`] for unknown names and `InvalidArgs` for a
/// malformed example.
pub fn dry_run(name: &str, args: &Value) -> Result<(), ToolError> {
    if crate::catalogue::spec(name).is_none() {
        return Err(ToolError::Unknown(format!("unknown tool {name:?}")));
    }
    if !args.is_object() {
        return Err(ToolError::InvalidArgs(
            "arguments must be an object".to_owned(),
        ));
    }
    Ok(())
}

/// A read-only host used by the contract tests.
pub struct FixtureHost {
    /// Program path.
    pub program: PathBuf,
    /// Program source.
    pub source: String,
    /// Compiled document.
    pub document: Document,
    /// Value origins.
    pub origins: Option<motioner_dsl::ValueOrigins>,
    /// Images.
    pub images: ImageSet,
    /// Fonts.
    pub fonts: motioner_render::FontSet,
    /// Metrics.
    pub metrics: std::sync::Arc<dyn TextMetrics>,
}

impl Host for FixtureHost {
    fn program_path(&self) -> &Path {
        &self.program
    }
    fn program_source(&self) -> &str {
        &self.source
    }
    fn document(&self) -> &Document {
        &self.document
    }
    fn origins(&self) -> Option<&motioner_dsl::ValueOrigins> {
        self.origins.as_ref()
    }
    fn images(&self) -> &ImageSet {
        &self.images
    }
    fn fonts(&self) -> &motioner_render::FontSet {
        &self.fonts
    }
    fn text_metrics(&self) -> &dyn TextMetrics {
        self.metrics.as_ref()
    }
    fn summary(&self) -> Value {
        let composition = self.document.composition();
        json!({
            "comps": [composition.name()],
            "width": composition.width(),
            "height": composition.height(),
            "fps": format!("{}/{}", composition.frame_rate().numerator(), composition.frame_rate().denominator()),
            "duration": composition.duration(),
            "layers": self.document.len(),
            "std_version": motioner_dsl::std_version(),
        })
    }
}

/// Unused helper kept for the analysis text trait (contract tests use it).
#[must_use]
pub fn text_metrics_measure(
    metrics: &dyn TextMetrics,
    text: &str,
    size: f64,
) -> Option<LineExtent> {
    RenderTextMeasure(metrics).line(text, size)
}

/// The property of a text node's content, for callers that want a raw value.
#[must_use]
pub fn content_number(
    document: &Document,
    id: NodeId,
    field: ContentField,
    frame: i64,
) -> Option<f64> {
    let node = document.node(id)?;
    let channel = node.content().channel(field)?;
    match channel {
        motioner_core::ContentChannel::Number(value) => Some(sample(value, frame)),
        motioner_core::ContentChannel::Color(_)
        | motioner_core::ContentChannel::OptionalColor(_) => None,
    }
}

/// Whether a node draws text (used by the contract tests).
#[must_use]
pub fn is_text_node(document: &Document, id: NodeId) -> bool {
    document
        .node(id)
        .is_some_and(|node| matches!(node.content(), NodeContent::Text(_)))
}

/// The transform property of a node, for the contract tests.
#[must_use]
pub fn transform_property(document: &Document, id: NodeId, property: Property) -> Option<Value> {
    let node = document.node(id)?;
    let transform = node.transform();
    Some(match property {
        Property::Position => json!([
            sample_vec2(&transform.position, 0).x,
            sample_vec2(&transform.position, 0).y
        ]),
        Property::Scale => json!([
            sample_vec2(&transform.scale, 0).x,
            sample_vec2(&transform.scale, 0).y
        ]),
        Property::Anchor => json!([
            sample_vec2(&transform.anchor, 0).x,
            sample_vec2(&transform.anchor, 0).y
        ]),
        Property::Rotation => json!(sample(&transform.rotation, 0)),
        Property::Opacity => json!(sample(&transform.opacity, 0)),
    })
}

/// The WCAG contrast between two colours (exposed for CLI parity).
#[must_use]
pub fn contrast(a: Color, b: Color) -> f64 {
    contrast_ratio(a, b)
}
