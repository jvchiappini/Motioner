//! The 2D scene API: `comp`, node constructors, node properties and the
//! `key` / `add` methods (spec section 7, v0 subset).
//!
//! This is the only place where MDF values become document mutations, and
//! every mutation goes through the `motioner_core::Document` API.

use std::fmt::Write as _;
use std::sync::Arc;

use motioner_core::{
    AudioContent, BlendMode, CharOrder, CharProperty, CharValue, Color, Composition,
    ContentChannel, ContentField, ContentValue, CoreError, Document, Ease, Effect, EffectChannel,
    EffectKind, EffectParam, EffectValueInput, FrameRate, Gradient, ImageContent, ImageFit,
    LineCap, LineJoin, MaskMode, NodeContent, NodeId, NodeIdentity, Paint, PathGeometry, PathPoint,
    Property, PropertyValue, Stroke, TextAlign, TextAnimator, TextContent, Vec2, VideoContent,
    MAX_CANVAS_SIZE,
};

use crate::diagnostic::{ErrorCategory, Span};
use crate::host::{
    as_integer, as_vec2, bind, vec2_value, Bound, CallArgs, CallValue, HostError, HostResult,
    PaintInput,
};
use crate::origins::{
    BindingOrigin, BlockOrigin, EaseOrigin, Literal, NameId, NodeOrigin, OriginKind, OriginSite,
    ValueOrigin, ValueOrigins, ValueSource,
};
use crate::theme::{self, Theme};
use crate::value::{Builtin, NodeMethod, Value};
use motioner_core::{TextBounds, TextMetrics, TextRequest};
use std::collections::HashMap;

/// NTSC-style frame rates written as decimals, mapped to exact rationals.
const NTSC_RATES: [(f64, u32); 3] = [(23.976, 24_000), (29.97, 30_000), (59.94, 60_000)];

/// Highest integer frame rate accepted by `comp()`.
const MAX_FPS: i64 = 1000;

/// Where a call that creates nodes came from (D-0022 provenance).
///
/// The key is `@<call start>` followed by `/c<call-site start>` for every user
/// function frame (outermost first) and `/i<index>` for every enclosing loop.
/// The same program always produces the same sites.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CreationSite {
    /// Span of the constructor call.
    pub creation: Span,
    /// Span of the statement that contains the call.
    pub statement: Span,
    /// The block that contains the statement.
    pub block: BlockOrigin,
    /// Loop and call context of the call.
    pub site: Arc<OriginSite>,
}

impl CreationSite {
    /// The stable provenance key this site renders as.
    pub fn key(&self) -> String {
        let mut key = format!("@{}", self.creation.start);
        for call in &self.site.calls {
            let _ = write!(key, "/c{}", call.call_site.start);
        }
        for loop_frame in &self.site.loops {
            let _ = write!(key, "/i{}", loop_frame.index);
        }
        key
    }
}

/// A property or key write with the statement and context that produced it.
#[derive(Debug, Clone)]
pub(crate) struct WriteSite {
    /// Span of the statement that contains the write.
    pub statement: Span,
    /// The block that contains the statement.
    pub block: BlockOrigin,
    /// Loop and call context of the write.
    pub site: Arc<OriginSite>,
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
    /// The active design-token set (D-0039).
    theme: &'static Theme,
    /// The tempo in beats per minute (`beat()`/`bar()`).
    tempo: f64,
    /// Whether `comp(tempo=...)` pinned the tempo before a theme changed it.
    tempo_explicit: bool,
    /// Scene windows by group, for `scene.t(offset)`.
    scenes: HashMap<NodeId, SceneWindow>,
}

/// The absolute window of a `scene(...)` group, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SceneWindow {
    /// Absolute start time in seconds.
    pub start: f64,
    /// Absolute end time in seconds.
    pub end: f64,
}

impl Scene {
    pub fn new(max_nodes: usize) -> Self {
        let theme = theme::default_theme();
        Self {
            document: None,
            max_nodes,
            origins: ValueOrigins::new(),
            theme,
            tempo: theme.tempo,
            tempo_explicit: false,
            scenes: HashMap::new(),
        }
    }

    /// The value origins recorded so far.
    pub fn origins(&self) -> &ValueOrigins {
        &self.origins
    }

    /// Interns `name` in the origin table, returning its stable id.
    pub fn intern_name(&mut self, name: &str) -> NameId {
        self.origins.intern_name(name)
    }

    /// The finished document, if `comp()` was called.
    pub fn into_document(self) -> Option<Document> {
        self.document
    }

    /// Records where a node property value came from, when the source is
    /// known (an argument that was actually written in the program).
    fn record(
        &mut self,
        node: NodeId,
        property: &str,
        source: Option<ValueSource>,
        write: &WriteSite,
    ) {
        if let Some(source) = source {
            self.origins.record(ValueOrigin {
                node,
                property: property.to_owned(),
                span: source.span,
                literal: source.literal,
                kind: OriginKind::Set,
                ease: None,
                expression: source.expression,
                statement: write.statement,
                block: write.block,
                site: Arc::clone(&write.site),
            });
        }
    }

    /// Binds `node` to a variable in the caller's block.
    pub fn attach_binding(&mut self, node: NodeId, binding: BindingOrigin) {
        self.origins.attach_binding(node, binding);
    }

    /// Calls a scene constructor builtin.
    #[allow(clippy::too_many_lines)] // one arm per builtin, kept as one dispatch table
    pub fn call(
        &mut self,
        builtin: Builtin,
        args: CallArgs,
        site: &CreationSite,
        metrics: &dyn TextMetrics,
    ) -> HostResult<Value> {
        match builtin {
            Builtin::Comp => self.comp(args),
            Builtin::Theme => Self::theme(args),
            Builtin::UseTheme => self.use_theme(args),
            Builtin::Token => self.token(args),
            Builtin::Beat => self.beat(args, false),
            Builtin::Bar => self.beat(args, true),
            Builtin::Scene => self.scene(args, site),
            Builtin::Row => self.axis_layout(args, metrics, Axis2::Row),
            Builtin::Column => self.axis_layout(args, metrics, Axis2::Column),
            Builtin::Grid => self.grid_layout(args, metrics),
            Builtin::Stack => self.stack_layout(args, metrics),
            Builtin::CompSize => self.comp_size(args),
            Builtin::Stagger => self.stagger(args, site),
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
                let paint =
                    self.paint_args(&bound, 3, Some(PaintInput::Color(Color::WHITE)), None, 0.0)?;
                let content = NodeContent::rect(
                    bound.number(0)?,
                    bound.number(1)?,
                    self.number_arg(bound.get(2), 0.0)?,
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
                        self.record(
                            id,
                            property,
                            bound.argument(index).map(value_source),
                            &WriteSite {
                                statement: site.statement,
                                block: site.block,
                                site: Arc::clone(&site.site),
                            },
                        );
                    }
                }
                Ok(node)
            }
            Builtin::Circle => {
                let params = ["radius", "fill", "stroke", "stroke_width"];
                let bound = bind("circle", &params, 1, args)?;
                let paint =
                    self.paint_args(&bound, 1, Some(PaintInput::Color(Color::WHITE)), None, 0.0)?;
                let content = NodeContent::circle(self.number_arg(bound.get(0), 0.0)?, paint.paint);
                let node = self.create(content, site)?;
                if let Value::Node(id) = node {
                    self.apply_gradients(id, paint)?;
                    for (index, property) in [
                        (0, "radius"),
                        (1, "fill"),
                        (2, "stroke"),
                        (3, "stroke_width"),
                    ] {
                        self.record(
                            id,
                            property,
                            bound.argument(index).map(value_source),
                            &WriteSite {
                                statement: site.statement,
                                block: site.block,
                                site: Arc::clone(&site.site),
                            },
                        );
                    }
                }
                Ok(node)
            }
            Builtin::Text => self.text(args, site),
            Builtin::Image => self.image(args, site),
            Builtin::Audio => self.audio(args, site),
            Builtin::Video => self.video(args, site),
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
        let params = [
            "name",
            "width",
            "height",
            "fps",
            "duration",
            "background",
            "motion_blur",
            "shutter",
            "samples",
            "theme",
            "tempo",
        ];
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
        let motion_blur = match bound.get(6) {
            None | Some(Value::None) => false,
            Some(Value::Bool(value)) => *value,
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "comp(): 'motion_blur' must be True or False, got {}",
                    other.type_name()
                )))
            }
        };
        let shutter = bound.number_or(7, 180.0)?;
        let samples = bound.integer_in(8, 8, 1, 64)?;
        let samples = u32::try_from(samples)
            .map_err(|_| HostError::value_error("comp(): 'samples' is out of range"))?;
        // Theme and tempo come first so a background token resolves against
        // the theme this composition asks for, not the previous one.
        self.apply_comp_design(&bound)?;
        let composition = Composition::new(
            bound.string_or(0, "")?.as_ref(),
            side(1, 1920)?,
            side(2, 1080)?,
            frame_rate,
            frame_rate
                .seconds_to_frame(duration)
                .map_err(|error| core_error(&error))?,
            match bound.get(5) {
                None => Color::BLACK,
                Some(Value::Str(text)) => self.resolve_color(text)?,
                Some(other) => {
                    return Err(HostError::type_error(format!(
                        "comp(): 'background' must be a colour or a colour token, got {}",
                        other.type_name()
                    )))
                }
            },
        )
        .and_then(|composition| composition.with_motion_blur(motion_blur, shutter, samples))
        .map_err(|error| core_error(&error))?;
        self.document = Some(Document::new(composition));
        Ok(Value::None)
    }

    /// Applies `comp(theme=..., tempo=...)` before the composition is built.
    fn apply_comp_design(&mut self, bound: &Bound<'_>) -> HostResult<()> {
        if let Some(name) = bound.get(9) {
            let Value::Str(name) = name else {
                return Err(HostError::type_error(
                    "comp(): 'theme' must be a theme name string",
                ));
            };
            let theme = theme::by_name(name).ok_or_else(|| {
                HostError::value_error(format!(
                    "comp(): unknown theme {name:?}; use one of {}",
                    theme::names().collect::<Vec<_>>().join(", ")
                ))
            })?;
            theme
                .validate_contrast()
                .map_err(|error| HostError::value_error(format!("comp(): {error}")))?;
            self.theme = theme;
            if !self.tempo_explicit {
                self.tempo = theme.tempo;
            }
        }
        if let Some(value) = bound.get(10) {
            let tempo = match value {
                Value::Number(value) => *value,
                other => {
                    return Err(HostError::type_error(format!(
                        "comp(): 'tempo' must be a number, got {}",
                        other.type_name()
                    )))
                }
            };
            if !(tempo.is_finite() && tempo > 0.0) {
                return Err(HostError::value_error(
                    "comp(): 'tempo' must be a positive number of beats per minute",
                ));
            }
            self.tempo = tempo;
            self.tempo_explicit = true;
        }
        Ok(())
    }

    /// `theme(name)`: a curated token set, validated for WCAG AA.
    fn theme(args: CallArgs) -> HostResult<Value> {
        let bound = bind("theme", &["name"], 1, args)?;
        let name = bound.string_or(0, "")?;
        let theme = theme::by_name(&name).ok_or_else(|| {
            HostError::value_error(format!(
                "theme(): unknown theme {name:?}; use one of {}",
                theme::names().collect::<Vec<_>>().join(", ")
            ))
        })?;
        theme
            .validate_contrast()
            .map_err(|error| HostError::value_error(format!("theme(): {error}")))?;
        Ok(Value::Theme(Arc::new(theme.clone())))
    }

    /// `use_theme(t)`: makes `t` the default for constructors and setters.
    fn use_theme(&mut self, args: CallArgs) -> HostResult<Value> {
        let bound = bind("use_theme", &["theme"], 1, args)?;
        let Some(Value::Theme(theme)) = bound.get(0) else {
            let got = bound.get(0).map_or("nothing", Value::type_name);
            return Err(HostError::type_error(format!(
                "use_theme(): 'theme' must come from theme(name), got {got}"
            )));
        };
        theme
            .validate_contrast()
            .map_err(|error| HostError::value_error(format!("use_theme(): {error}")))?;
        // The prelude/theme owns a `'static` table; keep the active pointer.
        self.theme = theme::by_name(theme.name).unwrap_or(self.theme);
        if !self.tempo_explicit {
            self.tempo = self.theme.tempo;
        }
        Ok(Value::None)
    }

    /// `token(name)`: a design token as a value.
    fn token(&self, args: CallArgs) -> HostResult<Value> {
        let bound = bind("token", &["name"], 1, args)?;
        let name = bound.string_or(0, "")?;
        if let Some(color) = self.theme.color(&name) {
            return Ok(Value::Str(Arc::from(color.to_hex())));
        }
        let height = self.composition_height()?;
        if let Some(size) = self.theme.size(&name, height) {
            return Ok(Value::Number(size));
        }
        if let Some(space) = self.theme.space(&name, height) {
            return Ok(Value::Number(space));
        }
        if let Some(radius) = self.theme.radius(&name, height) {
            return Ok(Value::Number(radius));
        }
        if let Some(ease) = self.theme.ease(&name) {
            return Ok(Value::Str(Arc::from(ease.mdf_value())));
        }
        if let Some(font) = self.theme.font(name.strip_prefix("font:").unwrap_or("")) {
            return Ok(Value::Str(Arc::from(font)));
        }
        if let Some(weight) = match &*name {
            "weight_display" => Some(self.theme.display_weight),
            "weight_heading" => Some(self.theme.heading_weight),
            "weight_body" => Some(self.theme.body_weight),
            "weight_label" => Some(self.theme.label_weight),
            _ => None,
        } {
            return Ok(Value::Number(f64::from(weight)));
        }
        if &*name == "tempo" {
            return Ok(Value::Number(self.tempo));
        }
        Err(HostError::value_error(format!(
            "token(): unknown token {name:?}; colour tokens are {}, sizes are {}, spacing is {}, \
             radii are {} and motion tokens are {}",
            [
                "bg", "surface", "text", "muted", "accent", "accent_2", "success", "warning",
                "danger"
            ]
            .join(", "),
            ["display", "h1", "h2", "h3", "body", "caption", "label"].join(", "),
            ["xs", "s", "m", "l", "xl"].join(", "),
            ["s", "m", "l"].join(", "),
            ["enter", "exit", "move", "emphasis"].join(", ")
        )))
    }

    /// `beat(n)` / `bar(n)`: beats or four-beat bars as seconds.
    fn beat(&self, args: CallArgs, bar: bool) -> HostResult<Value> {
        let function = if bar { "bar" } else { "beat" };
        let bound = bind(function, &["count"], 1, args)?;
        let count = bound.number(0)?;
        if !count.is_finite() {
            return Err(HostError::value_error(format!(
                "{function}(): 'count' must be finite"
            )));
        }
        let beats = if bar { count * 4.0 } else { count };
        Ok(Value::Number(beats * 60.0 / self.tempo))
    }

    /// `comp_size()`: the composition as `(width, height)`.
    fn comp_size(&self, args: CallArgs) -> HostResult<Value> {
        bind("comp_size", &[], 0, args)?;
        let composition = self.document()?.composition();
        Ok(Value::tuple(vec![
            Value::Number(f64::from(composition.width())),
            Value::Number(f64::from(composition.height())),
        ]))
    }

    fn composition_height(&self) -> HostResult<f64> {
        Ok(f64::from(self.document()?.composition().height()))
    }

    fn composition_size(&self) -> HostResult<(f64, f64)> {
        let composition = self.document()?.composition();
        Ok((
            f64::from(composition.width()),
            f64::from(composition.height()),
        ))
    }

    /// `scene(name, start, length)`: a group visible only inside its window.
    ///
    /// Visibility is compiled to opacity hold keys (the document has no keyed
    /// visibility), so the group is an ordinary node and every other tool
    /// keeps working. `scene.t(x)` turns a scene-local time into an absolute
    /// one.
    fn scene(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let bound = bind("scene", &["name", "start", "length"], 3, args)?;
        let name = bound.string_or(0, "")?;
        let start = bound.number(1)?;
        let length = bound.number(2)?;
        if !(start.is_finite() && length.is_finite() && length > 0.0) {
            return Err(HostError::value_error(
                "scene(): 'start' must be finite and 'length' must be positive",
            ));
        }
        let group = self.create(NodeContent::Group, site)?;
        let Value::Node(id) = group else {
            return Ok(group);
        };
        self.document_mut()?
            .set_name(id, &name)
            .map_err(|error| core_error(&error))?;
        let end = start + length;
        let write = WriteSite {
            statement: site.statement,
            block: site.block,
            site: Arc::clone(&site.site),
        };
        let span = site.creation;
        self.key_value(
            id,
            "opacity",
            0.0,
            Value::Number(0.0),
            Ease::Linear,
            span,
            &write,
        )?;
        self.key_value(
            id,
            "opacity",
            start,
            Value::Number(1.0),
            Ease::Hold,
            span,
            &write,
        )?;
        self.key_value(
            id,
            "opacity",
            end,
            Value::Number(0.0),
            Ease::Hold,
            span,
            &write,
        )?;
        self.scenes.insert(id, SceneWindow { start, end });
        Ok(Value::Node(id))
    }

    /// `row(nodes, gap="m", align="center")` and its column twin.
    fn axis_layout(
        &mut self,
        args: CallArgs,
        metrics: &dyn TextMetrics,
        axis: Axis2,
    ) -> HostResult<Value> {
        let function = if axis == Axis2::Row { "row" } else { "column" };
        let bound = bind(function, &["nodes", "gap", "align"], 1, args)?;
        let nodes = list_items(bound.get(0), "layout(): 'nodes' must be a list")?;
        let gap = self.length_arg(bound.get(1), "m")?;
        let align = bound.string_or(2, "center")?;
        let mut entries = Vec::with_capacity(nodes.len());
        for node in &nodes {
            let Value::Node(id) = node else {
                return Err(HostError::type_error(format!(
                    "{function}(): 'nodes' must contain only nodes"
                )));
            };
            entries.push((*id, self.measure_node(*id, metrics)?));
        }
        let (width, height) = self.composition_size()?;
        let count = entries.len();
        #[allow(clippy::cast_precision_loss)]
        let gaps = gap * (count.saturating_sub(1)) as f64;
        let main: f64 = entries
            .iter()
            .map(|(_, bounds)| {
                if axis == Axis2::Row {
                    bounds.width()
                } else {
                    bounds.height()
                }
            })
            .sum::<f64>()
            + gaps;
        let cross = entries
            .iter()
            .map(|(_, bounds)| {
                if axis == Axis2::Row {
                    bounds.height()
                } else {
                    bounds.width()
                }
            })
            .fold(0.0_f64, f64::max);
        let mut cursor = if axis == Axis2::Row {
            width * 0.5 - main * 0.5
        } else {
            height * 0.5 - main * 0.5
        };
        for (id, box_bounds) in &entries {
            let (target_x, target_y) = if axis == Axis2::Row {
                (
                    cursor + box_bounds.width() * 0.5,
                    align_cross(
                        &align,
                        height * 0.5 - cross * 0.5,
                        height * 0.5 + cross * 0.5,
                        box_bounds.height(),
                    ),
                )
            } else {
                (
                    align_cross(
                        &align,
                        width * 0.5 - cross * 0.5,
                        width * 0.5 + cross * 0.5,
                        box_bounds.width(),
                    ),
                    cursor + box_bounds.height() * 0.5,
                )
            };
            self.set_box_center(*id, box_bounds, target_x, target_y)?;
            if axis == Axis2::Row {
                cursor += box_bounds.width() + gap;
            } else {
                cursor += box_bounds.height() + gap;
            }
        }
        Ok(Value::None)
    }

    /// `grid(nodes, cols, gap="m")`.
    #[allow(clippy::cast_precision_loss)] // a grid dimension is far below 2^53
    fn grid_layout(&mut self, args: CallArgs, metrics: &dyn TextMetrics) -> HostResult<Value> {
        let bound = bind("grid", &["nodes", "cols", "gap"], 2, args)?;
        let nodes = list_items(bound.get(0), "grid(): 'nodes' must be a list")?;
        let cols = bound.integer_in(1, 2, 1, 1000)?;
        let cols = usize::try_from(cols).unwrap_or(usize::MAX);
        let gap = self.length_arg(bound.get(2), "m")?;
        let mut entries = Vec::with_capacity(nodes.len());
        for node in &nodes {
            let Value::Node(id) = node else {
                return Err(HostError::type_error(
                    "grid(): 'nodes' must contain only nodes",
                ));
            };
            entries.push((*id, self.measure_node(*id, metrics)?));
        }
        let (width, height) = self.composition_size()?;
        let row_count = entries.len().div_ceil(cols);
        let mut column_widths = vec![0.0_f64; cols];
        let mut row_heights = vec![0.0_f64; row_count];
        for (index, (_, box_bounds)) in entries.iter().enumerate() {
            let column = index % cols;
            let row = index / cols;
            column_widths[column] = column_widths[column].max(box_bounds.width());
            row_heights[row] = row_heights[row].max(box_bounds.height());
        }
        let total_width: f64 =
            column_widths.iter().sum::<f64>() + gap * (cols.saturating_sub(1) as f64);
        let total_height: f64 =
            row_heights.iter().sum::<f64>() + gap * (row_count.saturating_sub(1) as f64);
        let left = width * 0.5 - total_width * 0.5;
        let top = height * 0.5 - total_height * 0.5;
        let mut column_x = Vec::with_capacity(cols);
        let mut x = left;
        for column_width in &column_widths {
            column_x.push(x + column_width * 0.5);
            x += column_width + gap;
        }
        let mut row_y = Vec::with_capacity(row_count);
        let mut y = top;
        for row_height in &row_heights {
            row_y.push(y + row_height * 0.5);
            y += row_height + gap;
        }
        for (index, (id, box_bounds)) in entries.iter().enumerate() {
            let column = index % cols;
            let row = index / cols;
            self.set_box_center(*id, box_bounds, column_x[column], row_y[row])?;
        }
        Ok(Value::None)
    }

    /// `stack(nodes)`: every node centred on the composition.
    fn stack_layout(&mut self, args: CallArgs, metrics: &dyn TextMetrics) -> HostResult<Value> {
        let bound = bind("stack", &["nodes"], 1, args)?;
        let nodes = list_items(bound.get(0), "stack(): 'nodes' must be a list")?;
        let (width, height) = self.composition_size()?;
        for node in &nodes {
            let Value::Node(id) = node else {
                return Err(HostError::type_error(
                    "stack(): 'nodes' must contain only nodes",
                ));
            };
            let box_bounds = self.measure_node(*id, metrics)?;
            self.set_box_center(*id, &box_bounds, width * 0.5, height * 0.5)?;
        }
        Ok(Value::None)
    }

    /// Places the measured box's centre at a composition point.
    fn set_box_center(
        &mut self,
        id: NodeId,
        box_bounds: &TextBounds,
        target_x: f64,
        target_y: f64,
    ) -> HostResult<()> {
        let node = self
            .document()?
            .node(id)
            .ok_or_else(|| HostError::value_error(format!("unknown node {id}")))?;
        let anchor = node.transform().anchor.value();
        let center_x = (box_bounds.left + box_bounds.right) * 0.5;
        let center_y = (box_bounds.top + box_bounds.bottom) * 0.5;
        self.document_mut()?
            .set_property(
                id,
                Property::Position,
                PropertyValue::Vec2(Vec2::new(
                    target_x - center_x + anchor.x,
                    target_y - center_y + anchor.y,
                )),
            )
            .map_err(|error| core_error(&error))
    }

    /// Measures a node's local box, recursing into groups.
    pub(crate) fn measure_node(
        &self,
        id: NodeId,
        metrics: &dyn TextMetrics,
    ) -> HostResult<TextBounds> {
        let document = self.document()?;
        let node = document
            .node(id)
            .ok_or_else(|| HostError::value_error(format!("unknown node {id}")))?;
        match node.content() {
            NodeContent::Text(text) => {
                let request = TextRequest {
                    text: &text.text,
                    size: text.size.value(),
                    font: &text.font,
                    weight: text.weight,
                    tracking: text.tracking.value(),
                    line_height: text.line_height.value(),
                    align: text.align,
                };
                let measurement = metrics.measure(&request).ok_or_else(|| {
                    HostError::value_error(format!(
                        "bounds(): no metrics for {:?} at weight {} (font files are not loaded \
                         in this run)",
                        text.font, text.weight
                    ))
                })?;
                Ok(measurement.ink)
            }
            NodeContent::Rect { width, height, .. } => Ok(TextBounds {
                left: -width.value() * 0.5,
                top: -height.value() * 0.5,
                right: width.value() * 0.5,
                bottom: height.value() * 0.5,
            }),
            NodeContent::Circle { radius, .. } => Ok(TextBounds {
                left: -radius.value(),
                top: -radius.value(),
                right: radius.value(),
                bottom: radius.value(),
            }),
            NodeContent::Path { geometry, .. } => {
                let [left, top, right, bottom] =
                    geometry.local_bounds().unwrap_or([0.0, 0.0, 0.0, 0.0]);
                Ok(TextBounds {
                    left,
                    top,
                    right,
                    bottom,
                })
            }
            NodeContent::Image(image) => {
                let width = image.width.value();
                let height = image.height.value();
                Ok(TextBounds {
                    left: -width * 0.5,
                    top: -height * 0.5,
                    right: width * 0.5,
                    bottom: height * 0.5,
                })
            }
            NodeContent::Group => {
                let mut union = TextBounds {
                    left: f64::INFINITY,
                    top: f64::INFINITY,
                    right: f64::NEG_INFINITY,
                    bottom: f64::NEG_INFINITY,
                };
                for child in node.children() {
                    let child_node = document
                        .node(*child)
                        .ok_or_else(|| HostError::value_error(format!("unknown node {child}")))?;
                    let local = self.measure_node(*child, metrics)?;
                    let position = child_node.transform().position.value();
                    let anchor = child_node.transform().anchor.value();
                    union.left = union.left.min(local.left + position.x - anchor.x);
                    union.top = union.top.min(local.top + position.y - anchor.y);
                    union.right = union.right.max(local.right + position.x - anchor.x);
                    union.bottom = union.bottom.max(local.bottom + position.y - anchor.y);
                }
                if union.left.is_finite() {
                    Ok(union)
                } else {
                    Ok(TextBounds::empty())
                }
            }
            NodeContent::Audio(_) | NodeContent::Video(_) => Ok(TextBounds::empty()),
        }
    }

    /// `node.bounds()`: the measured composition-space box.
    fn bounds_of(&self, id: NodeId, metrics: &dyn TextMetrics) -> HostResult<TextBounds> {
        let document = self.document()?;
        let mut bounds = self.measure_node(id, metrics)?;
        let mut current = document
            .node(id)
            .ok_or_else(|| HostError::value_error(format!("unknown node {id}")))?;
        loop {
            let position = current.transform().position.value();
            let anchor = current.transform().anchor.value();
            bounds.left += position.x - anchor.x;
            bounds.right += position.x - anchor.x;
            bounds.top += position.y - anchor.y;
            bounds.bottom += position.y - anchor.y;
            let Some(parent) = current.parent() else {
                break;
            };
            current = document
                .node(parent)
                .ok_or_else(|| HostError::value_error(format!("unknown node {parent}")))?;
        }
        Ok(bounds)
    }

    /// `node.place(anchor="center", x=None, y=None, margin=None)`.
    fn place(
        &mut self,
        id: NodeId,
        args: CallArgs,
        metrics: &dyn TextMetrics,
    ) -> HostResult<Value> {
        let bound = bind("place", &["anchor", "x", "y", "margin"], 0, args)?;
        let anchor_name = bound.string_or(0, "center")?;
        let box_bounds = self.measure_node(id, metrics)?;
        let (width, height) = self.composition_size()?;
        let is_text = self
            .document()?
            .node(id)
            .is_some_and(|node| matches!(node.content(), NodeContent::Text(_)));
        let default_margin = if is_text {
            width.min(height) * 0.1
        } else {
            0.0
        };
        let margin = match bound.get(3) {
            None => default_margin,
            Some(Value::Number(value)) => *value,
            Some(Value::Str(token)) => self.theme.space(token, height).ok_or_else(|| {
                HostError::value_error(format!(
                    "place(): unknown spacing token {token:?}; nearest is {:?}",
                    theme::spacing_token_suggestion(token)
                ))
            })?,
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "place(): 'margin' must be a number or a spacing token, got {}",
                    other.type_name()
                )))
            }
        };
        let fraction = |index: usize, default: f64| -> HostResult<f64> {
            match bound.get(index) {
                None => Ok(default),
                Some(Value::Number(value)) => Ok(*value),
                Some(other) => Err(HostError::type_error(format!(
                    "place(): '{}' must be a fraction of the composition, got {}",
                    if index == 1 { "x" } else { "y" },
                    other.type_name()
                ))),
            }
        };
        let (ax, ay, bx, by) = anchor_point(&anchor_name).ok_or_else(|| {
            HostError::value_error(format!(
                "place(): unknown anchor {anchor_name:?}; use center, top, bottom, left, right or \
                 a corner"
            ))
        })?;
        let default_x = if ax < 0.5 {
            margin / width
        } else if ax > 0.5 {
            1.0 - margin / width
        } else {
            0.5
        };
        let default_y = if ay < 0.5 {
            margin / height
        } else if ay > 0.5 {
            1.0 - margin / height
        } else {
            0.5
        };
        let x = fraction(1, default_x)? * width;
        let y = fraction(2, default_y)? * height;
        let local_x = box_bounds.left + (box_bounds.right - box_bounds.left) * bx;
        let local_y = box_bounds.top + (box_bounds.bottom - box_bounds.top) * by;
        let anchor = self
            .document()?
            .node(id)
            .ok_or_else(|| HostError::value_error(format!("unknown node {id}")))?
            .transform()
            .anchor
            .value();
        self.document_mut()?
            .set_property(
                id,
                Property::Position,
                PropertyValue::Vec2(Vec2::new(x - local_x + anchor.x, y - local_y + anchor.y)),
            )
            .map_err(|error| core_error(&error))?;
        Ok(Value::Node(id))
    }

    /// Resolves a paint argument: `#rrggbb`, a colour token, a gradient or
    /// `None`.
    fn paint_arg(
        &self,
        value: Option<&Value>,
        default: Option<PaintInput>,
    ) -> HostResult<Option<PaintInput>> {
        match value {
            None => Ok(default),
            Some(Value::None) => Ok(None),
            Some(Value::Gradient(gradient)) => Ok(Some(PaintInput::Gradient(Arc::clone(gradient)))),
            Some(Value::Str(text)) => {
                let color = self.resolve_color(text)?;
                Ok(Some(PaintInput::Color(color)))
            }
            Some(other) => Err(HostError::type_error(format!(
                "expected a colour, a colour token, a gradient or None, got {}",
                other.type_name()
            ))),
        }
    }

    /// Resolves `#rrggbb[aa]` or a theme colour token.
    fn resolve_color(&self, text: &str) -> HostResult<Color> {
        if text.starts_with('#') {
            return Color::parse_hex(text)
                .map_err(|error| HostError::value_error(error.to_string()));
        }
        self.theme.color(text).ok_or_else(|| {
            HostError::value_error(format!(
                "unknown colour token {text:?}; nearest is {:?} (or write \"#RRGGBB\")",
                theme::color_token_suggestion(text)
            ))
        })
    }

    /// A number that also accepts a type-scale, spacing or radius token.
    fn number_arg(&self, value: Option<&Value>, default: f64) -> HostResult<f64> {
        let height = self.composition_height()?;
        match value {
            None => Ok(default),
            Some(Value::Number(value)) => Ok(*value),
            Some(Value::Str(token)) => self
                .theme
                .size(token, height)
                .or_else(|| self.theme.space(token, height))
                .or_else(|| self.theme.radius(token, height))
                .ok_or_else(|| {
                    HostError::value_error(format!(
                        "unknown size/spacing token {token:?}; nearest is {:?}",
                        theme::size_token_suggestion(token)
                    ))
                }),
            Some(other) => Err(HostError::type_error(format!(
                "expected a number or a size token, got {}",
                other.type_name()
            ))),
        }
    }

    /// A font family or a `display`/`body`/`font:*` token.
    fn font_arg(&self, value: Option<&Value>, default: &str) -> HostResult<String> {
        match value {
            None => Ok(default.to_owned()),
            Some(Value::Str(text)) => {
                let name = text.strip_prefix("font:").unwrap_or(text);
                if let Some(font) = self.theme.font(name) {
                    Ok(font.to_owned())
                } else {
                    Ok(text.to_string())
                }
            }
            Some(other) => Err(HostError::type_error(format!(
                "expected a font family string, got {}",
                other.type_name()
            ))),
        }
    }

    /// Reads optional fill/stroke/stroke-width tokens starting at `first`.
    fn paint_args(
        &self,
        bound: &Bound<'_>,
        first: usize,
        fill_default: Option<PaintInput>,
        stroke_default: Option<PaintInput>,
        width_default: f64,
    ) -> HostResult<PaintArgs> {
        let fill = self.paint_arg(bound.get(first), fill_default)?;
        let stroke = self.paint_arg(bound.get(first + 1), stroke_default)?;
        let stroke_width = self.number_arg(bound.get(first + 2), width_default)?;
        let (fill, fill_gradient) = split_paint(fill);
        let (stroke_color, stroke_gradient) = split_paint(stroke);
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

    /// Resolves a token string written to a content field.
    fn token_content_value(&self, field: ContentField, value: &Value) -> HostResult<Value> {
        let Value::Str(text) = value else {
            return Ok(value.clone());
        };
        match field {
            ContentField::Fill
            | ContentField::Stroke
            | ContentField::Color
            | ContentField::FillStopColor(_)
            | ContentField::StrokeStopColor(_) => {
                Ok(Value::Str(Arc::from(self.resolve_color(text)?.to_hex())))
            }
            ContentField::Width
            | ContentField::Height
            | ContentField::Radius
            | ContentField::StrokeWidth
            | ContentField::TrimStart
            | ContentField::TrimEnd
            | ContentField::TrimOffset
            | ContentField::Dash
            | ContentField::DashOffset
            | ContentField::Size
            | ContentField::Tracking
            | ContentField::LineHeight
            | ContentField::Gain
            | ContentField::Pan
            | ContentField::InPoint
            | ContentField::OutPoint
            | ContentField::FadeIn
            | ContentField::FadeOut
            | ContentField::Speed
            | ContentField::FillStopOffset(_)
            | ContentField::StrokeStopOffset(_) => {
                Ok(Value::Number(self.number_arg(Some(value), 0.0)?))
            }
            _ => Ok(value.clone()),
        }
    }

    /// A numeric argument that also accepts a spacing/radius/size token.
    fn length_arg(&self, value: Option<&Value>, default_token: &str) -> HostResult<f64> {
        let height = self.composition_height()?;
        match value {
            None => self
                .theme
                .space(default_token, height)
                .ok_or_else(|| HostError::value_error(format!("unknown token {default_token:?}"))),
            Some(Value::Number(value)) => Ok(*value),
            Some(Value::Str(token)) => self
                .theme
                .space(token, height)
                .or_else(|| self.theme.radius(token, height))
                .ok_or_else(|| {
                    HostError::value_error(format!(
                        "unknown spacing token {token:?}; nearest is {:?}",
                        theme::spacing_token_suggestion(token)
                    ))
                }),
            Some(other) => Err(HostError::type_error(format!(
                "expected a number or a spacing token, got {}",
                other.type_name()
            ))),
        }
    }

    #[allow(clippy::too_many_lines)] // the constructor mirrors its flat parameter list
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
            "style",
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
        let color = self.paint_arg(bound.get(3), Some(PaintInput::Color(Color::WHITE)))?;
        let (color, gradient) = split_paint(color);
        let height = self.composition_height()?;
        let style = bound.get(8).map(|value| match value {
            Value::Str(name) => Ok(name.to_string()),
            other => Err(HostError::type_error(format!(
                "text(): 'style' must be a type-scale token, got {}",
                other.type_name()
            ))),
        });
        let style = match style {
            None => None,
            Some(Ok(name)) => Some(name),
            Some(Err(error)) => return Err(error),
        };
        let em_size = match style.as_deref() {
            Some(name) => self.theme.size(name, height).ok_or_else(|| {
                HostError::value_error(format!(
                    "text(): unknown style {name:?}; nearest is {:?}",
                    theme::size_token_suggestion(name)
                ))
            })?,
            None => self.number_arg(bound.get(2), 48.0)?,
        };
        let content = NodeContent::Text(TextContent::new(
            content.to_string(),
            // Roboto is the historical default: documents that never asked for
            // a family keep the exact advances they were authored with.
            self.font_arg(bound.get(1), "Roboto")?,
            em_size,
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
            let weight = match bound.get(5) {
                Some(value) => match value {
                    Value::Number(value) => *value,
                    other => {
                        return Err(HostError::type_error(format!(
                            "text(): 'weight' must be a number, got {}",
                            other.type_name()
                        )))
                    }
                },
                None => style.as_deref().map_or(400.0, |style| match style {
                    "display" => f64::from(self.theme.display_weight),
                    "h1" | "h2" | "h3" => f64::from(self.theme.heading_weight),
                    "label" => f64::from(self.theme.label_weight),
                    _ => 400.0,
                }),
            };
            let tracking = match bound.get(6) {
                Some(value) => self.number_arg(Some(value), 0.0)?,
                None => style.as_deref().map_or(0.0, |style| {
                    if matches!(style, "display" | "h1" | "h2" | "h3") {
                        self.theme.heading_tracking
                    } else {
                        self.theme.body_tracking
                    }
                }),
            };
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
                self.record(
                    id,
                    property,
                    bound.argument(index).map(value_source),
                    &WriteSite {
                        statement: site.statement,
                        block: site.block,
                        site: Arc::clone(&site.site),
                    },
                );
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
                self.record(
                    id,
                    property,
                    bound.argument(index).map(value_source),
                    &WriteSite {
                        statement: site.statement,
                        block: site.block,
                        site: Arc::clone(&site.site),
                    },
                );
            }
        }
        Ok(node)
    }

    /// `audio(source, in_point=0.0, out_point=None, gain=1.0, start=0,
    /// pan=0.0, fade_in=0, fade_out=0)` (D-0048).
    fn audio(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = [
            "source",
            "in_point",
            "out_point",
            "gain",
            "start",
            "pan",
            "fade_in",
            "fade_out",
        ];
        let bound = bind("audio", &params, 1, args)?;
        let source = bound.string_or(0, "")?;
        if source.is_empty() {
            return Err(HostError::value_error(
                "audio(): 'source' must be a non-empty project-relative path",
            ));
        }
        let optional = |index: usize, name: &str| -> HostResult<f64> {
            match bound.get(index) {
                None | Some(Value::None) => Ok(0.0),
                Some(Value::Number(number)) => Ok(*number),
                Some(other) => Err(HostError::type_error(format!(
                    "audio(): '{name}' must be a number or None, got {}",
                    other.type_name()
                ))),
            }
        };
        let start = bound.number_or(4, 0.0)?;
        if !start.is_finite() {
            return Err(HostError::value_error("audio(): 'start' must be finite"));
        }
        #[allow(clippy::cast_possible_truncation)]
        let start = start.round() as i64;
        let mut content = AudioContent::new(source.to_string(), start);
        content.in_point = motioner_core::Animatable::new(bound.number_or(1, 0.0)?);
        content.out_point = motioner_core::Animatable::new(optional(2, "out_point")?);
        content.gain = motioner_core::Animatable::new(bound.number_or(3, 1.0)?);
        content.pan = motioner_core::Animatable::new(bound.number_or(5, 0.0)?);
        content.fade_in = bound.number_or(6, 0.0)?.max(0.0);
        content.fade_out = bound.number_or(7, 0.0)?.max(0.0);
        let node = self.create(NodeContent::Audio(content), site)?;
        if let Value::Node(id) = node {
            for (index, property) in [
                (0, "source"),
                (1, "in_point"),
                (2, "out_point"),
                (3, "gain"),
                (4, "start"),
                (5, "pan"),
                (6, "fade_in"),
                (7, "fade_out"),
            ] {
                self.record(
                    id,
                    property,
                    bound.argument(index).map(value_source),
                    &WriteSite {
                        statement: site.statement,
                        block: site.block,
                        site: Arc::clone(&site.site),
                    },
                );
            }
        }
        Ok(node)
    }

    /// `video(source, in_point=0.0, out_point=None, start=0, speed=1.0,
    /// width=None, height=None, fit="contain", gain=1.0)` (Phase 6).
    fn video(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = [
            "source",
            "in_point",
            "out_point",
            "start",
            "speed",
            "width",
            "height",
            "fit",
            "gain",
        ];
        let bound = bind("video", &params, 1, args)?;
        let source = bound.string_or(0, "")?;
        if source.is_empty() {
            return Err(HostError::value_error(
                "video(): 'source' must be a non-empty project-relative path",
            ));
        }
        let optional = |index: usize, name: &str| -> HostResult<f64> {
            match bound.get(index) {
                None | Some(Value::None) => Ok(0.0),
                Some(Value::Number(number)) => Ok(*number),
                Some(other) => Err(HostError::type_error(format!(
                    "video(): '{name}' must be a number or None, got {}",
                    other.type_name()
                ))),
            }
        };
        let start = bound.number_or(3, 0.0)?;
        if !start.is_finite() {
            return Err(HostError::value_error("video(): 'start' must be finite"));
        }
        #[allow(clippy::cast_possible_truncation)]
        let start = start.round() as i64;
        let speed = bound.number_or(4, 1.0)?;
        if !speed.is_finite() || speed <= 0.0 {
            return Err(HostError::value_error(
                "video(): 'speed' must be a positive number",
            ));
        }
        let fit_name = bound.string_or(7, "contain")?;
        let fit = ImageFit::from_name(&fit_name).ok_or_else(|| {
            HostError::value_error(format!(
                "video(): unknown fit {fit_name:?}; use \"contain\", \"cover\" or \"fill\""
            ))
        })?;
        let mut content = VideoContent::new(source.to_string(), start);
        content.in_point = motioner_core::Animatable::new(bound.number_or(1, 0.0)?);
        content.out_point = motioner_core::Animatable::new(optional(2, "out_point")?);
        content.speed = speed;
        content.width = motioner_core::Animatable::new(optional(5, "width")?);
        content.height = motioner_core::Animatable::new(optional(6, "height")?);
        content.fit = fit;
        content.gain = motioner_core::Animatable::new(bound.number_or(8, 1.0)?);
        let node = self.create(NodeContent::Video(content), site)?;
        if let Value::Node(id) = node {
            for (index, property) in [
                (0, "source"),
                (1, "in_point"),
                (2, "out_point"),
                (3, "start"),
                (4, "speed"),
                (5, "width"),
                (6, "height"),
                (7, "fit"),
                (8, "gain"),
            ] {
                self.record(
                    id,
                    property,
                    bound.argument(index).map(value_source),
                    &WriteSite {
                        statement: site.statement,
                        block: site.block,
                        site: Arc::clone(&site.site),
                    },
                );
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
                self.record(
                    id,
                    &field.name(),
                    bound.argument(index).map(value_source),
                    &WriteSite {
                        statement: site.statement,
                        block: site.block,
                        site: Arc::clone(&site.site),
                    },
                );
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
        let paint = self.paint_args(
            bound,
            paint_at,
            None,
            Some(PaintInput::Color(Color::WHITE)),
            2.0,
        )?;
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
                self.record(
                    id,
                    property,
                    bound.argument(index).map(value_source),
                    &WriteSite {
                        statement: site.statement,
                        block: site.block,
                        site: Arc::clone(&site.site),
                    },
                );
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
        let node = document
            .add_node_with_identity(content, NodeIdentity::provenance(site.key()))
            .map_err(|error| core_error(&error))?;
        self.origins.record_node(NodeOrigin {
            node,
            creation: site.creation,
            statement: site.statement,
            block: site.block,
            site: Arc::clone(&site.site),
            bindings: Vec::new(),
        });
        Ok(Value::Node(node))
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
            "name" => node
                .name()
                .map_or(Value::None, |name| Value::Str(Arc::from(name))),
            "blend" => Value::Str(Arc::from(node.blend().name())),
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
        source: ValueSource,
        site: &WriteSite,
    ) -> HostResult<()> {
        let result = if name == "name" {
            let Value::Str(text) = value else {
                return Err(HostError::type_error(format!(
                    "'name' expects a string, got {}",
                    value.type_name()
                )));
            };
            self.document_mut()?
                .set_name(id, text)
                .map_err(|error| core_error(&error))
        } else if name == "blend" {
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
            let value = self.token_content_value(field, value)?;
            let value = content_value(field, &value)?;
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
        self.record(id, name, Some(source), site);
        Ok(())
    }

    /// Calls `node.method(args)`.
    pub fn call_method(
        &mut self,
        id: NodeId,
        method: NodeMethod,
        args: CallArgs,
        site: &WriteSite,
        metrics: &dyn TextMetrics,
    ) -> HostResult<Value> {
        match method {
            NodeMethod::Key => self.key(id, args, site),
            NodeMethod::Add => self.add_children(id, &args),
            NodeMethod::Id => self.assign_id(id, args),
            NodeMethod::Effect => self.effect(id, args, site),
            NodeMethod::EffectStack => self.effect_stack(id, args),
            NodeMethod::Mask => self.mask(id, args),
            NodeMethod::AnimateChars => self.animate_chars(id, args, site),
            NodeMethod::ClearChars => self.clear_chars(id),
            NodeMethod::Wiggle => self.wiggle(id, args, site),
            NodeMethod::Place => self.place(id, args, metrics),
            NodeMethod::Bounds => {
                bind("bounds", &[], 0, args)?;
                let bounds = self.bounds_of(id, metrics)?;
                Ok(Value::tuple(vec![
                    Value::Number(bounds.left),
                    Value::Number(bounds.top),
                    Value::Number(bounds.right),
                    Value::Number(bounds.bottom),
                ]))
            }
            NodeMethod::SceneTime => {
                let bound = bind("t", &["offset"], 1, args)?;
                let offset = bound.number(0)?;
                let window = self.scenes.get(&id).ok_or_else(|| {
                    HostError::value_error(
                        "t(): this node is not a scene; create one with scene(name, start, length)",
                    )
                })?;
                Ok(Value::Number(window.start + offset))
            }
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

    /// `stagger(nodes, property, values, start, step, duration, ease)`.
    ///
    /// `values` is a list of `(from, to)` pairs, one per node; a single pair is
    /// reused for every node. Node `i` gets a linear key at `start + i*step`
    /// with `from` and a key at `start + i*step + duration` with `to` and
    /// `ease`. The helper only emits ordinary keys, so it is deterministic and
    /// bakeable.
    fn stagger(&mut self, args: CallArgs, site: &CreationSite) -> HostResult<Value> {
        let params = [
            "nodes", "property", "values", "start", "step", "duration", "ease",
        ];
        let bound = bind("stagger", &params, 4, args)?;
        let nodes = list_items(bound.get(0), "stagger(): 'nodes' must be a list")?;
        let property = bound.string_or(1, "")?;
        let values = list_items(
            bound.get(2),
            "stagger(): 'values' must be a list of (from, to) pairs",
        )?;
        let start = bound.number_or(3, 0.0)?;
        let step = bound.number_or(4, 0.1)?;
        let duration = bound.number_or(5, 0.5)?;
        if !(start.is_finite() && step.is_finite() && duration.is_finite()) || duration < 0.0 {
            return Err(HostError::value_error(
                "stagger(): 'start', 'step' and 'duration' must be finite and duration non-negative",
            ));
        }
        let ease = match bound.get(6) {
            None | Some(Value::None) => Ease::Linear,
            Some(Value::Ease(ease)) => *ease,
            Some(Value::Str(name)) => Ease::from_name(name).ok_or_else(|| {
                HostError::value_error(format!(
                    "stagger(): unknown ease {name:?}; use an MDF ease name or bezier(...)"
                ))
            })?,
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "stagger(): 'ease' must be a string or bezier(...), got {}",
                    other.type_name()
                )))
            }
        };
        let span = Span::default();
        for (index, node) in nodes.iter().enumerate() {
            let Value::Node(id) = node else {
                return Err(HostError::type_error(
                    "stagger(): 'nodes' must contain only nodes",
                ));
            };
            let (from, to) = pair_at(&values, index, "stagger()")?;
            #[allow(clippy::cast_precision_loss)] // a list index is far below 2^53
            let time = start + index as f64 * step;
            self.key_value(
                *id,
                &property,
                time,
                from,
                Ease::Linear,
                span,
                &WriteSite {
                    statement: site.statement,
                    block: site.block,
                    site: Arc::clone(&site.site),
                },
            )?;
            self.key_value(
                *id,
                &property,
                time + duration,
                to,
                ease,
                span,
                &WriteSite {
                    statement: site.statement,
                    block: site.block,
                    site: Arc::clone(&site.site),
                },
            )?;
        }
        Ok(Value::None)
    }

    /// `node.wiggle(property, seed, frequency, amplitude, start, end)`.
    ///
    /// Emits one `sine_in_out` key per `1/frequency` seconds, with the value
    /// `base ± amplitude` from a documented `SplitMix64` stream. The same seed
    /// always produces the same keys. `base` is the property's current static
    /// value (existing keyframes are not sampled).
    fn wiggle(&mut self, id: NodeId, args: CallArgs, site: &WriteSite) -> HostResult<Value> {
        let params = ["property", "seed", "frequency", "amplitude", "start", "end"];
        let bound = bind("wiggle", &params, 6, args)?;
        let property = bound.string_or(0, "")?;
        let seed = bound.integer_in(1, 0, i64::MIN, i64::MAX)?;
        let frequency = bound.number(2)?;
        let amplitude = bound.number(3)?;
        let start = bound.number(4)?;
        let end = bound.number(5)?;
        if !(frequency.is_finite() && frequency > 0.0) {
            return Err(HostError::value_error(
                "wiggle(): 'frequency' must be a positive number of keys per second",
            ));
        }
        if !(amplitude.is_finite() && amplitude >= 0.0) {
            return Err(HostError::value_error(
                "wiggle(): 'amplitude' must be a finite, non-negative number",
            ));
        }
        if !(start.is_finite() && end.is_finite() && end >= start) {
            return Err(HostError::value_error(
                "wiggle(): 'start' and 'end' must be finite and end >= start",
            ));
        }
        let base = self.wiggle_base(id, &property)?;
        let span = bound.source(0).map_or_else(Span::default, |(span, _)| span);
        let mut state = splitmix64_seed(seed);
        let step = 1.0 / frequency;
        let mut time = start;
        while time <= end + 1e-9 {
            let random = splitmix64_next(&mut state);
            let value = amplitude.mul_add(2.0 * random - 1.0, base);
            self.key_value(
                id,
                &property,
                time,
                Value::Number(value),
                Ease::SineInOut,
                span,
                site,
            )?;
            time += step;
        }
        Ok(Value::None)
    }

    /// The static numeric value `wiggle` modulates.
    fn wiggle_base(&self, id: NodeId, property: &str) -> HostResult<f64> {
        let node = self
            .document()?
            .node(id)
            .ok_or_else(|| HostError::value_error(format!("unknown node {id}")))?;
        let (base, axis) = motioner_core::split_property(property);
        if let Some(field) = ContentField::from_name(base) {
            if let Some(ContentChannel::Number(value)) = node.content().channel(field) {
                return Ok(value.value());
            }
            return Err(HostError::value_error(format!(
                "wiggle(): '{property}' is not a numeric content channel"
            )));
        }
        match (Property::from_name(base), axis) {
            (Some(Property::Rotation), None) => Ok(node.transform().rotation.value()),
            (Some(Property::Opacity), None) => Ok(node.transform().opacity.value()),
            (Some(Property::Position), Some(axis)) => {
                Ok(node.transform().position.axis(axis).value())
            }
            (Some(Property::Scale), Some(axis)) => Ok(node.transform().scale.axis(axis).value()),
            (Some(Property::Anchor), Some(axis)) => Ok(node.transform().anchor.axis(axis).value()),
            (Some(parsed), _) if parsed.is_vector() => Err(HostError::value_error(format!(
                "wiggle(): '{property}' needs an axis ('{base}.x' or '{base}.y')"
            ))),
            _ => Err(HostError::value_error(format!(
                "wiggle(): unknown animatable property {property:?}"
            ))),
        }
    }

    /// Writes one ordinary key through the same path as `node.key(...)`.
    #[allow(clippy::too_many_arguments)] // mirrors the key site it forwards
    fn key_value(
        &mut self,
        id: NodeId,
        property: &str,
        time: f64,
        value: Value,
        ease: Ease,
        span: Span,
        site: &WriteSite,
    ) -> HostResult<()> {
        let mut args = CallArgs::default();
        args.positional.push(CallValue {
            value: Value::Str(Arc::from(property)),
            span,
            literal: Some(Literal::Text),
            origin: crate::origins::ExpressionOrigin::Literal(Literal::Text),
        });
        args.positional.push(CallValue {
            value: Value::Number(time),
            span,
            literal: Some(Literal::Number),
            origin: crate::origins::ExpressionOrigin::Literal(Literal::Number),
        });
        args.positional.push(CallValue {
            value,
            span,
            literal: None,
            origin: crate::origins::ExpressionOrigin::Computed,
        });
        args.keywords.push((
            Arc::from("ease"),
            CallValue {
                value: Value::Ease(ease),
                span,
                literal: None,
                origin: crate::origins::ExpressionOrigin::Computed,
            },
        ));
        self.key(id, args, site)?;
        Ok(())
    }

    /// `node.animate_chars(property, from, to, start, stagger, duration,
    /// ease="linear", order="forward")`: appends a per-character animator
    /// (D-0034).
    fn animate_chars(
        &mut self,
        id: NodeId,
        args: CallArgs,
        _site: &WriteSite,
    ) -> HostResult<Value> {
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
    #[allow(clippy::too_many_lines)] // one branch per effect parameter
    fn effect(&mut self, id: NodeId, args: CallArgs, site: &WriteSite) -> HostResult<Value> {
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
        if let (true, Some(argument)) = (bound.get(1).is_some(), bound.argument(1)) {
            let color = match bound.get(1) {
                Some(Value::Str(text)) => self.resolve_color(text)?,
                Some(Value::None) | None => Color::BLACK,
                Some(other) => {
                    return Err(HostError::type_error(format!(
                        "effect(): 'color' must be a colour or a colour token, got {}",
                        other.type_name()
                    )))
                }
            };
            let channel = EffectChannel {
                kind,
                param: EffectParam::Color,
                axis: None,
            };
            self.write_effect_param(
                id,
                channel,
                EffectValueInput::Color(color),
                value_source(argument),
                site,
            )?;
        }
        if let (true, Some(argument)) = (bound.get(2).is_some(), bound.argument(2)) {
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
                self.write_effect_param(
                    id,
                    channel,
                    EffectValueInput::Number(value),
                    value_source(argument),
                    site,
                )?;
            }
        }
        for (index, param) in [
            (3_usize, EffectParam::Blur),
            (4, EffectParam::Opacity),
            (5, EffectParam::Radius),
            (6, EffectParam::Intensity),
        ] {
            if let (true, Some(argument)) = (bound.get(index).is_some(), bound.argument(index)) {
                let number = bound.number(index)?;
                let channel = EffectChannel {
                    kind,
                    param,
                    axis: None,
                };
                self.write_effect_param(
                    id,
                    channel,
                    EffectValueInput::Number(number),
                    value_source(argument),
                    site,
                )?;
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
        source: ValueSource,
        site: &WriteSite,
    ) -> HostResult<()> {
        self.document_mut()?
            .set_effect(id, channel, value)
            .map_err(|error| core_error(&error))?;
        self.record(id, &channel.name(), Some(source), site);
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

    fn key(&mut self, id: NodeId, args: CallArgs, site: &WriteSite) -> HostResult<Value> {
        let bound = bind("key", &["property", "time", "value", "ease"], 3, args)?;
        let property_name = bound.string_or(0, "")?;
        if let Some(channel) = EffectChannel::from_name(&property_name) {
            return self.key_effect(id, channel, &property_name, &bound, site);
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
            return self.key_content(id, field, &property_name, axis, &bound, site);
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
        self.record_key(
            id,
            &property_name,
            &timing,
            bound.argument(2).map(value_source),
            site,
        );
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
        site: &WriteSite,
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
        let raw = bound.get(2).unwrap_or(&Value::None);
        let token_value = self.token_content_value(field, raw)?;
        let value = content_value(field, &token_value)?;
        let timing = self.key_timing(bound, time)?;
        self.document_mut()?
            .add_content_keyframe(id, field, timing.frame, value, timing.ease)
            .map_err(|error| core_error(&error))?;
        self.record_key(
            id,
            property_name,
            &timing,
            bound.argument(2).map(value_source),
            site,
        );
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
        site: &WriteSite,
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
        self.record_key(
            id,
            property_name,
            &timing,
            bound.argument(2).map(value_source),
            site,
        );
        Ok(Value::None)
    }

    fn key_timing(&self, bound: &Bound<'_>, time: f64) -> HostResult<KeyTiming> {
        let ease = match bound.get(3) {
            None | Some(Value::None) => Ease::Linear,
            Some(Value::Ease(ease)) => *ease,
            Some(Value::Str(name)) => Ease::from_name(name).ok_or_else(|| {
                HostError::value_error(format!(
                    "key(): unknown ease {name:?}; available eases are {} or bezier(...)",
                    quoted_names(Ease::ALL.iter().map(|(_, n)| *n))
                ))
            })?,
            Some(other) => {
                return Err(HostError::type_error(format!(
                    "key(): 'ease' must be a string or bezier(...), got {}",
                    other.type_name()
                )))
            }
        };
        let frame = self
            .document()?
            .composition()
            .frame_rate()
            .seconds_to_frame(time)
            .map_err(|error| core_error(&error))?;
        let ease_origin = bound.source(3).and_then(|(span, literal)| {
            (literal == Some(Literal::Text) || ease.is_bezier()).then(|| EaseOrigin {
                span,
                name: ease.mdf_value(),
            })
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
        source: Option<ValueSource>,
        site: &WriteSite,
    ) {
        if let Some(source) = source {
            self.origins.record(ValueOrigin {
                node: id,
                property: property_name.to_string(),
                span: source.span,
                literal: source.literal,
                kind: OriginKind::Key(timing.frame),
                ease: timing.ease_origin.clone(),
                expression: source.expression,
                statement: site.statement,
                block: site.block,
                site: Arc::clone(&site.site),
            });
        }
    }

    fn add_children(&mut self, id: NodeId, args: &CallArgs) -> HostResult<Value> {
        if !args.keywords.is_empty() {
            return Err(HostError::type_error("add() takes no keyword arguments"));
        }
        let children: Vec<Value> = match args.positional.as_slice() {
            [only] => match &only.value {
                Value::List(items) | Value::Tuple(items) => items.borrow().clone(),
                other => vec![other.clone()],
            },
            _ => args
                .positional
                .iter()
                .map(|argument| argument.value.clone())
                .collect(),
        };
        let document = self.document_mut()?;
        for child in &children {
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

/// The origin of a written argument, with its syntax classification.
fn value_source(value: &CallValue) -> ValueSource {
    ValueSource {
        span: value.span,
        literal: value.literal,
        expression: value.origin,
    }
}

/// Clones the items of a list or tuple argument.
fn list_items(value: Option<&Value>, message: &str) -> HostResult<Vec<Value>> {
    match value {
        Some(Value::List(items) | Value::Tuple(items)) => Ok(items.borrow().clone()),
        _ => Err(HostError::type_error(message)),
    }
}

/// The `(from, to)` pair for node `index`; a single pair is shared.
fn pair_at(values: &[Value], index: usize, what: &str) -> HostResult<(Value, Value)> {
    let pair = if values.len() == 1 {
        values.first()
    } else {
        values.get(index)
    };
    let Some(Value::List(items) | Value::Tuple(items)) = pair else {
        return Err(HostError::type_error(format!(
            "{what}: 'values' must contain (from, to) pairs"
        )));
    };
    let items = items.borrow();
    if items.len() != 2 {
        return Err(HostError::type_error(format!(
            "{what}: each pair must have exactly two values"
        )));
    }
    Ok((items[0].clone(), items[1].clone()))
}

/// Seeds a `SplitMix64` stream from an arbitrary 64-bit integer.
fn splitmix64_seed(seed: i64) -> u64 {
    #[allow(clippy::cast_sign_loss)] // the bit pattern is what matters
    {
        seed as u64
    }
}

/// Returns the next `SplitMix64` value as a uniform number in `[0, 1)`.
fn splitmix64_next(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    // The top 53 bits map exactly onto an f64 mantissa.
    #[allow(clippy::cast_precision_loss)]
    let value = (z >> 11) as f64 / 9_007_199_254_740_992.0;
    value
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

/// The main axis of `row` / `column`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis2 {
    Row,
    Column,
}

/// Anchor placement as `(comp_x, comp_y, box_x, box_y)`, each a 0..1
/// fraction of the corresponding span.
fn anchor_point(name: &str) -> Option<(f64, f64, f64, f64)> {
    let normalized = name.trim().to_ascii_lowercase().replace('-', "_");
    Some(match normalized.as_str() {
        "center" | "middle" => (0.5, 0.5, 0.5, 0.5),
        "top" | "top_center" => (0.5, 0.0, 0.5, 0.0),
        "bottom" | "bottom_center" => (0.5, 1.0, 0.5, 1.0),
        "left" | "left_center" => (0.0, 0.5, 0.0, 0.5),
        "right" | "right_center" => (1.0, 0.5, 1.0, 0.5),
        "top_left" => (0.0, 0.0, 0.0, 0.0),
        "top_right" => (1.0, 0.0, 1.0, 0.0),
        "bottom_left" => (0.0, 1.0, 0.0, 1.0),
        "bottom_right" => (1.0, 1.0, 1.0, 1.0),
        _ => return None,
    })
}

/// The box-centre coordinate on the cross axis of a row or column.
fn align_cross(align: &str, start: f64, end: f64, size: f64) -> f64 {
    match align {
        "start" | "left" | "top" => start + size * 0.5,
        "end" | "right" | "bottom" => end - size * 0.5,
        _ => (start + end) * 0.5,
    }
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
        ContentValue::Color(Some(color)) => Value::Str(Arc::from(color.to_hex())),
        ContentValue::Color(None) | ContentValue::Gradient(None) => Value::None,
        ContentValue::Text(text) => Value::Str(Arc::from(text)),
        ContentValue::Align(align) => Value::Str(Arc::from(match align {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        })),
        ContentValue::Cap(cap) => Value::Str(Arc::from(cap.name())),
        ContentValue::Join(join) => Value::Str(Arc::from(join.name())),
        ContentValue::Fit(fit) => Value::Str(Arc::from(fit.name())),
        ContentValue::Gradient(Some(gradient)) => Value::Gradient(Arc::new(gradient)),
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
        | ContentField::InPoint
        | ContentField::OutPoint
        | ContentField::Gain
        | ContentField::Pan
        | ContentField::Start
        | ContentField::FadeIn
        | ContentField::FadeOut
        | ContentField::Speed
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
