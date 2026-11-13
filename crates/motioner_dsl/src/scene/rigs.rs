//! Symbol instantiation, rig components, properties and baking (prompt 16).
//!
//! This is a child module of `scene`, so it shares the document and the
//! origin table without making them public. The DSL is the only place where
//! rig data becomes document data: symbols become ordinary nodes, and the
//! per-frame result of a rig is written as ordinary keys (D-0052).

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use motioner_core::{
    Animatable, Color, ContentField, ContentValue, Ease, GeometryChannel, NodeContent, NodeId,
    NodeIdentity, Property, PropertyValue, RigAimManifest, RigClipKeyManifest, RigClipManifest,
    RigEventManifest, RigInputManifest, RigInstanceManifest, RigLayerManifest, RigMachineManifest,
    RigManifest, RigMirrorManifest, RigMirrorSide, RigPartManifest, RigPlayManifest,
    RigPoseManifest, RigPoseTrackManifest, RigPropertyKeyManifest, RigPropertyManifest,
    RigPropertyValue, RigStateManifest, RigTransitionManifest, RigValue, RigValueKind, Symbol,
    SymbolPart, SymbolShape, Vec2,
};
use motioner_rig as rig_rt;

use super::{core_error, CreationSite, Scene};
use crate::ast::{
    BinaryOp, CompareOp, Expr as AstExpr, ExprKind, FunctionDef, LogicalOp, Stmt, StmtKind, UnaryOp,
};
use crate::host::{as_vec2, bind, Bound, CallArgs, HostError, HostResult};
use crate::origins::NodeOrigin;
use crate::value::{RigMethod, Value};

/// One node created from a symbol part.
#[derive(Debug, Clone)]
pub(crate) struct PartNode {
    /// Addressable id.
    pub name: String,
    /// The node.
    pub node: NodeId,
}

/// A symbol instantiated in the document.
#[derive(Debug, Clone)]
pub(crate) struct InstantiatedSymbol {
    /// Addressable parts, depth first.
    pub parts: Vec<PartNode>,
}

/// A mirror group: one name edits both sides.
#[derive(Debug, Clone)]
pub(crate) struct MirrorEntry {
    /// Group name.
    pub name: String,
    /// Left property index.
    pub left: usize,
    /// Right property index.
    pub right: usize,
    /// Whether the right side is negated.
    pub flip: bool,
}

/// A built rig attached to its root node.
#[derive(Debug, Clone)]
pub(crate) struct RigEntry {
    /// Root group node (`m` in MDF).
    pub node: NodeId,
    /// The runtime rig.
    pub rig: rig_rt::Rig,
    /// A node per part, same order as `rig.parts`.
    pub part_nodes: Vec<NodeId>,
    /// Composition frames per second.
    pub fps: f64,
    /// Mirror groups.
    pub mirrors: Vec<MirrorEntry>,
    /// Names of properties declared as machine inputs.
    pub inputs: Vec<String>,
}

fn kind_of(default: &Value, declared: Option<&str>) -> HostResult<(rig_rt::PropKind, bool)> {
    if let Some(declared) = declared {
        return match declared {
            "float" => Ok((rig_rt::PropKind::Float, false)),
            "int" => Ok((rig_rt::PropKind::Int, false)),
            "bool" => Ok((rig_rt::PropKind::Bool, false)),
            "enum" => Ok((rig_rt::PropKind::Enum, false)),
            "color" => Ok((rig_rt::PropKind::Color, false)),
            "vec2" => Ok((rig_rt::PropKind::Vec2, false)),
            "angle" => Ok((rig_rt::PropKind::Float, true)),
            other => Err(HostError::value_error(format!(
                "prop(): unknown kind {other:?}; use float, int, bool, enum, color, vec2 or angle"
            ))),
        };
    }
    match default {
        Value::Bool(_) => Ok((rig_rt::PropKind::Bool, false)),
        Value::Number(_) => Ok((rig_rt::PropKind::Float, false)),
        Value::Str(text) if text.starts_with('#') => Ok((rig_rt::PropKind::Color, false)),
        Value::Str(_) => Ok((rig_rt::PropKind::Enum, false)),
        Value::Tuple(items) | Value::List(items) if items.borrow().len() == 2 => {
            Ok((rig_rt::PropKind::Vec2, false))
        }
        other => Err(HostError::type_error(format!(
            "prop(): cannot infer a property kind from a {}, pass kind=\"...\"",
            other.type_name()
        ))),
    }
}

fn coerce(kind: rig_rt::PropKind, value: &Value, what: &str) -> HostResult<rig_rt::PropValue> {
    match kind {
        rig_rt::PropKind::Float | rig_rt::PropKind::Int => match value {
            Value::Number(number) => Ok(rig_rt::PropValue::Number(*number)),
            other => Err(HostError::type_error(format!(
                "{what}: expected a number, got {}",
                other.type_name()
            ))),
        },
        rig_rt::PropKind::Bool => match value {
            Value::Bool(flag) => Ok(rig_rt::PropValue::Bool(*flag)),
            other => Err(HostError::type_error(format!(
                "{what}: expected True or False, got {}",
                other.type_name()
            ))),
        },
        rig_rt::PropKind::Enum => match value {
            Value::Str(text) => Ok(rig_rt::PropValue::Text(text.to_string())),
            other => Err(HostError::type_error(format!(
                "{what}: expected a variant name, got {}",
                other.type_name()
            ))),
        },
        rig_rt::PropKind::Color => match value {
            Value::Str(text) => Color::parse_hex(text)
                .map(rig_rt::PropValue::Color)
                .map_err(|_| HostError::value_error(format!("{what}: invalid colour {text:?}"))),
            other => Err(HostError::type_error(format!(
                "{what}: expected \"#rrggbb\", got {}",
                other.type_name()
            ))),
        },
        rig_rt::PropKind::Vec2 => as_vec2(value)
            .map(rig_rt::PropValue::Vector)
            .ok_or_else(|| HostError::type_error(format!("{what}: expected an (x, y) pair"))),
    }
}

fn ease_of(value: Option<&Value>) -> HostResult<Ease> {
    match value {
        None | Some(Value::None) => Ok(Ease::Linear),
        Some(Value::Ease(ease)) => Ok(*ease),
        Some(Value::Str(name)) => Ease::from_name(name).ok_or_else(|| {
            HostError::value_error(format!(
                "unknown ease {name:?}; use an MDF ease name or bezier(...)"
            ))
        }),
        Some(other) => Err(HostError::type_error(format!(
            "ease must be a string or bezier(...), got {}",
            other.type_name()
        ))),
    }
}

fn number_of(value: &Value, what: &str) -> HostResult<f64> {
    match value {
        Value::Number(number) => Ok(*number),
        other => Err(HostError::type_error(format!(
            "{what} must be a number, got {}",
            other.type_name()
        ))),
    }
}

fn bool_arg(bound: &Bound<'_>, index: usize, default: bool) -> HostResult<bool> {
    match bound.get(index) {
        None | Some(Value::None) => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        Some(other) => Err(HostError::type_error(format!(
            "expected True or False, got {}",
            other.type_name()
        ))),
    }
}

fn list_items(value: &Value, message: &str) -> HostResult<Vec<Value>> {
    match value {
        Value::List(items) | Value::Tuple(items) => Ok(items.borrow().to_vec()),
        _ => Err(HostError::type_error(message)),
    }
}

fn flip_value(value: rig_rt::PropValue) -> rig_rt::PropValue {
    match value {
        rig_rt::PropValue::Number(number) => rig_rt::PropValue::Number(-number),
        rig_rt::PropValue::Vector(vector) => {
            rig_rt::PropValue::Vector(Vec2::new(-vector.x, vector.y))
        }
        other => other,
    }
}

/// The placement scale of a symbol instance: the geometric mean of its axes.
///
/// The authored stroke width of a non-scaling-stroke part is in symbol space,
/// so multiplying it by this factor yields its width in composition pixels
/// even while a driver squashes the instance.
fn placement_scale(scale: Vec2) -> f64 {
    let factor = (scale.x * scale.y).abs().sqrt();
    if factor.is_finite() {
        factor
    } else {
        0.0
    }
}

/// Converts a runtime property value into its manifest form.
fn manifest_value(value: &rig_rt::PropValue) -> RigValue {
    match value {
        rig_rt::PropValue::Number(number) => RigValue::Number(*number),
        rig_rt::PropValue::Bool(flag) => RigValue::Bool(*flag),
        rig_rt::PropValue::Text(text) => RigValue::Text(text.clone()),
        rig_rt::PropValue::Vector(vector) => RigValue::Vec2(*vector),
        rig_rt::PropValue::Color(color) => RigValue::Color(*color),
    }
}

/// Converts a runtime property kind into its manifest form.
fn manifest_kind(kind: rig_rt::PropKind) -> RigValueKind {
    match kind {
        rig_rt::PropKind::Float => RigValueKind::Float,
        rig_rt::PropKind::Int => RigValueKind::Int,
        rig_rt::PropKind::Bool => RigValueKind::Bool,
        rig_rt::PropKind::Enum => RigValueKind::Enum,
        rig_rt::PropKind::Color => RigValueKind::Color,
        rig_rt::PropKind::Vec2 => RigValueKind::Vec2,
    }
}

/// The stable name of a transition comparison.
fn compare_name(compare: rig_rt::Compare) -> &'static str {
    match compare {
        rig_rt::Compare::Equals => "equals",
        rig_rt::Compare::NotEquals => "not_equals",
        rig_rt::Compare::Greater => "greater",
        rig_rt::Compare::Less => "less",
        rig_rt::Compare::Trigger => "trigger",
    }
}

impl Scene {
    fn fps(&self) -> HostResult<f64> {
        Ok(self.document()?.composition().frame_rate().as_f64())
    }

    fn rig_index(&self, node: NodeId) -> HostResult<usize> {
        self.rig_nodes.get(&node).copied().ok_or_else(|| {
            HostError::value_error(
                "this node is not a rig; create one with rig(use(\"x.svg\"), ...)",
            )
        })
    }

    /// `symbol(pos=(0, 0), scale=1)` and `use("x.svg")(...)`.
    pub(crate) fn instantiate_symbol(
        &mut self,
        symbol: &Symbol,
        args: CallArgs,
        site: &CreationSite,
    ) -> HostResult<Value> {
        let bound = bind("symbol", &["pos", "scale"], 0, args)?;
        let pos = match bound.get(0) {
            None | Some(Value::None) => Vec2::ZERO,
            Some(value) => as_vec2(value)
                .ok_or_else(|| HostError::type_error("symbol(): 'pos' must be an (x, y) pair"))?,
        };
        let scale = match bound.get(1) {
            None | Some(Value::None) => Vec2::ONE,
            Some(Value::Number(number)) => Vec2::new(*number, *number),
            Some(value) => as_vec2(value).ok_or_else(|| {
                HostError::type_error("symbol(): 'scale' must be a number or an (x, y) pair")
            })?,
        };
        let mut used = HashSet::new();
        let mut parts = Vec::new();
        let base = format!("{}/svg", site.key());
        let placement = placement_scale(scale);
        let mut roots = Vec::new();
        for part in &symbol.parts {
            let node =
                self.instantiate_part(part, &base, &mut used, &mut parts, site, placement)?;
            roots.push(node);
        }
        let root = if roots.len() == 1 {
            roots[0]
        } else {
            let document = self.document_mut()?;
            let root = document
                .add_node_with_identity(
                    NodeContent::Group,
                    NodeIdentity::provenance(format!("{base}/root")),
                )
                .map_err(|error| core_error(&error))?;
            for child in roots {
                document
                    .reparent(child, root)
                    .map_err(|error| core_error(&error))?;
            }
            self.origins.record_node(NodeOrigin {
                node: root,
                creation: site.creation,
                statement: site.statement,
                block: site.block,
                site: Arc::clone(&site.site),
                bindings: Vec::new(),
            });
            root
        };
        let centre = symbol.bounds().map_or(Vec2::ZERO, |bounds| {
            Vec2::new(
                f64::midpoint(bounds[0], bounds[2]),
                f64::midpoint(bounds[1], bounds[3]),
            )
        });
        let document = self.document_mut()?;
        // A single top-level part keeps its own id as display name; a
        // synthetic wrapper is named after the symbol.
        if document.node(root).is_none_or(|node| node.name().is_none()) {
            document
                .set_name(root, &symbol.name)
                .map_err(|error| core_error(&error))?;
        }
        let current = document
            .node(root)
            .map_or(centre, |node| node.transform().position.value());
        document
            .set_property(root, Property::Position, PropertyValue::Vec2(current + pos))
            .map_err(|error| core_error(&error))?;
        document
            .set_property(root, Property::Scale, PropertyValue::Vec2(scale))
            .map_err(|error| core_error(&error))?;
        self.symbols.insert(root, InstantiatedSymbol { parts });
        Ok(Value::Node(root))
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    // one recursion for a whole subtree
    fn instantiate_part(
        &mut self,
        part: &SymbolPart,
        base: &str,
        used: &mut HashSet<String>,
        parts: &mut Vec<PartNode>,
        site: &CreationSite,
        placement: f64,
    ) -> HostResult<NodeId> {
        let _ = site;
        let anchor = part.anchor;
        let mut identity = match part.id.as_deref() {
            Some(id) => format!("{base}#{id}"),
            None => format!("{base}#anonymous"),
        };
        let mut counter = 1;
        while !used.insert(identity.clone()) {
            identity = format!("{base}#part{counter}");
            counter += 1;
        }
        let mut children: &[SymbolPart] = &[];
        let node = {
            let document = self.document_mut()?;
            match &part.shape {
                SymbolShape::Group(group) => {
                    children = group;
                    document
                        .add_node_with_identity(
                            NodeContent::Group,
                            NodeIdentity::provenance(identity),
                        )
                        .map_err(|error| core_error(&error))?
                }
                SymbolShape::Path(path) => {
                    // The authored width is in symbol space; the placement
                    // scale converts it to composition pixels when the
                    // renderer strokes a non-scaling outline.
                    let mut paint = path.paint.clone();
                    paint.non_scaling_stroke = paint
                        .non_scaling_stroke
                        .map(|reference| reference * placement);
                    let content = NodeContent::Path {
                        geometry: GeometryChannel::new(path.geometry.clone()),
                        paint,
                        trim_start: Animatable::new(0.0),
                        trim_end: Animatable::new(1.0),
                        trim_offset: Animatable::new(0.0),
                        dash: Animatable::new(path.dash.unwrap_or(0.0)),
                        dash_offset: Animatable::new(0.0),
                    };
                    document
                        .add_node_with_identity(content, NodeIdentity::provenance(identity))
                        .map_err(|error| core_error(&error))?
                }
            }
        };
        self.origins.record_node(NodeOrigin {
            node,
            creation: site.creation,
            statement: site.statement,
            block: site.block,
            site: Arc::clone(&site.site),
            bindings: Vec::new(),
        });
        {
            let document = self.document_mut()?;
            document
                .set_property(node, Property::Position, PropertyValue::Vec2(anchor))
                .map_err(|error| core_error(&error))?;
            document
                .set_property(node, Property::Anchor, PropertyValue::Vec2(anchor))
                .map_err(|error| core_error(&error))?;
            document
                .set_property(node, Property::Opacity, PropertyValue::Scalar(part.opacity))
                .map_err(|error| core_error(&error))?;
            if let Some(id) = part.id.as_deref() {
                document
                    .set_name(node, id)
                    .map_err(|error| core_error(&error))?;
            }
        }
        if let Some(id) = part.id.as_deref() {
            parts.push(PartNode {
                name: id.to_owned(),
                node,
            });
        }
        for child in children {
            let child_node = self.instantiate_part(child, base, used, parts, site, placement)?;
            self.document_mut()?
                .reparent(child_node, node)
                .map_err(|error| core_error(&error))?;
        }
        if !part.clip.is_empty() {
            let clip_group = self
                .document_mut()?
                .add_node_with_identity(
                    NodeContent::Group,
                    NodeIdentity::provenance(format!("{base}/clip/{node}")),
                )
                .map_err(|error| core_error(&error))?;
            self.document_mut()?
                .set_name(clip_group, "clip")
                .map_err(|error| core_error(&error))?;
            self.document_mut()?
                .reparent(clip_group, node)
                .map_err(|error| core_error(&error))?;
            for clip in &part.clip {
                let clip_node = self.instantiate_part(clip, base, used, parts, site, placement)?;
                self.document_mut()?
                    .reparent(clip_node, clip_group)
                    .map_err(|error| core_error(&error))?;
            }
            self.document_mut()?
                .set_mask(node, clip_group, motioner_core::MaskMode::Alpha, false)
                .map_err(|error| core_error(&error))?;
        }
        Ok(node)
    }

    fn node_base(&self, node: NodeId) -> HostResult<(Vec2, f64, Vec2, Vec2, f64)> {
        let node = self
            .document()?
            .node(node)
            .ok_or_else(|| HostError::value_error(format!("unknown node {node}")))?;
        let transform = node.transform();
        Ok((
            transform.position.value(),
            transform.rotation.value(),
            transform.scale.value(),
            transform.anchor.value(),
            transform.opacity.value(),
        ))
    }

    /// `rig(symbol_or_node, pos=..., scale=..., name=...)`.
    pub(crate) fn rig_component(
        &mut self,
        args: CallArgs,
        site: &CreationSite,
    ) -> HostResult<Value> {
        let bound = bind("rig", &["art", "pos", "scale", "name"], 1, args)?;
        let art = bound.get(0).cloned().expect("required");
        let root = match art {
            Value::Node(node) => {
                if !self.symbols.contains_key(&node) {
                    return Err(HostError::value_error(
                        "rig(): the node is not a symbol instance; pass use(\"x.svg\") or its call result",
                    ));
                }
                node
            }
            Value::Symbol(symbol) => {
                let mut call_args = CallArgs::default();
                if let Some(value) = bound.argument(1) {
                    call_args.keywords.push((Arc::from("pos"), value.clone()));
                }
                if let Some(value) = bound.argument(2) {
                    call_args.keywords.push((Arc::from("scale"), value.clone()));
                }
                let Value::Node(node) = self.instantiate_symbol(&symbol, call_args, site)? else {
                    unreachable!("a symbol always creates a node");
                };
                node
            }
            other => {
                return Err(HostError::type_error(format!(
                    "rig(): 'art' must be a symbol, got {}",
                    other.type_name()
                )))
            }
        };
        let instance = self
            .symbols
            .get(&root)
            .cloned()
            .ok_or_else(|| HostError::value_error("rig(): the symbol instance has no parts"))?;
        let name = bound.string_or(3, "")?;
        let rig_name = if name.is_empty() {
            instance
                .parts
                .first()
                .map_or("rig".to_owned(), |part| part.name.clone())
        } else {
            name.to_string()
        };
        let mut rig = rig_rt::Rig::new(rig_name);
        let mut part_nodes = Vec::new();
        for part in &instance.parts {
            let (position, rotation, scale, anchor, opacity) = self.node_base(part.node)?;
            let geometry = self
                .document()?
                .node(part.node)
                .and_then(|node| match node.content() {
                    NodeContent::Path { geometry, .. } => Some(geometry.geometry().clone()),
                    _ => None,
                });
            rig.add_part(rig_rt::PartBase {
                name: part.name.clone(),
                position,
                rotation,
                scale,
                anchor,
                opacity,
                fill: None,
                stroke: None,
                stroke_width: 0.0,
                geometry,
            });
            part_nodes.push(part.node);
        }
        let fps = self.fps()?;
        let index = self.rigs.len();
        self.rigs.push(RigEntry {
            node: root,
            rig,
            part_nodes,
            fps,
            mirrors: Vec::new(),
            inputs: Vec::new(),
        });
        self.rig_nodes.insert(root, index);
        Ok(Value::Node(root))
    }

    /// Reads a rig property (`m.name`) as a property handle.
    pub(crate) fn rig_property(&self, node: NodeId, name: &str) -> HostResult<Value> {
        let index = self.rig_index(node)?;
        let entry = &self.rigs[index];
        if entry.mirrors.iter().any(|mirror| mirror.name == name)
            || entry.rig.property(name).is_some()
        {
            return Ok(Value::RigProperty(node, Arc::from(name)));
        }
        Err(HostError::value_error(format!(
            "the rig has no property {name:?}; declared properties are {}",
            entry
                .rig
                .properties
                .iter()
                .map(|property| property.name.as_str())
                .chain(entry.mirrors.iter().map(|mirror| mirror.name.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    /// `m.prop("name", default, range=..., step=..., kind=..., values=..., angle=...)`.
    #[allow(clippy::too_many_lines)] // one argument per property facet
    pub(crate) fn rig_prop(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = [
            "name", "default", "range", "step", "kind", "values", "angle", "mirror", "flip",
        ];
        let bound = bind("prop", &params, 1, args)?;
        let name = bound.string_or(0, "")?;
        if name.is_empty() {
            return Err(HostError::value_error(
                "prop(): 'name' must be a non-empty string",
            ));
        }
        if let Some(sides) = bound.get(7) {
            let flip = matches!(bound.get(8), Some(Value::Bool(true)));
            return self.register_mirror(node, &name, sides, flip);
        }
        let default = bound.get(1).cloned().unwrap_or(Value::Number(0.0));
        let declared = match bound.get(4) {
            None | Some(Value::None) => None,
            Some(Value::Str(kind)) => Some(kind.to_string()),
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "prop(): 'kind' must be a string, got {}",
                    other.type_name()
                )))
            }
        };
        let (kind, inferred_angle) = kind_of(&default, declared.as_deref())?;
        let angle = inferred_angle || matches!(bound.get(6), Some(Value::Bool(true)));
        let default = coerce(kind, &default, "prop()")?;
        let mut values = Vec::new();
        if let Some(list) = bound.get(5) {
            for item in list_items(list, "prop(): 'values' must be a list of names")? {
                let Value::Str(text) = item else {
                    return Err(HostError::type_error(
                        "prop(): 'values' must contain only strings",
                    ));
                };
                values.push(text.to_string());
            }
        }
        if kind == rig_rt::PropKind::Enum && values.is_empty() {
            if let rig_rt::PropValue::Text(text) = &default {
                values.push(text.clone());
            }
        }
        if let rig_rt::PropValue::Text(text) = &default {
            if !values.is_empty() && !values.contains(text) {
                return Err(HostError::value_error(format!(
                    "prop(): default {text:?} is not one of the declared values"
                )));
            }
        }
        let (min, max) = parse_range(bound.get(2))?;
        let step = match bound.get(3) {
            None | Some(Value::None) => 1.0,
            Some(Value::Number(step)) if *step > 0.0 => *step,
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "prop(): 'step' must be a positive number, got {}",
                    other.type_name()
                )))
            }
        };
        let index = self.rig_index(node)?;
        if self.rigs[index].rig.property(&name).is_some() {
            return Err(HostError::value_error(format!(
                "prop(): the rig already declares {name:?}"
            )));
        }
        self.rigs[index].rig.add_property(rig_rt::PropertyDef {
            name: name.to_string(),
            kind,
            default,
            min,
            max,
            step,
            values,
            angle,
        });
        Ok(Value::None)
    }

    /// `r.mirror("lid", ("lid_L", "lid_R"), flip=False)`.
    pub(crate) fn rig_mirror(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("mirror", &["name", "sides", "flip"], 2, args)?;
        let name = bound.string_or(0, "")?;
        let sides = bound.get(1).cloned().expect("required");
        let flip = matches!(bound.get(2), Some(Value::Bool(true)));
        self.register_mirror(node, &name, &sides, flip)
    }

    fn register_mirror(
        &mut self,
        node: NodeId,
        group: &str,
        sides: &Value,
        flip: bool,
    ) -> HostResult<Value> {
        let list = list_items(sides, "prop(): 'mirror' must be a pair of property names")?;
        if list.len() != 2 {
            return Err(HostError::value_error(
                "prop(): 'mirror' must name exactly two properties",
            ));
        }
        let names: Vec<String> = list
            .iter()
            .map(|item| match item {
                Value::Str(text) => Ok(text.to_string()),
                other => Err(HostError::type_error(format!(
                    "prop(): 'mirror' names must be strings, got {}",
                    other.type_name()
                ))),
            })
            .collect::<HostResult<_>>()?;
        let index = self.rig_index(node)?;
        let left = self.rigs[index].rig.property(&names[0]).ok_or_else(|| {
            HostError::value_error(format!("prop(): unknown property {:?}", names[0]))
        })?;
        let right = self.rigs[index].rig.property(&names[1]).ok_or_else(|| {
            HostError::value_error(format!("prop(): unknown property {:?}", names[1]))
        })?;
        self.rigs[index].mirrors.push(MirrorEntry {
            name: group.to_owned(),
            left,
            right,
            flip,
        });
        Ok(Value::None)
    }

    /// Writes a key or the static value of one rig property.
    pub(crate) fn rig_property_key(
        &mut self,
        node: NodeId,
        name: &str,
        method: RigMethod,
        args: CallArgs,
    ) -> HostResult<Value> {
        let index = self.rig_index(node)?;
        let mirrors = self.rigs[index]
            .mirrors
            .iter()
            .find(|mirror| mirror.name == name)
            .cloned();
        let fps = self.rigs[index].fps;
        let properties: Vec<(usize, bool)> = if let Some(mirror) = mirrors {
            vec![(mirror.left, false), (mirror.right, mirror.flip)]
        } else {
            let property = self.rigs[index].rig.property(name).ok_or_else(|| {
                HostError::value_error(format!("the rig has no property {name:?}"))
            })?;
            vec![(property, false)]
        };
        match method {
            RigMethod::Set => {
                let bound = bind("set", &["value"], 1, args)?;
                let source = bound.get(0).cloned().expect("required");
                for (property, flip) in properties {
                    let def = &self.rigs[index].rig.properties[property];
                    let mut value = coerce(def.kind, &source, "set()")?;
                    if flip {
                        value = flip_value(value);
                    }
                    let channel = &mut self.rigs[index].rig.channels[property];
                    channel.keys.retain(|key| key.frame > 0.0);
                    channel
                        .keys
                        .insert(0, rig_rt::Key::new(0.0, value, Ease::Linear));
                }
                Ok(Value::None)
            }
            RigMethod::Key => {
                let bound = bind("key", &["time", "value", "ease"], 2, args)?;
                let time = bound.number(0)?;
                let source = bound.get(1).cloned().expect("required");
                let ease = ease_of(bound.get(2))?;
                let frame = time * fps;
                for (property, flip) in properties {
                    let def = &self.rigs[index].rig.properties[property];
                    let mut value = coerce(def.kind, &source, "key()")?;
                    if flip {
                        value = flip_value(value);
                    }
                    let channel = &mut self.rigs[index].rig.channels[property];
                    insert_key(channel, frame, value, ease);
                }
                Ok(Value::None)
            }
        }
    }

    /// `m.part("eye-L")`.
    pub(crate) fn rig_part(&self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("part", &["name"], 1, args)?;
        let name = bound.string_or(0, "")?;
        self.rig_part_node(node, &name).map(Value::Node)
    }

    fn rig_part_node(&self, node: NodeId, name: &str) -> HostResult<NodeId> {
        let index = self.rig_index(node)?;
        let parts = &self.symbols[&self.rigs[index].node].parts;
        parts.iter().find(|part| *part.name == *name).map_or_else(
            || {
                Err(HostError::value_error(format!(
                    "part(): no part {name:?}; parts are {}",
                    parts
                        .iter()
                        .map(|part| part.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )))
            },
            |part| Ok(part.node),
        )
    }

    pub(crate) fn rig_parts(&self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        bind("parts", &[], 0, args)?;
        let index = self.rig_index(node)?;
        let parts = &self.symbols[&self.rigs[index].node].parts;
        Ok(Value::list(
            parts
                .iter()
                .map(|part| {
                    Value::tuple(vec![
                        Value::Str(Arc::from(part.name.as_str())),
                        Value::Node(part.node),
                    ])
                })
                .collect(),
        ))
    }

    /// `r.driver(fn)`.
    pub(crate) fn rig_driver(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("driver", &["function"], 1, args)?;
        let Some(Value::Function(function)) = bound.get(0) else {
            let got = bound.get(0).map_or("nothing", Value::type_name);
            return Err(HostError::type_error(format!(
                "driver(): 'function' must be a MDF function, got {got}"
            )));
        };
        let index = self.rig_index(node)?;
        let driver = compile_driver(&self.rigs[index].rig, function)?;
        self.rigs[index].rig.drivers.push(driver);
        Ok(Value::None)
    }

    /// `m.geometry("lid-L", 1.0, [[x, y], ...])`: keys a part's points
    /// (D-0058). The part's closure and style stay fixed.
    pub(crate) fn rig_geometry(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("geometry", &["part", "time", "points", "ease"], 3, args)?;
        let part = bound.string_or(0, "")?;
        let target = self.rig_part_node(node, &part)?;
        let time = bound.number(1)?;
        if !time.is_finite() {
            return Err(HostError::value_error(
                "geometry(): 'time' must be a finite number of seconds",
            ));
        }
        let points = super::path_points(bound.get(2).unwrap_or(&Value::None))?;
        let timing = self.key_timing(&bound, time)?;
        self.document_mut()?
            .add_content_keyframe(
                target,
                ContentField::Geometry,
                timing.frame,
                ContentValue::GeometryPoints(points),
                timing.ease,
            )
            .map_err(|error| core_error(&error))?;
        Ok(Value::None)
    }

    /// `r.pose(name, ...)`: declares a pose, or applies it when `at` is given.
    pub(crate) fn rig_pose(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let mut positional = args.positional.into_iter();
        let name = match positional.next().map(|value| value.value) {
            Some(Value::Str(name)) => name.to_string(),
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "pose(): the name must be a string, got {}",
                    other.type_name()
                )))
            }
            None => return Err(HostError::type_error("pose(): the name is required")),
        };
        let mut at = None;
        let mut duration = None;
        let mut ease = Ease::Linear;
        let mut values: Vec<(String, Value)> = Vec::new();
        let mut reset = false;
        for (keyword, call) in args.keywords {
            match &*keyword {
                "at" => at = Some(number_of(&call.value, "pose(): 'at'")?),
                "dur" => duration = Some(number_of(&call.value, "pose(): 'dur'")?),
                "ease" => ease = ease_of(Some(&call.value))?,
                "reset" => reset = matches!(call.value, Value::Bool(true)),
                other => values.push((other.to_owned(), call.value.clone())),
            }
        }
        let index = self.rig_index(node)?;
        let Some(at) = at else {
            if values.is_empty() {
                return Err(HostError::value_error(
                    "pose(): declare property values or pass at=... to apply a declared pose",
                ));
            }
            if self.rigs[index].rig.pose(&name).is_some() {
                return Err(HostError::value_error(format!(
                    "pose(): pose {name:?} is already declared"
                )));
            }
            let mut map = BTreeMap::new();
            for (property, value) in values {
                self.set_pose_entry(index, &name, &property, &value, &mut map)?;
            }
            self.rigs[index].rig.poses.push(rig_rt::Pose {
                name,
                values: map,
                reset,
            });
            return Ok(Value::None);
        };
        let pose = self.rigs[index].rig.pose(&name).ok_or_else(|| {
            HostError::value_error(format!(
                "pose(): no declared pose {name:?}; declare it with r.pose(\"{name}\", prop=value)"
            ))
        })?;
        let duration = duration.unwrap_or(0.3);
        if !(duration.is_finite() && duration >= 0.0) {
            return Err(HostError::value_error(
                "pose(): 'dur' must be a non-negative number of seconds",
            ));
        }
        let fps = self.rigs[index].fps;
        self.rigs[index].rig.tracks.push(rig_rt::PoseTrack {
            pose,
            start: at * fps,
            duration: duration * fps,
            ease,
        });
        Ok(Value::None)
    }

    fn set_pose_entry(
        &self,
        index: usize,
        pose: &str,
        property: &str,
        value: &Value,
        map: &mut BTreeMap<usize, rig_rt::PropValue>,
    ) -> HostResult<()> {
        let entry = &self.rigs[index];
        if let Some(mirror) = entry.mirrors.iter().find(|mirror| mirror.name == property) {
            for (side, flip) in [(mirror.left, false), (mirror.right, mirror.flip)] {
                let def = &entry.rig.properties[side];
                let mut value = coerce(def.kind, value, &format!("pose {pose:?}"))?;
                if flip {
                    value = flip_value(value);
                }
                map.insert(side, value);
            }
            return Ok(());
        }
        let side = entry.rig.property(property).ok_or_else(|| {
            HostError::value_error(format!(
                "pose {pose:?}: the rig has no property {property:?}"
            ))
        })?;
        let def = &entry.rig.properties[side];
        map.insert(side, coerce(def.kind, value, &format!("pose {pose:?}"))?);
        Ok(())
    }

    /// `r.clip(name)` returns a handle whose `.key` edits the clip.
    pub(crate) fn rig_clip(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("clip", &["name", "length"], 1, args)?;
        let name = bound.string_or(0, "")?;
        let index = self.rig_index(node)?;
        if self.rigs[index].rig.clip(&name).is_none() {
            let length = bound.number_or(1, 0.0)? * self.rigs[index].fps;
            self.rigs[index].rig.clips.push(rig_rt::Clip {
                name: name.to_string(),
                keys: Vec::new(),
                length,
            });
        }
        Ok(Value::Clip(node, Arc::clone(&name)))
    }

    /// `cl.key(property, time, value, ease=...)`.
    pub(crate) fn rig_clip_key(
        &mut self,
        node: NodeId,
        clip: &str,
        args: CallArgs,
    ) -> HostResult<Value> {
        let bound = bind("key", &["property", "time", "value", "ease"], 3, args)?;
        let property = bound.string_or(0, "")?;
        let time = bound.number(1)?;
        let source = bound.get(2).cloned().expect("required");
        let ease = ease_of(bound.get(3))?;
        let index = self.rig_index(node)?;
        let property = self.rigs[index].rig.property(&property).ok_or_else(|| {
            HostError::value_error(format!("clip key: the rig has no property {property:?}"))
        })?;
        let def = &self.rigs[index].rig.properties[property];
        let value = coerce(def.kind, &source, "clip key")?;
        let frame = time * self.rigs[index].fps;
        let clip_index = self.rigs[index]
            .rig
            .clip(clip)
            .ok_or_else(|| HostError::value_error(format!("clip key: no clip {clip:?}")))?;
        self.rigs[index].rig.clips[clip_index]
            .keys
            .push(rig_rt::ClipKey {
                property,
                frame,
                value,
                ease,
            });
        if frame > self.rigs[index].rig.clips[clip_index].length {
            self.rigs[index].rig.clips[clip_index].length = frame;
        }
        Ok(Value::None)
    }

    /// `m.play(name, at=..., loops=1, speed=1.0)`.
    pub(crate) fn rig_play(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("play", &["name", "at", "loops", "speed"], 1, args)?;
        let name = bound.string_or(0, "")?;
        let at = bound.number_or(1, 0.0)?;
        let loops = bound.integer_in(2, 1, 0, 10_000)?;
        let speed = bound.number_or(3, 1.0)?;
        let index = self.rig_index(node)?;
        let clip = self.rigs[index]
            .rig
            .clip(&name)
            .ok_or_else(|| HostError::value_error(format!("play(): no clip {name:?}")))?;
        if !(speed.is_finite() && speed > 0.0) {
            return Err(HostError::value_error(
                "play(): 'speed' must be a positive number",
            ));
        }
        let start = at * self.rigs[index].fps;
        self.rigs[index].rig.plays.push(rig_rt::Play {
            clip,
            start,
            loops: u32::try_from(loops).unwrap_or(u32::MAX),
            speed,
        });
        Ok(Value::None)
    }

    /// `r.input(name, default, kind=..., values=[...])`.
    pub(crate) fn rig_input(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("input", &["name", "default", "kind", "values"], 2, args)?;
        let name = bound.string_or(0, "")?;
        let index = self.rig_index(node)?;
        if self.rigs[index].rig.property(&name).is_none() {
            let mut call = CallArgs::default();
            let mut push = |keyword: &str, value: Value| {
                call.keywords.push((
                    Arc::from(keyword),
                    crate::host::CallValue {
                        value,
                        span: crate::diagnostic::Span::default(),
                        literal: None,
                        origin: crate::origins::ExpressionOrigin::Computed,
                    },
                ));
            };
            push("name", Value::Str(Arc::clone(&name)));
            if let Some(default) = bound.get(1) {
                push("default", default.clone());
            }
            if let Some(kind) = bound.get(2) {
                push("kind", kind.clone());
            }
            if let Some(values) = bound.get(3) {
                push("values", values.clone());
            }
            self.rig_prop(node, call)?;
        }
        if !self.rigs[index].inputs.iter().any(|input| input == &*name) {
            self.rigs[index].inputs.push(name.to_string());
        }
        Ok(Value::RigProperty(node, name))
    }

    /// `r.state(name, pose=..., clip=..., speed=1.0, looping=True)`.
    pub(crate) fn rig_state(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind(
            "state",
            &["name", "pose", "clip", "speed", "looping"],
            1,
            args,
        )?;
        let name = bound.string_or(0, "")?;
        let pose =
            match bound.get(1) {
                None | Some(Value::None) => None,
                Some(Value::Str(pose)) => {
                    let index = self.rig_index(node)?;
                    Some(self.rigs[index].rig.pose(pose).ok_or_else(|| {
                        HostError::value_error(format!("state(): no pose {pose:?}"))
                    })?)
                }
                Some(other) => {
                    return Err(HostError::type_error(format!(
                        "state(): 'pose' must be a string, got {}",
                        other.type_name()
                    )))
                }
            };
        let clip =
            match bound.get(2) {
                None | Some(Value::None) => None,
                Some(Value::Str(clip)) => {
                    let index = self.rig_index(node)?;
                    Some(self.rigs[index].rig.clip(clip).ok_or_else(|| {
                        HostError::value_error(format!("state(): no clip {clip:?}"))
                    })?)
                }
                Some(other) => {
                    return Err(HostError::type_error(format!(
                        "state(): 'clip' must be a string, got {}",
                        other.type_name()
                    )))
                }
            };
        let speed = bound.number_or(3, 1.0)?;
        let looping = !matches!(bound.get(4), Some(Value::Bool(false)));
        let index = self.rig_index(node)?;
        if self.rigs[index]
            .rig
            .machine
            .as_ref()
            .is_some_and(|machine| {
                machine
                    .states
                    .iter()
                    .any(|state| state.name.as_str() == &*name)
            })
        {
            return Err(HostError::value_error(format!(
                "state(): a state named {name:?} already exists"
            )));
        }
        let machine = self.rigs[index]
            .rig
            .machine
            .get_or_insert_with(|| rig_rt::Machine {
                states: Vec::new(),
                transitions: Vec::new(),
                default: 0,
            });
        machine.states.push(rig_rt::State {
            name: name.to_string(),
            pose,
            clip,
            speed,
            looping,
        });
        Ok(Value::None)
    }

    /// `r.transition(from, to, input="mood", equals="sad", dur=0.3, ...)`.
    pub(crate) fn rig_transition(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = [
            "from", "to", "input", "equals", "gt", "lt", "trigger", "dur",
        ];
        let bound = bind("transition", &params, 2, args)?;
        let from_name = bound.string_or(0, "")?;
        let to_name = bound.string_or(1, "")?;
        let index = self.rig_index(node)?;
        let machine = self.rigs[index]
            .rig
            .machine
            .as_ref()
            .ok_or_else(|| HostError::value_error("transition(): declare states first"))?;
        let from = if &*from_name == "*" {
            None
        } else {
            Some(
                machine
                    .states
                    .iter()
                    .position(|state| state.name.as_str() == &*from_name)
                    .ok_or_else(|| {
                        HostError::value_error(format!("transition(): no state {from_name:?}"))
                    })?,
            )
        };
        let to = machine
            .states
            .iter()
            .position(|state| state.name.as_str() == &*to_name)
            .ok_or_else(|| HostError::value_error(format!("transition(): no state {to_name:?}")))?;
        let (input, compare, condition) = if let Some(input) = bound.get(2) {
            let Value::Str(input) = input else {
                return Err(HostError::type_error(
                    "transition(): 'input' must be a property name",
                ));
            };
            let property = self.rigs[index].rig.property(input).ok_or_else(|| {
                HostError::value_error(format!("transition(): no property {input:?}"))
            })?;
            let (compare, condition) = if let Some(value) = bound.get(3) {
                (rig_rt::Compare::Equals, value.clone())
            } else if let Some(value) = bound.get(4) {
                (rig_rt::Compare::Greater, value.clone())
            } else if let Some(value) = bound.get(5) {
                (rig_rt::Compare::Less, value.clone())
            } else {
                return Err(HostError::value_error(
                    "transition(): pass equals=, gt= or lt= with input=",
                ));
            };
            (property, compare, condition)
        } else if let Some(trigger) = bound.get(6) {
            let Value::Str(trigger) = trigger else {
                return Err(HostError::type_error(
                    "transition(): 'trigger' must be a property name",
                ));
            };
            let property = self.rigs[index].rig.property(trigger).ok_or_else(|| {
                HostError::value_error(format!("transition(): no property {trigger:?}"))
            })?;
            (property, rig_rt::Compare::Trigger, Value::Bool(true))
        } else {
            return Err(HostError::value_error(
                "transition(): pass input= with equals=/gt=/lt=, or trigger=",
            ));
        };
        let def = &self.rigs[index].rig.properties[input];
        let value = coerce(def.kind, &condition, "transition()")?;
        let duration = bound.number_or(7, 0.0)?;
        if !(duration.is_finite() && duration >= 0.0) {
            return Err(HostError::value_error(
                "transition(): 'dur' must be a non-negative number",
            ));
        }
        let length = duration * self.rigs[index].fps;
        self.rigs[index]
            .rig
            .machine
            .as_mut()
            .expect("checked above")
            .transitions
            .push(rig_rt::Transition {
                from,
                to,
                input,
                compare,
                value,
                duration: length,
            });
        Ok(Value::None)
    }

    /// `m.fire(name, at=...)`: keys a boolean trigger at a frame.
    pub(crate) fn rig_fire(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("fire", &["name", "at"], 1, args)?;
        let name = bound.string_or(0, "")?;
        let at = bound.number_or(1, 0.0)?;
        let index = self.rig_index(node)?;
        if self.rigs[index].rig.property(&name).is_none() {
            self.rigs[index]
                .rig
                .add_property(rig_rt::PropertyDef::typed(
                    name.to_string(),
                    rig_rt::PropKind::Bool,
                    rig_rt::PropValue::Bool(false),
                ));
            self.rigs[index].inputs.push(name.to_string());
        }
        let property = self.rigs[index].rig.property(&name).expect("just added");
        if self.rigs[index].rig.properties[property].kind != rig_rt::PropKind::Bool {
            return Err(HostError::value_error(format!(
                "fire(): {name:?} must be a bool property"
            )));
        }
        let frame = at * self.rigs[index].fps;
        let channel = &mut self.rigs[index].rig.channels[property];
        for (offset, value) in [(-0.5, false), (0.0, true), (1.0, false)] {
            insert_key(
                channel,
                frame + offset,
                rig_rt::PropValue::Bool(value),
                Ease::Hold,
            );
        }
        Ok(Value::None)
    }

    /// `r.aim(part, property, radius=...)`.
    pub(crate) fn rig_aim(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("aim", &["part", "property", "radius"], 2, args)?;
        let part = bound.string_or(0, "")?;
        let property = bound.string_or(1, "")?;
        let radius = bound.number_or(2, 6.0)?;
        let index = self.rig_index(node)?;
        if let Some(bone) = part.strip_prefix("bone:") {
            let bone = self.rig_bone_index(index, bone, "aim")?;
            let property = self.rig_vec2_property(index, &property, "aim")?;
            self.rigs[index]
                .rig
                .bone_constraints
                .push(rig_rt::BoneConstraint::Aim {
                    bone,
                    property,
                    radius,
                });
            return Ok(Value::None);
        }
        let part = self.rigs[index]
            .rig
            .part(&part)
            .ok_or_else(|| HostError::value_error(format!("aim(): no part {part:?}")))?;
        let property = self.rigs[index]
            .rig
            .property(&property)
            .ok_or_else(|| HostError::value_error(format!("aim(): no property {property:?}")))?;
        if self.rigs[index].rig.properties[property].kind != rig_rt::PropKind::Vec2 {
            return Err(HostError::value_error(
                "aim(): the property must be a vec2 (an (x, y) pair)",
            ));
        }
        self.rigs[index].rig.aims.push(rig_rt::Aim {
            part,
            property,
            radius,
        });
        Ok(Value::None)
    }

    /// `r.blink("lid", every=3.5, jitter=0.6, duration=0.08, closed=1.0)`.
    pub(crate) fn rig_blink(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = [
            "property", "every", "jitter", "duration", "closed", "start", "seed",
        ];
        let bound = bind("blink", &params, 1, args)?;
        let property = bound.string_or(0, "")?;
        let every = bound.number_or(1, 3.5)?;
        let jitter = bound.number_or(2, 0.0)?;
        let duration = bound.number_or(3, 0.08)?;
        let closed = bound.get(4).cloned().unwrap_or(Value::Number(1.0));
        let start = bound.number_or(5, 0.0)?;
        let seed = bound.integer_in(6, 0, 0, i64::from(u32::MAX))?;
        if !(every.is_finite() && every > 0.0) {
            return Err(HostError::value_error(
                "blink(): 'every' must be a positive number of seconds",
            ));
        }
        let index = self.rig_index(node)?;
        let fps = self.rigs[index].fps;
        let properties: Vec<usize> = property
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| {
                self.rigs[index]
                    .rig
                    .property(name)
                    .ok_or_else(|| HostError::value_error(format!("blink(): no property {name:?}")))
            })
            .collect::<HostResult<_>>()?;
        if properties.is_empty() {
            return Err(HostError::value_error("blink(): name a property"));
        }
        let mut props = Vec::new();
        for property in properties {
            let def = &self.rigs[index].rig.properties[property];
            props.push((property, coerce(def.kind, &closed, "blink()")?));
        }
        self.rigs[index].rig.blinks.push(rig_rt::Blink {
            start: start * fps,
            period: every * fps,
            jitter: jitter * fps,
            duration: duration * fps,
            seed: u64::try_from(seed).unwrap_or(0),
            props,
        });
        Ok(Value::None)
    }

    /// `r.lattice("body", corners=("tl", "tr", "br", "bl"), edges=...)`.
    pub(crate) fn rig_lattice(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("lattice", &["part", "corners", "edges"], 2, args)?;
        let index = self.rig_index(node)?;
        let part = self.rig_part_index(index, &bound.string_or(0, "")?, "lattice")?;
        let corners = self.rig_property_list(index, &bound, 1, "lattice")?;
        let corners: [usize; 4] = corners.try_into().map_err(|_| {
            HostError::value_error("lattice(): 'corners' must name exactly four vec2 properties")
        })?;
        let mut edges = [None; 4];
        if let Some(value) = bound.get(2) {
            if !matches!(value, Value::None) {
                let items = list_items(value, "lattice(): 'edges' must be a list of four items")?;
                if items.len() != 4 {
                    return Err(HostError::value_error(
                        "lattice(): 'edges' must be a list of four items",
                    ));
                }
                for (slot, item) in edges.iter_mut().zip(items) {
                    if matches!(item, Value::None) {
                        continue;
                    }
                    let name = match item {
                        Value::Str(name) => name,
                        other => {
                            return Err(HostError::type_error(format!(
                                "lattice(): edge properties must be names or None, got {}",
                                other.type_name()
                            )))
                        }
                    };
                    *slot = Some(self.rig_vec2_property(index, &name, "lattice")?);
                }
            }
        }
        self.rigs[index]
            .rig
            .deformers
            .push(rig_rt::Deformer::Lattice(rig_rt::Lattice {
                part,
                corners,
                edges,
            }));
        Ok(Value::None)
    }

    /// `r.bone("root", parent=None, at=(0, 0), rotation=0, length=40,
    /// rotation_prop="rot")`.
    pub(crate) fn rig_bone(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = [
            "name",
            "parent",
            "at",
            "rotation",
            "length",
            "rotation_prop",
            "position_prop",
        ];
        let bound = bind("bone", &params, 1, args)?;
        let name = bound.string_or(0, "")?;
        if name.is_empty() {
            return Err(HostError::value_error("bone(): 'name' cannot be empty"));
        }
        let index = self.rig_index(node)?;
        if self.rigs[index]
            .rig
            .bones
            .iter()
            .any(|bone| bone.name == *name)
        {
            return Err(HostError::value_error(format!(
                "bone(): the rig already has a bone named {name:?}"
            )));
        }
        let parent = match bound.get(1) {
            None | Some(Value::None) => None,
            Some(Value::Str(parent)) => Some(
                self.rigs[index]
                    .rig
                    .bones
                    .iter()
                    .position(|bone| bone.name == **parent)
                    .ok_or_else(|| {
                        HostError::value_error(format!("bone(): no parent bone {parent:?}"))
                    })?,
            ),
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "bone(): 'parent' must be a name or None, got {}",
                    other.type_name()
                )))
            }
        };
        let at = bound.get(2).and_then(as_vec2).unwrap_or(Vec2::ZERO);
        let rotation = bound.number_or(3, 0.0)?;
        let length = bound.number_or(4, 0.0)?;
        let rotation_property =
            self.rig_optional_property(index, &bound, 5, rig_rt::PropKind::Float, "bone")?;
        let position_property =
            self.rig_optional_property(index, &bound, 6, rig_rt::PropKind::Vec2, "bone")?;
        self.rigs[index].rig.bones.push(rig_rt::Bone {
            name: name.to_string(),
            parent,
            bind_position: at,
            bind_rotation: rotation,
            length,
            rotation_property,
            position_property,
        });
        Ok(Value::None)
    }

    /// `r.skin("body", weights=[[("root", 1.0), ...], ...])`.
    pub(crate) fn rig_skin(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("skin", &["part", "weights"], 1, args)?;
        let index = self.rig_index(node)?;
        if self.rigs[index].rig.bones.is_empty() {
            return Err(HostError::value_error("skin(): declare a bone first"));
        }
        let part = self.rig_part_index(index, &bound.string_or(0, "")?, "skin")?;
        let weights = match bound.get(1) {
            None | Some(Value::None) => None,
            Some(value) => {
                let rows = list_items(value, "skin(): 'weights' must be a list of point rows")?;
                let mut parsed = Vec::with_capacity(rows.len());
                for row in rows {
                    let pairs = list_items(&row, "skin(): every point row must be a list")?;
                    let mut points = Vec::with_capacity(pairs.len());
                    for pair in pairs {
                        let pair = list_items(&pair, "skin(): a weight is (bone, value)")?;
                        if pair.len() != 2 {
                            return Err(HostError::value_error(
                                "skin(): a weight is (bone, value)",
                            ));
                        }
                        let bone = match &pair[0] {
                            Value::Str(name) => self.rigs[index]
                                .rig
                                .bones
                                .iter()
                                .position(|bone| bone.name == **name)
                                .ok_or_else(|| {
                                    HostError::value_error(format!(
                                        "skin(): no bone named {name:?}"
                                    ))
                                })?,
                            other => {
                                return Err(HostError::type_error(format!(
                                    "skin(): bone names must be strings, got {}",
                                    other.type_name()
                                )))
                            }
                        };
                        let weight = match &pair[1] {
                            Value::Number(weight) => *weight,
                            other => {
                                return Err(HostError::type_error(format!(
                                    "skin(): weights must be numbers, got {}",
                                    other.type_name()
                                )))
                            }
                        };
                        if !weight.is_finite() || weight < 0.0 {
                            return Err(HostError::value_error(
                                "skin(): weights must be finite and non-negative",
                            ));
                        }
                        points.push((bone, weight));
                    }
                    parsed.push(points);
                }
                Some(parsed)
            }
        };
        let bones = self.rigs[index].rig.bones.clone();
        self.rigs[index]
            .rig
            .deformers
            .push(rig_rt::Deformer::Skin(rig_rt::Skin {
                part,
                bones,
                weights,
            }));
        Ok(Value::None)
    }

    /// `r.blend("body", points=[[...]], weight="smile", fit=True)`.
    pub(crate) fn rig_blend(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = ["part", "points", "weight", "name", "fit"];
        let bound = bind("blend", &params, 3, args)?;
        let index = self.rig_index(node)?;
        let part_name = bound.string_or(0, "")?;
        let part = self.rig_part_index(index, &part_name, "blend")?;
        let weight = self.rig_number_property(index, &bound.string_or(2, "")?, "blend")?;
        let fit = bool_arg(&bound, 4, true)?;
        let name = match bound.get(3) {
            Some(Value::Str(name)) => name.to_string(),
            None | Some(Value::None) => format!("blend{part}"),
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "blend(): 'name' must be a string, got {}",
                    other.type_name()
                )))
            }
        };
        let target_points = super::path_points(bound.get(1).unwrap_or(&Value::None))?;
        let part_node = self.rigs[index].part_nodes[part];
        let base = self
            .document()?
            .node(part_node)
            .and_then(|node| match node.content() {
                NodeContent::Path { geometry, .. } => Some(geometry.geometry().clone()),
                _ => None,
            })
            .ok_or_else(|| HostError::value_error("blend(): the part has no path geometry"))?;
        let points = if target_points.len() == base.points().len() {
            target_points
        } else if fit {
            let target = motioner_core::PathGeometry::new(
                target_points,
                base.closed(),
                base.cap(),
                base.join(),
            )
            .map_err(|error| core_error(&error))?;
            rig_rt::compatible_points(&target, base.points().len(), base.closed())
        } else {
            return Err(HostError::value_error(format!(
                "blend(): topology mismatch ({} points, expected {}); pass fit=True",
                target_points.len(),
                base.points().len()
            )));
        };
        let shape = rig_rt::BlendShape {
            name,
            points,
            weight,
        };
        if let Some(rig_rt::Deformer::BlendShapes(shapes)) = self.rigs[index]
            .rig
            .deformers
            .iter_mut()
            .find(|deformer| deformer.part() == part)
        {
            shapes.shapes.push(shape);
        } else {
            self.rigs[index]
                .rig
                .deformers
                .push(rig_rt::Deformer::BlendShapes(rig_rt::BlendShapes {
                    part,
                    shapes: vec![shape],
                }));
        }
        Ok(Value::None)
    }

    /// `r.ik("upper", "lower", target="hand", pole="elbow", tip=30)`.
    pub(crate) fn rig_ik(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("ik", &["upper", "lower", "target", "pole", "tip"], 3, args)?;
        let index = self.rig_index(node)?;
        let upper_name = bound.string_or(0, "")?;
        let lower_name = bound.string_or(1, "")?;
        let target = self.rig_vec2_property(index, &bound.string_or(2, "")?, "ik")?;
        let pole = match bound.get(3) {
            None | Some(Value::None) => None,
            Some(Value::Str(name)) => Some(self.rig_vec2_property(index, name, "ik")?),
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "ik(): 'pole' must be a property name or None, got {}",
                    other.type_name()
                )))
            }
        };
        let tip_length = bound.number_or(4, 30.0)?;
        if let (Some(upper), Some(lower)) = (
            upper_name.strip_prefix("bone:"),
            lower_name.strip_prefix("bone:"),
        ) {
            let upper = self.rig_bone_index(index, upper, "ik")?;
            let lower = self.rig_bone_index(index, lower, "ik")?;
            self.rigs[index]
                .rig
                .bone_constraints
                .push(rig_rt::BoneConstraint::TwoBoneIk {
                    upper,
                    lower,
                    target,
                    pole,
                    tip_length,
                });
        } else if upper_name.starts_with("bone:") || lower_name.starts_with("bone:") {
            return Err(HostError::value_error(
                "ik(): upper and lower must both be parts or both be bones",
            ));
        } else {
            let upper = self.rig_part_index(index, &upper_name, "ik")?;
            let lower = self.rig_part_index(index, &lower_name, "ik")?;
            self.rigs[index]
                .rig
                .constraints
                .push(rig_rt::Constraint::TwoBoneIk {
                    upper,
                    lower,
                    target,
                    pole,
                    tip_length,
                });
        }
        Ok(Value::None)
    }

    /// `r.follow("cursor", "path", progress="t", orient=True)`.
    pub(crate) fn rig_follow(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("follow", &["part", "path", "progress", "orient"], 3, args)?;
        let index = self.rig_index(node)?;
        let part = self.rig_part_index(index, &bound.string_or(0, "")?, "follow")?;
        let path = self.rig_part_index(index, &bound.string_or(1, "")?, "follow")?;
        let progress = self.rig_number_property(index, &bound.string_or(2, "")?, "follow")?;
        let orient = bool_arg(&bound, 3, true)?;
        self.rigs[index]
            .rig
            .constraints
            .push(rig_rt::Constraint::FollowPath {
                part,
                path,
                progress,
                orient,
            });
        Ok(Value::None)
    }

    /// `r.copy("part", "source", position=True, rotation=True, scale=True)`.
    pub(crate) fn rig_copy(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = ["part", "source", "position", "rotation", "scale"];
        let bound = bind("copy", &params, 2, args)?;
        let index = self.rig_index(node)?;
        let part_name = bound.string_or(0, "")?;
        let source_name = bound.string_or(1, "")?;
        let position = bool_arg(&bound, 2, true)?;
        let rotation = bool_arg(&bound, 3, true)?;
        let scale = bool_arg(&bound, 4, true)?;
        if let (Some(part), Some(source)) = (
            part_name.strip_prefix("bone:"),
            source_name.strip_prefix("bone:"),
        ) {
            let bone = self.rig_bone_index(index, part, "copy")?;
            let source = self.rig_bone_index(index, source, "copy")?;
            self.rigs[index]
                .rig
                .bone_constraints
                .push(rig_rt::BoneConstraint::Copy {
                    bone,
                    source,
                    position,
                    rotation,
                });
        } else if part_name.starts_with("bone:") || source_name.starts_with("bone:") {
            return Err(HostError::value_error(
                "copy(): part and source must both be parts or both be bones",
            ));
        } else {
            let part = self.rig_part_index(index, &part_name, "copy")?;
            let source = self.rig_part_index(index, &source_name, "copy")?;
            self.rigs[index]
                .rig
                .constraints
                .push(rig_rt::Constraint::Copy {
                    part,
                    source,
                    position,
                    rotation,
                    scale,
                });
        }
        Ok(Value::None)
    }

    /// `r.limit("part", min=(-5, -5), max=(5, 5), rotation=(-30, 30))`.
    pub(crate) fn rig_limit(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("limit", &["part", "min", "max", "rotation"], 1, args)?;
        let index = self.rig_index(node)?;
        let part_name = bound.string_or(0, "")?;
        if let Some(bone) = part_name.strip_prefix("bone:") {
            let bone = self.rig_bone_index(index, bone, "limit")?;
            let rotation = bound
                .get(3)
                .and_then(as_vec2)
                .map_or((-180.0, 180.0), |pair| (pair.x, pair.y));
            self.rigs[index]
                .rig
                .bone_constraints
                .push(rig_rt::BoneConstraint::Limit {
                    bone,
                    rotation_min: rotation.0,
                    rotation_max: rotation.1,
                });
            return Ok(Value::None);
        }
        let part = self.rig_part_index(index, &part_name, "limit")?;
        let min = bound
            .get(1)
            .and_then(as_vec2)
            .ok_or_else(|| HostError::value_error("limit(): 'min' must be an (x, y) pair"))?;
        let max = bound
            .get(2)
            .and_then(as_vec2)
            .ok_or_else(|| HostError::value_error("limit(): 'max' must be an (x, y) pair"))?;
        let rotation = bound
            .get(3)
            .and_then(as_vec2)
            .map_or((-180.0, 180.0), |pair| (pair.x, pair.y));
        self.rigs[index]
            .rig
            .constraints
            .push(rig_rt::Constraint::Limit {
                part,
                min,
                max,
                rotation_min: rotation.0,
                rotation_max: rotation.1,
            });
        Ok(Value::None)
    }

    /// `r.distance("part", "source", distance=40)`.
    pub(crate) fn rig_distance(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let bound = bind("distance", &["part", "source", "distance"], 3, args)?;
        let index = self.rig_index(node)?;
        let part = self.rig_part_index(index, &bound.string_or(0, "")?, "distance")?;
        let source = self.rig_part_index(index, &bound.string_or(1, "")?, "distance")?;
        self.rigs[index]
            .rig
            .constraints
            .push(rig_rt::Constraint::Distance {
                part,
                source,
                distance: bound.number_or(2, 0.0)?,
            });
        Ok(Value::None)
    }

    /// `r.spring("hair", target="look", stiffness=0.4, damping=0.2)`.
    pub(crate) fn rig_spring(&mut self, node: NodeId, args: CallArgs) -> HostResult<Value> {
        let params = ["part", "target", "stiffness", "damping", "mass"];
        let bound = bind("spring", &params, 2, args)?;
        let index = self.rig_index(node)?;
        let part = self.rig_part_index(index, &bound.string_or(0, "")?, "spring")?;
        let target = self.rig_vec2_property(index, &bound.string_or(1, "")?, "spring")?;
        let stiffness = bound.number_or(2, 0.4)?;
        let damping = bound.number_or(3, 0.2)?;
        let mass = bound.number_or(4, 1.0)?;
        self.rigs[index]
            .rig
            .constraints
            .push(rig_rt::Constraint::Spring {
                part,
                target,
                stiffness,
                damping,
                mass,
            });
        Ok(Value::None)
    }

    fn rig_part_index(&self, index: usize, name: &str, caller: &str) -> HostResult<usize> {
        self.rigs[index]
            .rig
            .part(name)
            .ok_or_else(|| HostError::value_error(format!("{caller}(): no part {name:?}")))
    }

    fn rig_bone_index(&self, index: usize, name: &str, caller: &str) -> HostResult<usize> {
        self.rigs[index]
            .rig
            .bones
            .iter()
            .position(|bone| bone.name == name)
            .ok_or_else(|| HostError::value_error(format!("{caller}(): no bone {name:?}")))
    }

    fn rig_vec2_property(&self, index: usize, name: &str, caller: &str) -> HostResult<usize> {
        let property = self.rigs[index]
            .rig
            .property(name)
            .ok_or_else(|| HostError::value_error(format!("{caller}(): no property {name:?}")))?;
        if self.rigs[index].rig.properties[property].kind != rig_rt::PropKind::Vec2 {
            return Err(HostError::value_error(format!(
                "{caller}(): property {name:?} must be a vec2"
            )));
        }
        Ok(property)
    }

    fn rig_number_property(&self, index: usize, name: &str, caller: &str) -> HostResult<usize> {
        let property = self.rigs[index]
            .rig
            .property(name)
            .ok_or_else(|| HostError::value_error(format!("{caller}(): no property {name:?}")))?;
        if !matches!(
            self.rigs[index].rig.properties[property].kind,
            rig_rt::PropKind::Float | rig_rt::PropKind::Int
        ) {
            return Err(HostError::value_error(format!(
                "{caller}(): property {name:?} must be a number"
            )));
        }
        Ok(property)
    }

    fn rig_optional_property(
        &self,
        index: usize,
        bound: &Bound<'_>,
        at: usize,
        kind: rig_rt::PropKind,
        caller: &str,
    ) -> HostResult<Option<usize>> {
        match bound.get(at) {
            None | Some(Value::None) => Ok(None),
            Some(Value::Str(name)) => {
                let property = self.rigs[index].rig.property(name).ok_or_else(|| {
                    HostError::value_error(format!("{caller}(): no property {name:?}"))
                })?;
                if self.rigs[index].rig.properties[property].kind != kind {
                    return Err(HostError::value_error(format!(
                        "{caller}(): property {name:?} has the wrong type"
                    )));
                }
                Ok(Some(property))
            }
            Some(other) => Err(HostError::type_error(format!(
                "{caller}(): expected a property name or None, got {}",
                other.type_name()
            ))),
        }
    }

    fn rig_property_list(
        &self,
        index: usize,
        bound: &Bound<'_>,
        at: usize,
        caller: &str,
    ) -> HostResult<Vec<usize>> {
        let items = list_items(
            bound.get(at).unwrap_or(&Value::None),
            &format!("{caller}(): expected a list of property names"),
        )?;
        items
            .iter()
            .map(|item| match item {
                Value::Str(name) => self.rig_vec2_property(index, name, caller),
                other => Err(HostError::type_error(format!(
                    "{caller}(): expected a property name, got {}",
                    other.type_name()
                ))),
            })
            .collect()
    }

    ///
    /// The order is the authoring order (rigs, properties, poses, states and
    /// transitions), so two compiles of the same program produce equal
    /// manifests. Missing indices are skipped instead of panicking: the
    /// manifest is diagnostic data and must never break a load.
    #[allow(clippy::too_many_lines)] // one straight map per manifest section
    fn build_rig_manifest(&self) -> RigManifest {
        let mut manifest = RigManifest::default();
        let Some(document) = self.document.as_ref() else {
            return manifest;
        };
        for entry in &self.rigs {
            let rig = &entry.rig;
            let root = document.node(entry.node);
            let id = root
                .map(|node| node.identity().as_str().to_owned())
                .unwrap_or_default();
            let name = root
                .and_then(|node| node.name().map(str::to_owned))
                .unwrap_or_else(|| rig.name.clone());
            let parts = rig
                .parts
                .iter()
                .zip(entry.part_nodes.iter())
                .map(|(part, node)| RigPartManifest {
                    id: part.name.clone(),
                    node: document
                        .node(*node)
                        .map(|node| node.identity().as_str().to_owned())
                        .unwrap_or_default(),
                })
                .collect();
            let properties = rig
                .properties
                .iter()
                .enumerate()
                .map(|(index, property)| {
                    let mirror = entry.mirrors.iter().find_map(|mirror| {
                        if mirror.left == index {
                            Some(RigMirrorManifest {
                                group: mirror.name.clone(),
                                side: RigMirrorSide::Left,
                                flip: false,
                            })
                        } else if mirror.right == index {
                            Some(RigMirrorManifest {
                                group: mirror.name.clone(),
                                side: RigMirrorSide::Right,
                                flip: mirror.flip,
                            })
                        } else {
                            None
                        }
                    });
                    RigPropertyManifest {
                        name: property.name.clone(),
                        kind: manifest_kind(property.kind),
                        angle: property.angle,
                        default: manifest_value(&property.default),
                        min: property.min,
                        max: property.max,
                        step: property.step,
                        values: property.values.clone(),
                        group: mirror.as_ref().map(|mirror| mirror.group.clone()),
                        mirror,
                        keys: {
                            let mut keys: Vec<RigPropertyKeyManifest> = rig.channels[index]
                                .keys
                                .iter()
                                .map(|key| RigPropertyKeyManifest {
                                    frame: key.frame,
                                    value: manifest_value(&key.value),
                                    ease: key.ease.name().to_owned(),
                                })
                                .collect();
                            keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                            keys
                        },
                    }
                })
                .collect();
            let poses = rig
                .poses
                .iter()
                .map(|pose| RigPoseManifest {
                    name: pose.name.clone(),
                    reset: pose.reset,
                    values: pose
                        .values
                        .iter()
                        .filter_map(|(index, value)| {
                            rig.properties.get(*index).map(|property| RigPropertyValue {
                                property: property.name.clone(),
                                value: manifest_value(value),
                            })
                        })
                        .collect(),
                })
                .collect();
            let pose_tracks = rig
                .tracks
                .iter()
                .filter_map(|track| {
                    rig.poses.get(track.pose).map(|pose| RigPoseTrackManifest {
                        pose: pose.name.clone(),
                        start: track.start,
                        duration: track.duration,
                        ease: track.ease.name().to_owned(),
                    })
                })
                .collect();
            let clips = rig
                .clips
                .iter()
                .map(|clip| RigClipManifest {
                    name: clip.name.clone(),
                    length: clip.length(),
                    keys: clip
                        .keys
                        .iter()
                        .filter_map(|key| {
                            rig.properties
                                .get(key.property)
                                .map(|property| RigClipKeyManifest {
                                    property: property.name.clone(),
                                    frame: key.frame,
                                    value: manifest_value(&key.value),
                                    ease: key.ease.name().to_owned(),
                                })
                        })
                        .collect(),
                })
                .collect();
            let plays = rig
                .plays
                .iter()
                .filter_map(|play| {
                    rig.clips.get(play.clip).map(|clip| RigPlayManifest {
                        clip: clip.name.clone(),
                        start: play.start,
                        loops: play.loops,
                        speed: play.speed,
                    })
                })
                .collect();
            let layers = rig
                .blinks
                .iter()
                .map(|blink| RigLayerManifest {
                    kind: "blink".to_owned(),
                    start: blink.start,
                    period: blink.period,
                    jitter: blink.jitter,
                    duration: blink.duration,
                    seed: blink.seed,
                    values: blink
                        .props
                        .iter()
                        .filter_map(|(index, value)| {
                            rig.properties.get(*index).map(|property| RigPropertyValue {
                                property: property.name.clone(),
                                value: manifest_value(value),
                            })
                        })
                        .collect(),
                })
                .collect();
            let aims = rig
                .aims
                .iter()
                .filter_map(|aim| {
                    let part = rig.parts.get(aim.part)?;
                    let property = rig.properties.get(aim.property)?;
                    Some(RigAimManifest {
                        part: part.name.clone(),
                        property: property.name.clone(),
                        radius: aim.radius,
                    })
                })
                .collect();
            let machine = rig.machine.as_ref().map(|machine| {
                let state_name = |index: usize| {
                    machine
                        .states
                        .get(index)
                        .map(|state| state.name.clone())
                        .unwrap_or_default()
                };
                let mut input_indices = Vec::new();
                for transition in &machine.transitions {
                    if !input_indices.contains(&transition.input) {
                        input_indices.push(transition.input);
                    }
                }
                let inputs = input_indices
                    .into_iter()
                    .filter_map(|index| {
                        rig.properties.get(index).map(|property| RigInputManifest {
                            name: property.name.clone(),
                            kind: manifest_kind(property.kind),
                            values: property.values.clone(),
                        })
                    })
                    .collect();
                let states = machine
                    .states
                    .iter()
                    .map(|state| RigStateManifest {
                        name: state.name.clone(),
                        pose: state
                            .pose
                            .and_then(|index| rig.poses.get(index))
                            .map(|pose| pose.name.clone()),
                        clip: state
                            .clip
                            .and_then(|index| rig.clips.get(index))
                            .map(|clip| clip.name.clone()),
                        speed: state.speed,
                        looping: state.looping,
                    })
                    .collect();
                let transitions = machine
                    .transitions
                    .iter()
                    .filter_map(|transition| {
                        let property = rig.properties.get(transition.input)?;
                        Some(RigTransitionManifest {
                            from: transition.from.map(state_name),
                            to: state_name(transition.to),
                            input: property.name.clone(),
                            compare: compare_name(transition.compare).to_owned(),
                            value: manifest_value(&transition.value),
                            duration: transition.duration,
                        })
                    })
                    .collect();
                let events = machine
                    .transitions
                    .iter()
                    .map(|transition| RigEventManifest {
                        from: transition.from.map(state_name),
                        to: state_name(transition.to),
                    })
                    .collect();
                RigMachineManifest {
                    default: state_name(machine.default),
                    inputs,
                    states,
                    transitions,
                    events,
                }
            });
            manifest.rigs.push(RigInstanceManifest {
                id,
                name,
                parts,
                properties,
                poses,
                pose_tracks,
                clips,
                plays,
                layers,
                aims,
                machine,
            });
        }
        manifest
    }

    /// Evaluates every rig and writes the result as node keys (D-0052).
    #[allow(clippy::too_many_lines)] // one pass per rig over every composition frame
    pub(crate) fn bake_rigs(&mut self) -> HostResult<()> {
        if self.rigs.is_empty() {
            return Ok(());
        }
        let last = self.document()?.composition().duration();
        for index in 0..self.rigs.len() {
            let frames = {
                let entry = &self.rigs[index];
                #[allow(clippy::cast_precision_loss)] // composition frames are bounded
                let last = last as f64;
                rig_rt::bake(&entry.rig, 0.0, last)
                    .map_err(|error| HostError::value_error(error.to_string()))?
            };
            let entry = &self.rigs[index];
            let mut written: Vec<Written> = vec![Written::default(); entry.rig.parts.len()];
            for frame in &frames {
                for (part, output) in frame.parts.iter().enumerate() {
                    if !output.touched {
                        continue;
                    }
                    let base = &entry.rig.parts[part];
                    let position = base.position + output.position;
                    let rotation = base.rotation + output.rotation;
                    let scale =
                        Vec2::new(base.scale.x * output.scale.x, base.scale.y * output.scale.y);
                    let node = entry.part_nodes[part];
                    let state = &mut written[part];
                    let document = self.document.as_mut().expect("checked above");
                    write_channel(
                        document,
                        node,
                        Property::Position,
                        motioner_core::Axis::X,
                        frame.frame,
                        position.x,
                        &mut state.x,
                    )?;
                    write_channel(
                        document,
                        node,
                        Property::Position,
                        motioner_core::Axis::Y,
                        frame.frame,
                        position.y,
                        &mut state.y,
                    )?;
                    write_scalar(
                        document,
                        node,
                        Property::Rotation,
                        frame.frame,
                        rotation,
                        &mut state.rotation,
                    )?;
                    write_channel(
                        document,
                        node,
                        Property::Scale,
                        motioner_core::Axis::X,
                        frame.frame,
                        scale.x,
                        &mut state.scale_x,
                    )?;
                    write_channel(
                        document,
                        node,
                        Property::Scale,
                        motioner_core::Axis::Y,
                        frame.frame,
                        scale.y,
                        &mut state.scale_y,
                    )?;
                    // Always write the first observation, so the evaluator's hold-before-first-key rule cannot show a later value early.
                    write_scalar(
                        document,
                        node,
                        Property::Opacity,
                        frame.frame,
                        output.opacity,
                        &mut state.opacity,
                    )?;
                    if let Some(width) = output.stroke_width {
                        write_content_scalar(
                            document,
                            node,
                            ContentField::StrokeWidth,
                            frame.frame,
                            width,
                            &mut state.stroke_width,
                        )?;
                    }
                    if let Some(geometry) = &output.geometry {
                        write_content_geometry(
                            document,
                            node,
                            frame.frame,
                            geometry,
                            &mut state.geometry,
                        )?;
                    }
                }
            }
        }
        let manifest = self.build_rig_manifest();
        if let Some(document) = self.document.as_mut() {
            document.set_rig_manifest(manifest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
struct Written {
    x: ChannelWrite,
    y: ChannelWrite,
    rotation: ChannelWrite,
    scale_x: ChannelWrite,
    scale_y: ChannelWrite,
    opacity: ChannelWrite,
    stroke_width: ChannelWrite,
    geometry: GeometryWrite,
}

#[derive(Debug, Clone, Default)]
struct GeometryWrite {
    points: Option<Vec<motioner_core::PathPoint>>,
    frame: Option<motioner_core::Frame>,
}

/// The last key written for one channel, so a gap gets a hold key.
#[derive(Debug, Clone, Copy, Default)]
struct ChannelWrite {
    value: Option<f64>,
    frame: Option<motioner_core::Frame>,
}

fn write_channel(
    document: &mut motioner_core::Document,
    node: NodeId,
    property: Property,
    axis: motioner_core::Axis,
    frame: motioner_core::Frame,
    value: f64,
    last: &mut ChannelWrite,
) -> HostResult<()> {
    let Some(previous) = last.value else {
        document
            .add_axis_keyframe(node, property, axis, frame, value, Ease::Linear)
            .map_err(|error| core_error(&error))?;
        *last = ChannelWrite {
            value: Some(value),
            frame: Some(frame),
        };
        return Ok(());
    };
    if (value - previous).abs() <= 1e-9 {
        return Ok(());
    }
    // Hold the previous value up to the frame before the change, so a
    // per-frame sample never sees an interpolation across a rest gap.
    if last.frame.is_some_and(|last| frame > last + 1) {
        document
            .add_axis_keyframe(node, property, axis, frame - 1, previous, Ease::Linear)
            .map_err(|error| core_error(&error))?;
    }
    document
        .add_axis_keyframe(node, property, axis, frame, value, Ease::Linear)
        .map_err(|error| core_error(&error))?;
    *last = ChannelWrite {
        value: Some(value),
        frame: Some(frame),
    };
    Ok(())
}

fn write_scalar(
    document: &mut motioner_core::Document,
    node: NodeId,
    property: Property,
    frame: motioner_core::Frame,
    value: f64,
    last: &mut ChannelWrite,
) -> HostResult<()> {
    let Some(previous) = last.value else {
        document
            .add_keyframe(
                node,
                property,
                frame,
                PropertyValue::Scalar(value),
                Ease::Linear,
            )
            .map_err(|error| core_error(&error))?;
        *last = ChannelWrite {
            value: Some(value),
            frame: Some(frame),
        };
        return Ok(());
    };
    if (value - previous).abs() <= 1e-9 {
        return Ok(());
    }
    if last.frame.is_some_and(|last| frame > last + 1) {
        document
            .add_keyframe(
                node,
                property,
                frame - 1,
                PropertyValue::Scalar(previous),
                Ease::Linear,
            )
            .map_err(|error| core_error(&error))?;
    }
    document
        .add_keyframe(
            node,
            property,
            frame,
            PropertyValue::Scalar(value),
            Ease::Linear,
        )
        .map_err(|error| core_error(&error))?;
    *last = ChannelWrite {
        value: Some(value),
        frame: Some(frame),
    };
    Ok(())
}

fn write_content_scalar(
    document: &mut motioner_core::Document,
    node: NodeId,
    field: ContentField,
    frame: motioner_core::Frame,
    value: f64,
    last: &mut ChannelWrite,
) -> HostResult<()> {
    if last.value.is_some_and(|last| (value - last).abs() <= 1e-9) {
        return Ok(());
    }
    document
        .add_content_keyframe(
            node,
            field,
            frame,
            ContentValue::Number(value),
            Ease::Linear,
        )
        .map_err(|error| core_error(&error))?;
    *last = ChannelWrite {
        value: Some(value),
        frame: Some(frame),
    };
    Ok(())
}

fn write_content_geometry(
    document: &mut motioner_core::Document,
    node: NodeId,
    frame: motioner_core::Frame,
    geometry: &motioner_core::PathGeometry,
    last: &mut GeometryWrite,
) -> HostResult<()> {
    let points = geometry.points().to_vec();
    if last.points.as_ref().is_some_and(|last| *last == points) {
        return Ok(());
    }
    if let (Some(previous), Some(previous_frame)) = (&last.points, last.frame) {
        if frame > previous_frame + 1 {
            document
                .add_content_keyframe(
                    node,
                    ContentField::Geometry,
                    previous_frame + 1,
                    ContentValue::GeometryPoints(previous.clone()),
                    Ease::Linear,
                )
                .map_err(|error| core_error(&error))?;
        }
    }
    document
        .add_content_keyframe(
            node,
            ContentField::Geometry,
            frame,
            ContentValue::GeometryPoints(points.clone()),
            Ease::Linear,
        )
        .map_err(|error| core_error(&error))?;
    *last = GeometryWrite {
        points: Some(points),
        frame: Some(frame),
    };
    Ok(())
}

fn insert_key(channel: &mut rig_rt::Channel, frame: f64, value: rig_rt::PropValue, ease: Ease) {
    let key = rig_rt::Key::new(frame, value, ease);
    match channel
        .keys
        .binary_search_by(|existing| existing.frame.total_cmp(&frame))
    {
        Ok(position) => channel.keys[position] = key,
        Err(position) => channel.keys.insert(position, key),
    }
}

fn parse_range(value: Option<&Value>) -> HostResult<(Option<f64>, Option<f64>)> {
    let Some(value) = value else {
        return Ok((None, None));
    };
    if matches!(value, Value::None) {
        return Ok((None, None));
    }
    let items = list_items(value, "range must be a (min, max) pair")?;
    if items.len() != 2 {
        return Err(HostError::value_error("range must be a (min, max) pair"));
    }
    // A vec2 range is a pair of pairs; the inspector uses it per axis, but
    // the numeric bounds here only describe scalar properties.
    match (&items[0], &items[1]) {
        (Value::Number(min), Value::Number(max)) => Ok((Some(*min), Some(*max))),
        _ => Ok((None, None)),
    }
}

#[allow(clippy::too_many_lines)] // scalar and geometry writes share one pass
fn compile_driver(rig: &rig_rt::Rig, function: &FunctionDef) -> HostResult<rig_rt::Driver> {
    let name = function.name.to_string();
    let mut params = Vec::new();
    for param in &function.params {
        if param.default.is_some() {
            return Err(HostError::value_error(format!(
                "driver '{name}': parameters cannot have defaults; pass every property explicitly"
            )));
        }
        let property = rig.property(&param.name).ok_or_else(|| {
            HostError::value_error(format!(
                "driver '{name}': parameter {:?} is not a rig property",
                param.name
            ))
        })?;
        params.push(property);
    }
    let [Stmt {
        kind: StmtKind::Return(Some(expression)),
        ..
    }] = function.body.as_slice()
    else {
        return Err(HostError::value_error(format!(
            "driver '{name}': the body must be exactly `return [ ... ]`"
        )));
    };
    let writes = match &expression.kind {
        ExprKind::List(items) | ExprKind::Tuple(items) => items.clone(),
        _ => {
            return Err(HostError::value_error(format!(
                "driver '{name}': return a list of [part, field, value] entries"
            )))
        }
    };
    if writes.len() > rig_rt::driver_max_writes() {
        return Err(HostError::limit(format!(
            "driver '{name}': at most {} writes are allowed",
            rig_rt::driver_max_writes()
        )));
    }
    let mut out = Vec::new();
    let mut geometries = Vec::new();
    for entry in &writes {
        let parts = match &entry.kind {
            ExprKind::List(items) | ExprKind::Tuple(items) => items.clone(),
            _ => {
                return Err(HostError::value_error(format!(
                    "driver '{name}': every write must be [part, field, value]"
                )))
            }
        };
        if parts.len() != 3 {
            return Err(HostError::value_error(format!(
                "driver '{name}': every write must have exactly three items"
            )));
        }
        let part_name = match &parts[0].kind {
            ExprKind::Str(text) => text.to_string(),
            _ => {
                return Err(HostError::value_error(format!(
                    "driver '{name}': the part name must be a string literal"
                )))
            }
        };
        let part = rig.part(&part_name).ok_or_else(|| {
            HostError::value_error(format!(
                "driver '{name}': the rig has no part {part_name:?}"
            ))
        })?;
        let field_name = match &parts[1].kind {
            ExprKind::Str(text) => text.to_string(),
            _ => {
                return Err(HostError::value_error(format!(
                    "driver '{name}': the field must be a string literal"
                )))
            }
        };
        if field_name == "geometry" {
            let geometry = compile_geometry_expr(rig, &name, &parts[2])?;
            geometries.push(rig_rt::GeometryWrite { part, geometry });
            continue;
        }
        let field = rig_rt::Field::from_name(&field_name).ok_or_else(|| {
            HostError::value_error(format!(
                "driver '{name}': unknown field {field_name:?}; use geometry or {}",
                rig_rt::Field::NAMES.join(", ")
            ))
        })?;
        let value = compile_expr(rig, &name, &parts[2])?;
        out.push(rig_rt::Write { part, field, value });
    }
    if out.len() + geometries.len() > rig_rt::driver_max_writes() {
        return Err(HostError::limit(format!(
            "driver '{name}': at most {} writes are allowed",
            rig_rt::driver_max_writes()
        )));
    }
    Ok(rig_rt::Driver {
        name,
        params,
        writes: out,
        geometries,
    })
}

#[allow(clippy::too_many_lines)] // one constructor per arc/path form
fn compile_geometry_expr(
    rig: &rig_rt::Rig,
    driver: &str,
    expression: &AstExpr,
) -> HostResult<rig_rt::GeometryExpr> {
    let unsupported = || {
        HostError::value_error(format!(
            "driver '{driver}': a geometry value must be path_builder(...), ellipse_arc(...), \
             arc(...), rounded_rect(...) or mirror(...)"
        ))
    };
    let ExprKind::Call { callee, args } = &expression.kind else {
        return Err(unsupported());
    };
    let ExprKind::Name(name) = &callee.kind else {
        return Err(unsupported());
    };
    let positional: Vec<&AstExpr> = args
        .iter()
        .filter(|argument| argument.name.is_none())
        .map(|argument| &argument.value)
        .collect();
    let scalar = |index: usize| compile_expr(rig, driver, positional[index]);
    Ok(match name.as_ref() {
        "path_builder" => {
            if positional.len() > rig_rt::MAX_GEOMETRY_COMMANDS {
                return Err(HostError::limit(format!(
                    "driver '{driver}': path_builder has more than {} commands",
                    rig_rt::MAX_GEOMETRY_COMMANDS
                )));
            }
            let commands = positional
                .iter()
                .map(|command| compile_geometry_command(rig, driver, command))
                .collect::<HostResult<Vec<_>>>()?;
            rig_rt::GeometryExpr::Builder(commands)
        }
        "ellipse_arc" if positional.len() == 6 || positional.len() == 7 => {
            let segments = args
                .iter()
                .find(|argument| argument.name.as_deref() == Some("segments"))
                .map(|argument| &argument.value)
                .or_else(|| positional.get(6).copied());
            rig_rt::GeometryExpr::EllipseArc {
                cx: scalar(0)?,
                cy: scalar(1)?,
                rx: scalar(2)?,
                ry: scalar(3)?,
                start: scalar(4)?,
                end: scalar(5)?,
                segments: match segments {
                    Some(expression) => compile_expr(rig, driver, expression)?,
                    None => rig_rt::Expr::Number(f64::from(rig_rt::DEFAULT_ARC_SEGMENTS)),
                },
            }
        }
        "arc" if positional.len() == 3 || positional.len() == 4 => {
            let segments = args
                .iter()
                .find(|argument| argument.name.as_deref() == Some("segments"))
                .map(|argument| &argument.value)
                .or_else(|| positional.get(3).copied());
            rig_rt::GeometryExpr::Arc {
                radius: scalar(0)?,
                start: scalar(1)?,
                end: scalar(2)?,
                segments: match segments {
                    Some(expression) => compile_expr(rig, driver, expression)?,
                    None => rig_rt::Expr::Number(f64::from(rig_rt::DEFAULT_ARC_SEGMENTS)),
                },
            }
        }
        "rounded_rect" if positional.len() == 3 => rig_rt::GeometryExpr::RoundedRect {
            width: scalar(0)?,
            height: scalar(1)?,
            radius: scalar(2)?,
        },
        "mirror" if !positional.is_empty() => {
            let geometry = Box::new(compile_geometry_expr(rig, driver, positional[0])?);
            let axis = args
                .iter()
                .find(|argument| argument.name.as_deref() == Some("axis"))
                .or_else(|| {
                    args.iter()
                        .filter(|argument| argument.name.is_none())
                        .nth(1)
                })
                .map(|argument| &argument.value);
            let axis = match axis.map(|value| &value.kind) {
                None => rig_rt::MirrorAxis::X,
                Some(ExprKind::Str(text)) => {
                    rig_rt::MirrorAxis::from_name(text).ok_or_else(|| {
                        HostError::value_error(format!(
                            "driver '{driver}': mirror axis must be \"x\" or \"y\", got {text:?}"
                        ))
                    })?
                }
                Some(_) => {
                    return Err(HostError::value_error(format!(
                        "driver '{driver}': mirror axis must be \"x\" or \"y\""
                    )))
                }
            };
            rig_rt::GeometryExpr::Mirror { geometry, axis }
        }
        _ => return Err(unsupported()),
    })
}

fn compile_geometry_command(
    rig: &rig_rt::Rig,
    driver: &str,
    expression: &AstExpr,
) -> HostResult<rig_rt::GeometryCommand> {
    let ExprKind::Call { callee, args } = &expression.kind else {
        return Err(HostError::value_error(format!(
            "driver '{driver}': path_builder arguments must be move_to, line_to, cubic_to or close"
        )));
    };
    let ExprKind::Name(name) = &callee.kind else {
        return Err(HostError::value_error(format!(
            "driver '{driver}': path_builder arguments must be move_to, line_to, cubic_to or close"
        )));
    };
    let positional: Vec<&AstExpr> = args
        .iter()
        .filter(|argument| argument.name.is_none())
        .map(|argument| &argument.value)
        .collect();
    let scalar = |index: usize| compile_expr(rig, driver, positional[index]);
    Ok(match (name.as_ref(), positional.len()) {
        ("move_to", 2) => rig_rt::GeometryCommand::MoveTo {
            x: scalar(0)?,
            y: scalar(1)?,
        },
        ("line_to", 2) => rig_rt::GeometryCommand::LineTo {
            x: scalar(0)?,
            y: scalar(1)?,
        },
        ("cubic_to", 6) => rig_rt::GeometryCommand::CubicTo {
            c1x: scalar(0)?,
            c1y: scalar(1)?,
            c2x: scalar(2)?,
            c2y: scalar(3)?,
            x: scalar(4)?,
            y: scalar(5)?,
        },
        ("arc_to", 7 | 8) => {
            let segments = args
                .iter()
                .find(|argument| argument.name.as_deref() == Some("segments"))
                .map(|argument| &argument.value)
                .or_else(|| positional.get(7).copied());
            rig_rt::GeometryCommand::ArcTo {
                rx: scalar(0)?,
                ry: scalar(1)?,
                rotation: scalar(2)?,
                large_arc: scalar(3)?,
                sweep: scalar(4)?,
                x: scalar(5)?,
                y: scalar(6)?,
                segments: match segments {
                    Some(expression) => compile_expr(rig, driver, expression)?,
                    None => rig_rt::Expr::Number(f64::from(rig_rt::DEFAULT_ARC_SEGMENTS)),
                },
            }
        }
        ("close", 0) => rig_rt::GeometryCommand::Close,
        _ => {
            return Err(HostError::value_error(format!(
                "driver '{driver}': unknown path_builder command; use move_to(x, y), \
                 line_to(x, y), cubic_to(c1x, c1y, c2x, c2y, x, y), \
                 arc_to(rx, ry, rotation, large_arc, sweep, x, y) or close()"
            )))
        }
    })
}

fn compile_expr(rig: &rig_rt::Rig, driver: &str, expression: &AstExpr) -> HostResult<rig_rt::Expr> {
    use rig_rt::Expr as E;
    let compiled = |expression: &AstExpr| compile_expr(rig, driver, expression);
    let unsupported = || {
        HostError::value_error(format!(
            "driver '{driver}': unsupported expression; drivers may use numbers, property names, \
             + - * /, comparisons, and, or, not, min, max, abs, sqrt, sin, cos, clamp and lerp"
        ))
    };
    Ok(match &expression.kind {
        ExprKind::Number(value) => E::Number(*value),
        ExprKind::Name(name) => {
            let property = rig.property(name).ok_or_else(|| {
                HostError::value_error(format!("driver '{driver}': {name:?} is not a rig property"))
            })?;
            E::Property(property)
        }
        ExprKind::Unary {
            op: UnaryOp::Neg,
            operand,
        } => E::Neg(Box::new(compiled(operand)?)),
        ExprKind::Binary { op, left, right } => {
            let left = Box::new(compiled(left)?);
            let right = Box::new(compiled(right)?);
            match op {
                BinaryOp::Add => E::Add(left, right),
                BinaryOp::Sub => E::Sub(left, right),
                BinaryOp::Mul => E::Mul(left, right),
                BinaryOp::Div => E::Div(left, right),
                _ => return Err(unsupported()),
            }
        }
        ExprKind::Compare { op, left, right } => {
            let left = Box::new(compiled(left)?);
            let right = Box::new(compiled(right)?);
            match op {
                CompareOp::Lt => E::Less(left, right),
                CompareOp::Gt => E::Greater(left, right),
                _ => return Err(unsupported()),
            }
        }
        ExprKind::Logical { op, left, right } => {
            let left = Box::new(compiled(left)?);
            let right = Box::new(compiled(right)?);
            match op {
                LogicalOp::And => E::And(left, right),
                LogicalOp::Or => E::Or(left, right),
            }
        }
        ExprKind::Not(operand) => E::Not(Box::new(compiled(operand)?)),
        ExprKind::Call { callee, args } => {
            let ExprKind::Name(name) = &callee.kind else {
                return Err(unsupported());
            };
            let values = args
                .iter()
                .map(|argument| compiled(&argument.value))
                .collect::<HostResult<Vec<_>>>()?;
            let value = |index: usize| Box::new(values[index].clone());
            match (name.as_ref(), values.len()) {
                ("min", 2) => E::Min(value(0), value(1)),
                ("max", 2) => E::Max(value(0), value(1)),
                ("abs", 1) => E::Abs(value(0)),
                ("sin", 1) => E::Sin(value(0)),
                ("cos", 1) => E::Cos(value(0)),
                ("sqrt", 1) => E::Sqrt(value(0)),
                ("clamp", 3) => E::Clamp(value(0), value(1), value(2)),
                ("lerp", 3) => E::Lerp(value(0), value(1), value(2)),
                _ => return Err(unsupported()),
            }
        }
        _ => return Err(unsupported()),
    })
}
