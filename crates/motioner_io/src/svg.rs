//! SVG import: a small, dependency-free reader for the subset Motioner can
//! represent (D-0054).
//!
//! The reader turns a file into a [`Symbol`] whose geometry is already
//! flattened to document coordinates. It understands groups, paths (all
//! commands, arcs converted to cubics), the basic shapes, the transform
//! functions, fills and strokes (including linear and radial gradients),
//! opacity, `defs`/`use`, `clipPath` and `viewBox`. Everything else is
//! reported as a warning that names the element id: nothing is dropped
//! silently.
//!
//! Not supported and warned about: text (as text nodes), filters, patterns,
//! embedded rasters and CSS stylesheets. A `d` with several subpaths is split
//! into numbered child parts under the same id.
//!
//! The parser is deliberately forgiving about numbers (`1e3`, `.5`, `-3`) and
//! strict about structure: a malformed tag is an error, not a panic.

use std::collections::BTreeMap;
use std::fmt;

use motioner_core::{
    Color, Gradient, LineCap, LineJoin, PaintSpec, PathGeometry, PathPoint, Symbol, SymbolPart,
    SymbolPath, SymbolShape, Vec2,
};

/// A fatal SVG reading error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgError {
    message: String,
}

impl SvgError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The human-readable reason.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SvgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SvgError {}

/// Parses `text` into a symbol named `name`.
///
/// # Errors
///
/// Returns [`SvgError`] when the document is not well-formed XML, has no
/// `<svg>` root, or has a geometry that cannot produce a valid path.
pub fn parse_svg(text: &str, name: &str) -> Result<Symbol, SvgError> {
    let root = parse_xml(text)?;
    let svg = if root.name == "svg" {
        root
    } else {
        root.children
            .into_iter()
            .find(|child| child.name == "svg")
            .ok_or_else(|| SvgError::new("the file has no <svg> root element"))?
    };
    let mut reader = Reader::new(&svg);
    let width = attribute_length(&svg, "width").unwrap_or(0.0);
    let height = attribute_length(&svg, "height").unwrap_or(0.0);
    let view_box = svg
        .attr("viewBox")
        .or_else(|| svg.attr("viewbox"))
        .and_then(parse_view_box);
    let (symbol_width, symbol_height) = match view_box {
        Some((_, _, width, height)) => (width, height),
        None => (
            if width > 0.0 { width } else { 100.0 },
            if height > 0.0 { height } else { 100.0 },
        ),
    };
    let mut style = Style::default();
    if let Some((min_x, min_y, width, height)) = view_box {
        let natural = (width, height);
        style.transform = root_transform(natural, (width, height), (min_x, min_y), &svg);
        style.root_view_box = Some((min_x, min_y, width, height));
    }
    let parts = reader.emit_children(&svg.children, &style);
    Ok(Symbol {
        name: name.to_owned(),
        width: symbol_width,
        height: symbol_height,
        parts,
        warnings: reader.warnings,
    })
}

/// The root `viewBox` scaling for `preserveAspectRatio` (`xMidYMid meet`).
fn root_transform(
    natural: (f64, f64),
    view: (f64, f64),
    origin: (f64, f64),
    svg: &Element,
) -> Affine {
    if natural.0 <= 0.0 || natural.1 <= 0.0 {
        return Affine::translate(-origin.0, -origin.1);
    }
    let preserve = svg.attr("preserveAspectRatio").unwrap_or("xMidYMid meet");
    let scale = if preserve.split_whitespace().next() == Some("none") {
        (natural.0 / view.0, natural.1 / view.1)
    } else {
        let scale = (natural.0 / view.0).min(natural.1 / view.1);
        (scale, scale)
    };
    let x = (natural.0 - view.0 * scale.0) / 2.0 - origin.0 * scale.0;
    let y = (natural.1 - view.1 * scale.1) / 2.0 - origin.1 * scale.1;
    Affine::translate(x, y).then(Affine::scale(scale.0, scale.1))
}

fn parse_view_box(text: &str) -> Option<(f64, f64, f64, f64)> {
    let numbers: Vec<f64> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.trim().parse::<f64>().ok())
        .collect();
    if numbers.len() == 4 && numbers[2] > 0.0 && numbers[3] > 0.0 {
        Some((numbers[0], numbers[1], numbers[2], numbers[3]))
    } else {
        None
    }
}

fn attribute_length(element: &Element, name: &str) -> Option<f64> {
    let raw = element.attr(name)?;
    let number: String = raw
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
        .collect();
    number.parse().ok()
}

// ---------------------------------------------------------------------------
// XML
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Element>,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

fn parse_xml(source: &str) -> Result<Element, SvgError> {
    let bytes = source.as_bytes();
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut position = 0usize;
    while let Some(open) = find(bytes, b'<', position) {
        if bytes[open..].starts_with(b"<!--") {
            position = source[open + 4..]
                .find("-->")
                .map_or(bytes.len(), |end| open + 4 + end + 3);
            continue;
        }
        if bytes[open..].starts_with(b"<?") {
            position = find(bytes, b'>', open).map_or(bytes.len(), |end| end + 1);
            continue;
        }
        if bytes[open..].starts_with(b"<!") {
            position = find(bytes, b'>', open).map_or(bytes.len(), |end| end + 1);
            continue;
        }
        let close = find(bytes, b'>', open)
            .ok_or_else(|| SvgError::new("unterminated tag: the file ends inside <...>"))?;
        let inner = &source[open + 1..close];
        if let Some(name) = inner.strip_prefix('/') {
            let name = name.trim();
            let element = stack
                .pop()
                .ok_or_else(|| SvgError::new(format!("</{name}> closes nothing")))?;
            if element.name != name {
                return Err(SvgError::new(format!(
                    "</{name}> closes <{}>",
                    element.name
                )));
            }
            attach(&mut stack, &mut root, element)?;
        } else {
            let self_closing = inner.trim_end().ends_with('/');
            let content = inner.trim_end().trim_end_matches('/');
            let element = parse_tag(content)?;
            if self_closing {
                attach(&mut stack, &mut root, element)?;
            } else {
                stack.push(element);
            }
        }
        position = close + 1;
    }
    if let Some(open) = stack.last() {
        return Err(SvgError::new(format!(
            "unclosed element <{}> at end of file",
            open.name
        )));
    }
    root.ok_or_else(|| SvgError::new("the file contains no element"))
}

#[allow(clippy::ptr_arg)] // the stack needs push and pop
fn attach(
    stack: &mut Vec<Element>,
    root: &mut Option<Element>,
    element: Element,
) -> Result<(), SvgError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(element);
    } else if root.is_some() {
        return Err(SvgError::new("the document has more than one root element"));
    } else {
        *root = Some(element);
    }
    Ok(())
}

fn parse_tag(content: &str) -> Result<Element, SvgError> {
    let bytes = content.as_bytes();
    let mut index = 0;
    skip_space(bytes, &mut index);
    let start = index;
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if start == index {
        return Err(SvgError::new("a tag has no name"));
    }
    let name = content[start..index].to_owned();
    let mut attrs = Vec::new();
    loop {
        skip_space(bytes, &mut index);
        if index >= bytes.len() {
            break;
        }
        let start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        let key = content[start..index].to_owned();
        skip_space(bytes, &mut index);
        if index < bytes.len() && bytes[index] == b'=' {
            index += 1;
            skip_space(bytes, &mut index);
            if index >= bytes.len() {
                return Err(SvgError::new(format!("attribute '{key}' has no value")));
            }
            let quote = bytes[index];
            if quote != b'"' && quote != b'\'' {
                let start = index;
                while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                attrs.push((key, decode_entities(&content[start..index])));
            } else {
                index += 1;
                let start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                if index >= bytes.len() {
                    return Err(SvgError::new(format!("unterminated value for '{key}'")));
                }
                attrs.push((key, decode_entities(&content[start..index])));
                index += 1;
            }
        } else if !key.is_empty() {
            attrs.push((key, String::new()));
        }
    }
    Ok(Element {
        name,
        attrs,
        children: Vec::new(),
    })
}

fn skip_space(bytes: &[u8], index: &mut usize) {
    while *index < bytes.len() && bytes[*index].is_ascii_whitespace() {
        *index += 1;
    }
}

fn find(bytes: &[u8], needle: u8, from: usize) -> Option<usize> {
    bytes
        .get(from..)
        .and_then(|slice| slice.iter().position(|&byte| byte == needle))
        .map(|offset| from + offset)
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        let Some(end) = rest.find(';') else {
            out.push('&');
            continue;
        };
        let entity = &rest[..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            other => other
                .strip_prefix("#x")
                .or_else(|| other.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| {
                    other
                        .strip_prefix('#')
                        .and_then(|digits| digits.parse::<u32>().ok())
                })
                .and_then(char::from_u32),
        };
        if let Some(character) = decoded {
            out.push(character);
        } else {
            out.push('&');
            out.push_str(entity);
            out.push(';');
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Affine transforms
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Affine {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn translate(x: f64, y: f64) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    fn scale(x: f64, y: f64) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }

    fn rotate(degrees: f64) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    fn then(self, next: Self) -> Self {
        Self {
            a: self.a * next.a + self.c * next.b,
            b: self.b * next.a + self.d * next.b,
            c: self.a * next.c + self.c * next.d,
            d: self.b * next.c + self.d * next.d,
            e: self.a * next.e + self.c * next.f + self.e,
            f: self.b * next.e + self.d * next.f + self.f,
        }
    }

    fn apply(self, point: Vec2) -> Vec2 {
        Vec2::new(
            self.a * point.x + self.c * point.y + self.e,
            self.b * point.x + self.d * point.y + self.f,
        )
    }

    fn apply_vector(self, vector: Vec2) -> Vec2 {
        Vec2::new(
            self.a * vector.x + self.c * vector.y,
            self.b * vector.x + self.d * vector.y,
        )
    }

    fn scale_factor(self) -> f64 {
        ((self.a * self.d - self.b * self.c).abs()).sqrt()
    }
}

fn parse_transform(text: &str) -> Affine {
    let mut result = Affine::IDENTITY;
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let arguments: Vec<f64> = rest[open + 1..open + close]
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.trim().parse::<f64>().ok())
            .collect();
        let number = |index: usize, default: f64| arguments.get(index).copied().unwrap_or(default);
        let step = match name {
            "translate" => Affine::translate(number(0, 0.0), number(1, 0.0)),
            "scale" => {
                let x = number(0, 1.0);
                Affine::scale(x, number(1, x))
            }
            "rotate" => {
                let rotation = Affine::rotate(number(0, 0.0));
                if arguments.len() >= 3 {
                    let cx = number(1, 0.0);
                    let cy = number(2, 0.0);
                    Affine::translate(cx, cy)
                        .then(rotation)
                        .then(Affine::translate(-cx, -cy))
                } else {
                    rotation
                }
            }
            "matrix" if arguments.len() >= 6 => Affine {
                a: number(0, 1.0),
                b: number(1, 0.0),
                c: number(2, 0.0),
                d: number(3, 1.0),
                e: number(4, 0.0),
                f: number(5, 0.0),
            },
            "skewX" => Affine {
                c: number(0, 0.0).to_radians().tan(),
                ..Affine::IDENTITY
            },
            "skewY" => Affine {
                b: number(0, 0.0).to_radians().tan(),
                ..Affine::IDENTITY
            },
            _ => Affine::IDENTITY,
        };
        result = result.then(step);
        rest = &rest[open + close + 1..];
    }
    result
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaintKind {
    None,
    Solid,
    Gradient,
}

#[derive(Debug, Clone)]
struct Style {
    transform: Affine,
    fill: Option<Color>,
    fill_gradient_id: Option<String>,
    fill_gradient: Option<Gradient>,
    stroke: Option<Color>,
    stroke_gradient_id: Option<String>,
    stroke_gradient: Option<Gradient>,
    stroke_width: f64,
    cap: LineCap,
    join: LineJoin,
    opacity: f64,
    fill_opacity: f64,
    stroke_opacity: f64,
    dash: Option<f64>,
    root_view_box: Option<(f64, f64, f64, f64)>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            transform: Affine::IDENTITY,
            fill: Some(Color::BLACK),
            fill_gradient_id: None,
            fill_gradient: None,
            stroke: None,
            stroke_gradient_id: None,
            stroke_gradient: None,
            stroke_width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            opacity: 1.0,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            dash: None,
            root_view_box: None,
        }
    }
}

impl Style {
    fn inherit(&self, element: &Element) -> Self {
        let mut style = self.clone();
        // Group opacity is applied by the node hierarchy, so each element
        // carries only its own opacity here.
        style.opacity = 1.0;
        if let Some(transform) = element.attr("transform") {
            style.transform = style.transform.then(parse_transform(transform));
        }
        if let Some(style_text) = element.attr("style") {
            for declaration in style_text.split(';') {
                let Some((key, value)) = declaration.split_once(':') else {
                    continue;
                };
                style.presentation(key.trim(), value.trim());
            }
        }
        for key in ["fill", "stroke"] {
            if let Some(value) = element.attr(key) {
                style.presentation(key, value);
            }
        }
        for (key, target) in [
            ("stroke-width", "stroke-width"),
            ("stroke-linecap", "stroke-linecap"),
            ("stroke-linejoin", "stroke-linejoin"),
            ("stroke-opacity", "stroke-opacity"),
            ("fill-opacity", "fill-opacity"),
            ("stroke-dasharray", "stroke-dasharray"),
            ("opacity", "opacity"),
        ] {
            if let Some(value) = element.attr(key) {
                style.presentation(target, value);
            }
        }
        style
    }

    fn presentation(&mut self, key: &str, value: &str) {
        match key {
            "fill" => match paint_kind(value) {
                PaintKind::None => {
                    self.fill = None;
                    self.fill_gradient_id = None;
                }
                PaintKind::Gradient => {
                    self.fill_gradient_id = paint_reference(value);
                }
                PaintKind::Solid => {
                    if let Some(color) = parse_color(value) {
                        self.fill = Some(color);
                        self.fill_gradient_id = None;
                    }
                }
            },
            "stroke" => match paint_kind(value) {
                PaintKind::None => {
                    self.stroke = None;
                    self.stroke_gradient_id = None;
                }
                PaintKind::Gradient => {
                    self.stroke_gradient_id = paint_reference(value);
                }
                PaintKind::Solid => {
                    if let Some(color) = parse_color(value) {
                        self.stroke = Some(color);
                        self.stroke_gradient_id = None;
                    }
                }
            },
            "stroke-width" => {
                if let Some(width) = parse_number(value) {
                    self.stroke_width = width.max(0.0);
                }
            }
            "stroke-linecap" => {
                if let Some(cap) = LineCap::from_name(value) {
                    self.cap = cap;
                }
            }
            "stroke-linejoin" => {
                if let Some(join) = LineJoin::from_name(value) {
                    self.join = join;
                }
            }
            "stroke-opacity" => {
                if let Some(opacity) = parse_number(value) {
                    self.stroke_opacity = opacity.clamp(0.0, 1.0);
                }
            }
            "fill-opacity" => {
                if let Some(opacity) = parse_number(value) {
                    self.fill_opacity = opacity.clamp(0.0, 1.0);
                }
            }
            "stroke-dasharray" => {
                self.dash = value
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .find_map(parse_number)
                    .filter(|dash| *dash > 0.0);
            }
            "opacity" => {
                if let Some(opacity) = parse_number(value) {
                    self.opacity *= opacity.clamp(0.0, 1.0);
                }
            }
            _ => {}
        }
    }
}

fn paint_kind(value: &str) -> PaintKind {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") || value.is_empty() {
        PaintKind::None
    } else if value.starts_with("url(") {
        PaintKind::Gradient
    } else {
        PaintKind::Solid
    }
}

fn paint_reference(value: &str) -> Option<String> {
    let inner = value
        .trim()
        .strip_prefix("url(")?
        .trim_end_matches(')')
        .trim();
    let id = inner.trim_matches(|c| c == '#' || c == '"' || c == '\'');
    Some(id.to_owned())
}

fn apply_alpha(color: Color, opacity: f64) -> Color {
    let alpha = (f64::from(color.a) * opacity.clamp(0.0, 1.0)).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // The value is clamped to 0..=255 by the arithmetic above.
    let alpha = alpha as u8;
    Color::rgba(color.r, color.g, color.b, alpha)
}

fn parse_number(text: &str) -> Option<f64> {
    let number: String = text
        .trim()
        .trim_end_matches('%')
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
        .collect();
    number.parse().ok()
}

fn parse_color(text: &str) -> Option<Color> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('#') {
        if rest.len() == 3 {
            let mut expanded = String::from("#");
            for character in rest.chars() {
                expanded.push(character);
                expanded.push(character);
            }
            return Color::parse_hex(&expanded).ok();
        }
        return Color::parse_hex(text).ok();
    }
    if let Some(inner) = text
        .strip_prefix("rgb(")
        .or_else(|| text.strip_prefix("rgba("))
    {
        let values: Vec<f64> = inner
            .split([',', ' ', '/'])
            .filter(|part| !part.trim().is_empty())
            .filter_map(|part| part.trim().trim_end_matches('%').parse::<f64>().ok())
            .collect();
        if values.len() >= 3 {
            let channel = |index: usize| {
                let value = values[index];
                if text.contains('%') {
                    (value / 100.0 * 255.0).round()
                } else {
                    value.round()
                }
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // round() then clamp to a byte.
            let to_byte = |value: f64| value.clamp(0.0, 255.0) as u8;
            let alpha = values.get(3).map_or(255.0, |alpha| (alpha * 255.0).round());
            return Some(Color::rgba(
                to_byte(channel(0)),
                to_byte(channel(1)),
                to_byte(channel(2)),
                to_byte(alpha),
            ));
        }
    }
    named_color(text)
}

fn named_color(text: &str) -> Option<Color> {
    let color = match text {
        "black" | "currentColor" | "context-fill" => Color::BLACK,
        "white" => Color::WHITE,
        "red" => Color::rgb(255, 0, 0),
        "green" => Color::rgb(0, 128, 0),
        "blue" => Color::rgb(0, 0, 255),
        "yellow" => Color::rgb(255, 255, 0),
        "orange" => Color::rgb(255, 165, 0),
        "grey" | "gray" => Color::rgb(128, 128, 128),
        "transparent" => Color::rgba(0, 0, 0, 0),
        _ => return None,
    };
    Some(color)
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

struct Reader<'a> {
    elements: BTreeMap<String, &'a Element>,
    warnings: Vec<String>,
}

impl<'a> Reader<'a> {
    fn new(root: &'a Element) -> Self {
        let mut elements = BTreeMap::new();
        collect_ids(root, &mut elements);
        Self {
            elements,
            warnings: Vec::new(),
        }
    }

    fn warn(&mut self, id: Option<&str>, message: impl fmt::Display) {
        let label = id.unwrap_or("<anonymous>");
        self.warnings.push(format!("element '{label}': {message}"));
    }

    fn emit_children(&mut self, children: &'a [Element], style: &Style) -> Vec<SymbolPart> {
        let mut parts = Vec::new();
        for child in children {
            self.emit_element(child, style, &mut parts);
        }
        parts
    }

    #[allow(clippy::too_many_lines)] // one match arm per supported SVG element
    fn emit_element(&mut self, element: &'a Element, parent: &Style, out: &mut Vec<SymbolPart>) {
        let style = parent.inherit(element);
        match element.name.as_str() {
            "g" | "a" | "switch" | "svg" => {
                let mut children = self.emit_children(&element.children, &style);
                let clip = self.clip_parts(element, &style);
                if children.is_empty() && clip.is_empty() {
                    return;
                }
                if clip.is_empty() && element.attr("id").is_none() {
                    out.append(&mut children);
                    return;
                }
                out.push(SymbolPart {
                    id: element.attr("id").map(str::to_owned),
                    anchor: anchor_of(&children),
                    opacity: style.opacity,
                    shape: SymbolShape::Group(children),
                    clip,
                });
            }
            "path" => {
                let Some(data) = element.attr("d") else {
                    self.warn(element.attr("id"), "path without a 'd' attribute");
                    return;
                };
                let subpaths = match parse_path_data(data) {
                    Ok(subpaths) => subpaths,
                    Err(message) => {
                        self.warn(element.attr("id"), message);
                        return;
                    }
                };
                self.emit_subpaths(element, &style, &subpaths, out);
            }
            "rect" => {
                let subpaths = shape_subpaths(element, Shape::Rect);
                self.emit_subpaths(element, &style, &subpaths, out);
            }
            "circle" | "ellipse" => {
                let subpaths = shape_subpaths(element, Shape::Ellipse);
                self.emit_subpaths(element, &style, &subpaths, out);
            }
            "line" => {
                let x1 = attribute_length(element, "x1").unwrap_or(0.0);
                let y1 = attribute_length(element, "y1").unwrap_or(0.0);
                let x2 = attribute_length(element, "x2").unwrap_or(0.0);
                let y2 = attribute_length(element, "y2").unwrap_or(0.0);
                let subpaths = vec![Subpath {
                    points: vec![
                        PathPoint::corner(Vec2::new(x1, y1)),
                        PathPoint::corner(Vec2::new(x2, y2)),
                    ],
                    closed: false,
                }];
                self.emit_subpaths(element, &style, &subpaths, out);
            }
            "polyline" | "polygon" => {
                let Some(points) = element.attr("points") else {
                    self.warn(element.attr("id"), "polygon without 'points'");
                    return;
                };
                let numbers: Vec<f64> = points
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|part| !part.is_empty())
                    .filter_map(|part| part.trim().parse::<f64>().ok())
                    .collect();
                let mut path = Vec::new();
                for pair in numbers.chunks_exact(2) {
                    path.push(PathPoint::corner(Vec2::new(pair[0], pair[1])));
                }
                if path.len() < 2 {
                    self.warn(element.attr("id"), "polygon needs at least two points");
                    return;
                }
                let subpaths = vec![Subpath {
                    points: path,
                    closed: element.name == "polygon",
                }];
                self.emit_subpaths(element, &style, &subpaths, out);
            }
            "use" => {
                let reference = element
                    .attr("href")
                    .or_else(|| element.attr("xlink:href"))
                    .and_then(|href| href.strip_prefix('#'))
                    .map(str::to_owned);
                let Some(reference) = reference else {
                    self.warn(element.attr("id"), "<use> without a local href");
                    return;
                };
                let Some(target) = self.elements.get(&reference).copied() else {
                    self.warn(
                        element.attr("id"),
                        format!("<use> points at missing '{reference}'"),
                    );
                    return;
                };
                let mut use_style = style;
                let x = attribute_length(element, "x").unwrap_or(0.0);
                let y = attribute_length(element, "y").unwrap_or(0.0);
                use_style.transform = use_style.transform.then(Affine::translate(x, y));
                let mut parts = Vec::new();
                self.emit_element(target, &use_style, &mut parts);
                if let (Some(id), Some(first)) = (element.attr("id"), parts.first_mut()) {
                    first.id = Some(id.to_owned());
                }
                out.append(&mut parts);
            }
            "clipPath" | "defs" | "linearGradient" | "radialGradient" | "stop" | "title"
            | "desc" | "metadata" | "style" => {}
            "text" | "tspan" => self.warn(
                element.attr("id"),
                "text is not supported; convert it to outlines",
            ),
            "image" => self.warn(
                element.attr("id"),
                "embedded raster images are not supported; import the file with use()",
            ),
            "filter" | "mask" | "pattern" | "foreignObject" => {
                self.warn(
                    element.attr("id"),
                    format!("<{}> is not supported", element.name),
                );
            }
            other => self.warn(element.attr("id"), format!("<{other}> is not supported")),
        }
    }

    fn clip_parts(&mut self, element: &'a Element, style: &Style) -> Vec<SymbolPart> {
        let Some(reference) = element.attr("clip-path") else {
            return Vec::new();
        };
        let Some(id) = paint_reference(reference) else {
            self.warn(element.attr("id"), "unreadable clip-path reference");
            return Vec::new();
        };
        let Some(clip) = self.elements.get(&id).copied() else {
            self.warn(
                element.attr("id"),
                format!("clip-path points at missing '{id}'"),
            );
            return Vec::new();
        };
        let mut parts = Vec::new();
        parts.extend(self.emit_children(&clip.children, style));
        parts
    }

    fn emit_subpaths(
        &mut self,
        element: &'a Element,
        style: &Style,
        subpaths: &[Subpath],
        out: &mut Vec<SymbolPart>,
    ) {
        let clip = self.clip_parts(element, style);
        let bounds = subpath_bounds(subpaths, style);
        let mut style = style.clone();
        if let Some(id) = style.fill_gradient_id.clone() {
            style.fill_gradient = self.resolve_gradient(&id, &style, bounds);
        }
        if let Some(id) = style.stroke_gradient_id.clone() {
            style.stroke_gradient = self.resolve_gradient(&id, &style, bounds);
        }
        let fill = style
            .fill
            .map(|color| apply_alpha(color, style.fill_opacity));
        let stroke = style
            .stroke
            .map(|color| apply_alpha(color, style.stroke_opacity));
        let mut children = Vec::new();
        for (index, subpath) in subpaths.iter().enumerate() {
            let points: Vec<PathPoint> = subpath
                .points
                .iter()
                .map(|point| {
                    PathPoint::curved(
                        style.transform.apply(point.position),
                        style.transform.apply_vector(point.handle_in),
                        style.transform.apply_vector(point.handle_out),
                    )
                })
                .collect();
            if subpath.closed && points.len() >= 2 {
                // Closing segment handles live on the last point.
                // Nothing else to move: the closed flag closes the contour.
            }
            if points.len() < 2 {
                continue;
            }
            let geometry = match PathGeometry::new(points, subpath.closed, style.cap, style.join) {
                Ok(geometry) => geometry,
                Err(error) => {
                    self.warn(element.attr("id"), error);
                    continue;
                }
            };
            let mut paint = PaintSpec::new(
                fill,
                stroke,
                if stroke.is_some() {
                    style.stroke_width * style.transform.scale_factor()
                } else {
                    0.0
                },
            );
            if style.fill_gradient.is_some() {
                paint.fill_gradient.clone_from(&style.fill_gradient);
            }
            if style.stroke_gradient.is_some() {
                paint.stroke_gradient.clone_from(&style.stroke_gradient);
            }
            let id = if subpaths.len() > 1 {
                element.attr("id").map(|id| format!("{id}#{}", index + 1))
            } else {
                element.attr("id").map(str::to_owned)
            };
            children.push(SymbolPart {
                id,
                anchor: anchor_of_geometry(&geometry),
                opacity: style.opacity,
                shape: SymbolShape::Path(SymbolPath {
                    geometry,
                    paint,
                    dash: style.dash,
                }),
                clip: Vec::new(),
            });
        }
        if children.len() == 1 && clip.is_empty() {
            out.push(children.remove(0));
            return;
        }
        if children.is_empty() {
            return;
        }
        let anchor = anchor_of(&children);
        out.push(SymbolPart {
            id: element.attr("id").map(str::to_owned),
            anchor,
            opacity: 1.0,
            shape: SymbolShape::Group(children),
            clip,
        });
    }
}

fn collect_ids<'a>(element: &'a Element, map: &mut BTreeMap<String, &'a Element>) {
    if let Some(id) = element.attr("id") {
        map.insert(id.to_owned(), element);
    }
    if let Some(label) = element
        .attr("inkscape:label")
        .or_else(|| element.attr("data-name"))
    {
        map.entry(label.to_owned()).or_insert(element);
    }
    for child in &element.children {
        collect_ids(child, map);
    }
}

fn subpath_bounds(subpaths: &[Subpath], style: &Style) -> Option<[f64; 4]> {
    let mut bounds: Option<[f64; 4]> = None;
    for subpath in subpaths {
        for point in &subpath.points {
            let position = style.transform.apply(point.position);
            let x = position.x;
            let y = position.y;
            bounds = Some(match bounds {
                None => [x, y, x, y],
                Some(current) => [
                    current[0].min(x),
                    current[1].min(y),
                    current[2].max(x),
                    current[3].max(y),
                ],
            });
        }
    }
    bounds
}

fn anchor_of(parts: &[SymbolPart]) -> Vec2 {
    let mut bounds: Option<[f64; 4]> = None;
    for part in parts {
        merge_bounds(&mut bounds, part_bounds(part));
    }
    bounds.map_or(Vec2::ZERO, centre_of)
}

fn anchor_of_geometry(geometry: &PathGeometry) -> Vec2 {
    geometry.local_bounds().map_or(Vec2::ZERO, |bounds| {
        Vec2::new(
            f64::midpoint(bounds[0], bounds[2]),
            f64::midpoint(bounds[1], bounds[3]),
        )
    })
}

fn centre_of(bounds: [f64; 4]) -> Vec2 {
    Vec2::new(
        f64::midpoint(bounds[0], bounds[2]),
        f64::midpoint(bounds[1], bounds[3]),
    )
}

fn merge_bounds(target: &mut Option<[f64; 4]>, source: Option<[f64; 4]>) {
    if let Some(source) = source {
        *target = Some(match *target {
            None => source,
            Some(current) => [
                current[0].min(source[0]),
                current[1].min(source[1]),
                current[2].max(source[2]),
                current[3].max(source[3]),
            ],
        });
    }
}

fn part_bounds(part: &SymbolPart) -> Option<[f64; 4]> {
    match &part.shape {
        SymbolShape::Group(children) => {
            let mut bounds = None;
            for child in children {
                merge_bounds(&mut bounds, part_bounds(child));
            }
            merge_bounds(&mut bounds, clip_bounds(&part.clip));
            bounds
        }
        SymbolShape::Path(path) => path.geometry.local_bounds(),
    }
}

fn clip_bounds(clip: &[SymbolPart]) -> Option<[f64; 4]> {
    let mut bounds = None;
    for part in clip {
        merge_bounds(&mut bounds, part_bounds(part));
    }
    bounds
}

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

struct Subpath {
    points: Vec<PathPoint>,
    closed: bool,
}

#[derive(Clone, Copy)]
enum Shape {
    Rect,
    Ellipse,
}

const KAPPA: f64 = 0.552_284_749_830_793_4;

fn shape_subpaths(element: &Element, shape: Shape) -> Vec<Subpath> {
    match shape {
        Shape::Rect => {
            let x = attribute_length(element, "x").unwrap_or(0.0);
            let y = attribute_length(element, "y").unwrap_or(0.0);
            let width = attribute_length(element, "width").unwrap_or(0.0).max(0.0);
            let height = attribute_length(element, "height").unwrap_or(0.0).max(0.0);
            let radius = attribute_length(element, "rx")
                .or_else(|| attribute_length(element, "ry"))
                .unwrap_or(0.0)
                .max(0.0)
                .min(width.min(height) / 2.0);
            if width <= 0.0 || height <= 0.0 {
                return Vec::new();
            }
            let r = radius.min(width / 2.0).min(height / 2.0);
            let k = r * KAPPA;
            let points = if r <= 0.0 {
                vec![
                    PathPoint::corner(Vec2::new(x, y)),
                    PathPoint::corner(Vec2::new(x + width, y)),
                    PathPoint::corner(Vec2::new(x + width, y + height)),
                    PathPoint::corner(Vec2::new(x, y + height)),
                ]
            } else {
                vec![
                    PathPoint::curved(Vec2::new(x + r, y), Vec2::ZERO, Vec2::new(-k, 0.0)),
                    PathPoint::curved(Vec2::new(x + width - r, y), Vec2::new(k, 0.0), Vec2::ZERO),
                    PathPoint::curved(Vec2::new(x + width - r, y), Vec2::ZERO, Vec2::new(0.0, -k)),
                    PathPoint::curved(Vec2::new(x + width, y + r), Vec2::new(0.0, k), Vec2::ZERO),
                    PathPoint::curved(
                        Vec2::new(x + width, y + height - r),
                        Vec2::ZERO,
                        Vec2::new(0.0, k),
                    ),
                    PathPoint::curved(
                        Vec2::new(x + width - r, y + height),
                        Vec2::new(k, 0.0),
                        Vec2::ZERO,
                    ),
                    PathPoint::curved(Vec2::new(x + r, y + height), Vec2::ZERO, Vec2::new(-k, 0.0)),
                    PathPoint::curved(Vec2::new(x, y + height - r), Vec2::new(0.0, k), Vec2::ZERO),
                    PathPoint::curved(Vec2::new(x, y + r), Vec2::ZERO, Vec2::new(0.0, -k)),
                ]
            };
            vec![Subpath {
                points,
                closed: true,
            }]
        }
        Shape::Ellipse => {
            let cx = attribute_length(element, "cx").unwrap_or(0.0);
            let cy = attribute_length(element, "cy").unwrap_or(0.0);
            let rx = attribute_length(element, "rx")
                .or_else(|| attribute_length(element, "r"))
                .unwrap_or(0.0);
            let ry = attribute_length(element, "ry")
                .or_else(|| attribute_length(element, "r"))
                .unwrap_or(rx);
            if rx <= 0.0 || ry <= 0.0 {
                return Vec::new();
            }
            let (kx, ky) = (rx * KAPPA, ry * KAPPA);
            let left = Vec2::new(cx - rx, cy);
            let right = Vec2::new(cx + rx, cy);
            let top = Vec2::new(cx, cy - ry);
            let bottom = Vec2::new(cx, cy + ry);
            let points = vec![
                PathPoint::curved(left, Vec2::new(0.0, ky), Vec2::new(0.0, -ky)),
                PathPoint::curved(top, Vec2::new(-kx, 0.0), Vec2::new(kx, 0.0)),
                PathPoint::curved(right, Vec2::new(0.0, -ky), Vec2::new(0.0, ky)),
                PathPoint::curved(bottom, Vec2::new(kx, 0.0), Vec2::new(-kx, 0.0)),
            ];
            vec![Subpath {
                points,
                closed: true,
            }]
        }
    }
}

// ---------------------------------------------------------------------------
// Path data
// ---------------------------------------------------------------------------

/// Parses a `d` attribute into subpaths of authored points.
///
/// Handles command repetition, implicit lineto after moveto, relative
/// commands, smooth cubics/quadratics and elliptical arcs.
#[allow(clippy::too_many_lines)] // one arm per SVG path command
fn parse_path_data(data: &str) -> Result<Vec<Subpath>, String> {
    let mut scanner = PathScanner::new(data);
    let mut subpaths = Vec::new();
    let mut current = Subpath {
        points: Vec::new(),
        closed: false,
    };
    let mut point = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    let mut last_cubic: Option<Vec2> = None;
    let mut last_quad: Option<Vec2> = None;
    scanner.skip_space();
    while let Some(letter) = scanner.peek_command() {
        scanner.bump();
        let mut command = letter;
        if command == 'Z' || command == 'z' {
            if !current.points.is_empty() {
                current.closed = true;
                subpaths.push(std::mem::replace(
                    &mut current,
                    Subpath {
                        points: Vec::new(),
                        closed: false,
                    },
                ));
            }
            point = start;
            continue;
        }
        loop {
            let relative = command.is_ascii_lowercase();
            let origin = if relative { point } else { Vec2::ZERO };
            let upper = command.to_ascii_uppercase();
            let read_point = |scanner: &mut PathScanner<'_>| -> Result<Vec2, String> {
                let x = scanner.number()?;
                let y = scanner.number()?;
                Ok(Vec2::new(x, y))
            };
            match upper {
                'M' => {
                    let target = origin + read_point(&mut scanner)?;
                    if !current.points.is_empty() {
                        subpaths.push(std::mem::replace(
                            &mut current,
                            Subpath {
                                points: Vec::new(),
                                closed: false,
                            },
                        ));
                    }
                    start = target;
                    current.points.push(PathPoint::corner(target));
                    point = target;
                    command = if relative { 'l' } else { 'L' };
                }
                'L' => {
                    let target = origin + read_point(&mut scanner)?;
                    push_corner(&mut current, target);
                    point = target;
                    last_cubic = None;
                    last_quad = None;
                }
                'H' => {
                    let x = scanner.number()?;
                    let target = Vec2::new(if relative { point.x + x } else { x }, point.y);
                    push_corner(&mut current, target);
                    point = target;
                    last_cubic = None;
                    last_quad = None;
                }
                'V' => {
                    let y = scanner.number()?;
                    let target = Vec2::new(point.x, if relative { point.y + y } else { y });
                    push_corner(&mut current, target);
                    point = target;
                    last_cubic = None;
                    last_quad = None;
                }
                'C' => {
                    let c1 = origin + read_point(&mut scanner)?;
                    let c2 = origin + read_point(&mut scanner)?;
                    let target = origin + read_point(&mut scanner)?;
                    push_cubic(&mut current, point, c1, c2, target);
                    point = target;
                    last_cubic = Some(c2);
                    last_quad = None;
                }
                'S' => {
                    let c1 = last_cubic.map_or(point, |control| point * 2.0 - control);
                    let c2 = origin + read_point(&mut scanner)?;
                    let target = origin + read_point(&mut scanner)?;
                    push_cubic(&mut current, point, c1, c2, target);
                    point = target;
                    last_cubic = Some(c2);
                    last_quad = None;
                }
                'Q' => {
                    let control = origin + read_point(&mut scanner)?;
                    let target = origin + read_point(&mut scanner)?;
                    push_quadratic(&mut current, point, control, target);
                    point = target;
                    last_quad = Some(control);
                    last_cubic = None;
                }
                'T' => {
                    let control = last_quad.map_or(point, |control| point * 2.0 - control);
                    let target = origin + read_point(&mut scanner)?;
                    push_quadratic(&mut current, point, control, target);
                    point = target;
                    last_quad = Some(control);
                    last_cubic = None;
                }
                'A' => {
                    let rx = scanner.number()?.abs();
                    let ry = scanner.number()?.abs();
                    let rotation = scanner.number()?;
                    let large = scanner.flag()?;
                    let sweep = scanner.flag()?;
                    let target = origin + read_point(&mut scanner)?;
                    for (c1, c2, end) in
                        arc_to_cubics(point, rx, ry, rotation, large, sweep, target)
                    {
                        push_cubic(&mut current, point, c1, c2, end);
                        point = end;
                    }
                    point = target;
                    last_cubic = None;
                    last_quad = None;
                }
                other => return Err(format!("unknown path command '{other}'")),
            }
            scanner.skip_space();
            if !scanner.at_number() {
                break;
            }
        }
    }
    if !current.points.is_empty() {
        subpaths.push(current);
    }
    Ok(subpaths)
}

fn push_corner(subpath: &mut Subpath, target: Vec2) {
    subpath.points.push(PathPoint::corner(target));
}

fn push_cubic(subpath: &mut Subpath, from: Vec2, c1: Vec2, c2: Vec2, target: Vec2) {
    if let Some(last) = subpath.points.last_mut() {
        if last.position == from {
            last.handle_out = c1 - from;
        }
    }
    subpath
        .points
        .push(PathPoint::curved(target, c2 - target, Vec2::ZERO));
}

fn push_quadratic(subpath: &mut Subpath, from: Vec2, control: Vec2, target: Vec2) {
    let c1 = from + (control - from) * (2.0 / 3.0);
    let c2 = target + (control - target) * (2.0 / 3.0);
    push_cubic(subpath, from, c1, c2, target);
}

/// Converts one SVG elliptical arc to at most four cubic segments.
#[allow(clippy::too_many_arguments)]
fn arc_to_cubics(
    from: Vec2,
    rx: f64,
    ry: f64,
    rotation: f64,
    large: bool,
    sweep: bool,
    to: Vec2,
) -> Vec<(Vec2, Vec2, Vec2)> {
    if rx <= 0.0 || ry <= 0.0 || from == to {
        return Vec::new();
    }
    let phi = rotation.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (from.x - to.x) / 2.0;
    let dy = (from.y - to.y) / 2.0;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;
    let radii = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let radii = if radii < 0.0 { 0.0 } else { radii };
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let factor = if denominator == 0.0 {
        0.0
    } else {
        (radii / denominator).sqrt()
    };
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let cx1 = sign * factor * rx * y1 / ry;
    let cy1 = -sign * factor * ry * x1 / rx;
    let cx = cos_phi * cx1 - sin_phi * cy1 + f64::midpoint(from.x, to.x);
    let cy = sin_phi * cx1 + cos_phi * cy1 + f64::midpoint(from.y, to.y);
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let dot = ux * vx + uy * vy;
        let length = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        if length == 0.0 {
            0.0
        } else {
            let cosine = (dot / length).clamp(-1.0, 1.0);
            let sign = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
            sign * cosine.acos()
        }
    };
    let start_angle = angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let delta = angle(
        (x1 - cx1) / rx,
        (y1 - cy1) / ry,
        (-x1 - cx1) / rx,
        (-y1 - cy1) / ry,
    );
    let delta = if !sweep && delta > 0.0 {
        delta - std::f64::consts::TAU
    } else if sweep && delta < 0.0 {
        delta + std::f64::consts::TAU
    } else {
        delta
    };
    let segments = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let segments = segments as u32;
    let step = delta / f64::from(segments);
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let mut out = Vec::new();
    let mut angle = start_angle;
    let (rx_cos, ry_sin) = (rx, ry);
    for _ in 0..segments {
        let next = angle + step;
        let (sin_a, cos_a) = angle.sin_cos();
        let (sin_b, cos_b) = next.sin_cos();
        let point = |sin: f64, cos: f64| {
            Vec2::new(
                cx + cos_phi * rx_cos * cos - sin_phi * ry_sin * sin,
                cy + sin_phi * rx_cos * cos + cos_phi * ry_sin * sin,
            )
        };
        let p0 = point(sin_a, cos_a);
        let p3 = point(sin_b, cos_b);
        let d0 = Vec2::new(
            cos_phi * rx_cos * -sin_a - sin_phi * ry_sin * cos_a,
            sin_phi * rx_cos * -sin_a + cos_phi * ry_sin * cos_a,
        );
        let d1 = Vec2::new(
            cos_phi * rx_cos * -sin_b - sin_phi * ry_sin * cos_b,
            sin_phi * rx_cos * -sin_b + cos_phi * ry_sin * cos_b,
        );
        out.push((p0 + d0 * handle, p3 - d1 * handle, p3));
        angle = next;
    }
    out
}

struct PathScanner<'a> {
    bytes: &'a [u8],
    source: &'a str,
    index: usize,
}

impl<'a> PathScanner<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            bytes: source.as_bytes(),
            source,
            index: 0,
        }
    }

    fn skip_space(&mut self) {
        while self.index < self.bytes.len()
            && (self.bytes[self.index].is_ascii_whitespace() || self.bytes[self.index] == b',')
        {
            self.index += 1;
        }
    }

    fn peek_command(&mut self) -> Option<char> {
        self.skip_space();
        self.bytes
            .get(self.index)
            .map(|&byte| byte as char)
            .filter(char::is_ascii_alphabetic)
    }

    fn at_number(&mut self) -> bool {
        self.skip_space();
        self.bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.'))
    }

    fn bump(&mut self) {
        self.index += 1;
    }

    fn flag(&mut self) -> Result<bool, String> {
        self.skip_space();
        match self.bytes.get(self.index) {
            Some(b'0') => {
                self.index += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.index += 1;
                Ok(true)
            }
            _ => Err("expected an arc flag (0 or 1)".to_owned()),
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        self.skip_space();
        let start = self.index;
        if self.bytes.get(self.index) == Some(&b'-') || self.bytes.get(self.index) == Some(&b'+') {
            self.index += 1;
        }
        let mut seen_dot = false;
        while let Some(&byte) = self.bytes.get(self.index) {
            if byte.is_ascii_digit() {
                self.index += 1;
            } else if byte == b'.' && !seen_dot {
                seen_dot = true;
                self.index += 1;
            } else {
                break;
            }
        }
        if self.bytes.get(self.index) == Some(&b'e') || self.bytes.get(self.index) == Some(&b'E') {
            self.index += 1;
            if self.bytes.get(self.index) == Some(&b'-')
                || self.bytes.get(self.index) == Some(&b'+')
            {
                self.index += 1;
            }
            while self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
                self.index += 1;
            }
        }
        self.source[start..self.index]
            .parse()
            .map_err(|_| format!("expected a number near {:?}", &self.source[start..]))
    }
}

// ---------------------------------------------------------------------------
// Gradient hookup
// ---------------------------------------------------------------------------

/// Resolves gradient references after the tree was read.
///
/// Reads `<linearGradient>`/`<radialGradient>` definitions into concrete
/// [`Gradient`] values in document coordinates. Referenced paint ids that do
/// not resolve are reported by the caller through `warnings`.
fn gradient_from_element(
    elements: &BTreeMap<String, &Element>,
    id: &str,
    style: &Style,
    bounds: Option<[f64; 4]>,
) -> Result<Gradient, String> {
    let element = elements
        .get(id)
        .ok_or_else(|| format!("gradient '{id}' is not defined"))?;
    let stops: Vec<(f64, Color)> = element
        .children
        .iter()
        .filter(|child| child.name == "stop")
        .map(|stop| {
            let offset = stop
                .attr("offset")
                .and_then(|raw| {
                    if let Some(percent) = raw.trim().strip_suffix('%') {
                        percent.parse::<f64>().ok().map(|value| value / 100.0)
                    } else {
                        raw.trim().parse::<f64>().ok()
                    }
                })
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let color = stop
                .attr("stop-color")
                .and_then(parse_color)
                .unwrap_or(Color::BLACK);
            let opacity = stop
                .attr("stop-opacity")
                .and_then(parse_number)
                .unwrap_or(1.0);
            (offset, apply_alpha(color, opacity))
        })
        .collect();
    if stops.len() < 2 {
        return Err(format!("gradient '{id}' needs two stops"));
    }
    let user_space = element.attr("gradientUnits") == Some("userSpaceOnUse");
    let (min_x, min_y, max_x, max_y) = match bounds {
        Some(bounds) => (bounds[0], bounds[1], bounds[2], bounds[3]),
        None => (
            style.root_view_box.map_or(0.0, |view| view.0),
            style.root_view_box.map_or(0.0, |view| view.1),
            style.root_view_box.map_or(100.0, |view| view.0 + view.2),
            style.root_view_box.map_or(100.0, |view| view.1 + view.3),
        ),
    };
    let coordinate = |element: &Element, name: &str, default: f64, axis: bool| -> f64 {
        let Some(raw) = element.attr(name) else {
            return default;
        };
        let value = parse_number(raw).unwrap_or(0.0);
        if user_space {
            value
        } else if let Some(percent) = raw.trim().strip_suffix('%') {
            let fraction = percent.parse::<f64>().unwrap_or(0.0) / 100.0;
            if axis {
                min_x + (max_x - min_x) * fraction
            } else {
                min_y + (max_y - min_y) * fraction
            }
        } else if axis {
            min_x + (max_x - min_x) * value
        } else {
            min_y + (max_y - min_y) * value
        }
    };
    if element.name == "linearGradient" {
        let start = Vec2::new(
            coordinate(element, "x1", min_x, true),
            coordinate(element, "y1", min_y, false),
        );
        let end = Vec2::new(
            coordinate(element, "x2", max_x, true),
            coordinate(element, "y2", min_y, false),
        );
        let start = style.transform.apply(start);
        let end = style.transform.apply(end);
        Gradient::linear(start, end, stops).map_err(|error| error.to_string())
    } else {
        let center = Vec2::new(
            coordinate(element, "cx", f64::midpoint(min_x, max_x), true),
            coordinate(element, "cy", f64::midpoint(min_y, max_y), false),
        );
        let radius = element.attr("r").and_then(parse_number).unwrap_or(0.5)
            * if user_space {
                1.0
            } else {
                (max_x - min_x).max(max_y - min_y)
            };
        let center = style.transform.apply(center);
        Gradient::radial(center, radius * style.transform.scale_factor(), stops)
            .map_err(|error| error.to_string())
    }
}

/// Resolves a paint gradient reference for the given element bounds. Called by
/// [`Reader::emit_subpaths`] through [`Reader::resolve_gradient`].
impl Reader<'_> {
    fn resolve_gradient(
        &mut self,
        id: &str,
        style: &Style,
        bounds: Option<[f64; 4]>,
    ) -> Option<Gradient> {
        match gradient_from_element(&self.elements, id, style, bounds) {
            Ok(gradient) => Some(gradient),
            Err(message) => {
                self.warnings.push(message);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASCOT: &str = include_str!("../../../examples/rig/assets/mascot.svg");

    fn svg(text: &str) -> Symbol {
        parse_svg(text, "test").expect("a valid symbol")
    }

    fn wrapper(body: &str) -> String {
        format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\">{body}</svg>")
    }

    #[test]
    fn the_mascot_parses_with_every_stable_id() {
        let symbol = parse_svg(MASCOT, "mascot").expect("mascot.svg parses");
        assert_eq!((symbol.width, symbol.height), (200.0, 200.0));
        assert!(symbol.warnings.is_empty(), "{:?}", symbol.warnings);
        for id in [
            "mascot",
            "body",
            "face",
            "eye-left",
            "eye-right",
            "pupil-left",
            "pupil-right",
            "lid-top-left",
            "lid-top-right",
            "lid-bottom-left",
            "brow-left",
            "brow-right",
            "mouth",
            "mouth-shape",
            "mouth-teeth",
            "mouth-line",
            "glow",
            "sparks",
        ] {
            assert!(symbol.part(id).is_some(), "missing part {id}");
        }
        let body = symbol.part("body").expect("body");
        let bounds = part_bounds(body).expect("body bounds");
        assert!((bounds[0] - 40.0).abs() < 1e-6);
        assert!((bounds[3] - 160.0).abs() < 1e-6);
        let eye = symbol.part("eye-left").expect("eye");
        assert!((eye.anchor.x - 76.0).abs() < 1e-6);
        assert!((eye.anchor.y - 88.0).abs() < 1e-6);
        assert!(!eye.clip.is_empty(), "the eye clip path is a mask");
    }

    #[test]
    fn transforms_are_flattened_into_geometry() {
        let symbol = svg(&wrapper(
            "<g transform=\"translate(10 20) scale(2)\"><rect id=\"r\" x=\"0\" y=\"0\" width=\"10\" height=\"5\" fill=\"#ff0000\"/></g>",
        ));
        let rect = symbol.part("r").expect("r");
        let bounds = part_bounds(rect).expect("bounds");
        assert!((bounds[0] - 10.0).abs() < 1e-6, "{bounds:?}");
        assert!((bounds[2] - 30.0).abs() < 1e-6);
        assert!((bounds[1] - 20.0).abs() < 1e-6);
        assert!((bounds[3] - 30.0).abs() < 1e-6);
        let SymbolShape::Path(path) = &rect.shape else {
            panic!("a path");
        };
        let fill = path.paint.fill.value().expect("a fill");
        assert_eq!((fill.r, fill.g, fill.b), (255, 0, 0));
    }

    #[test]
    fn arcs_become_cubics_with_a_bounded_error() {
        let symbol = svg(&wrapper(
            "<path id=\"c\" d=\"M 50 20 A 30 30 0 1 1 50 80 A 30 30 0 1 1 50 20 Z\" fill=\"#ffffff\"/>",
        ));
        let part = symbol.part("c").expect("c");
        let bounds = part_bounds(part).expect("bounds");
        for (actual, expected) in bounds.iter().zip([20.0, 20.0, 80.0, 80.0]) {
            assert!((actual - expected).abs() < 0.2, "{bounds:?}");
        }
    }

    #[test]
    fn relative_commands_and_implicit_lineto_are_supported() {
        let symbol = svg(&wrapper(
            "<path id=\"p\" d=\"m10 10 20 0 0 20 -20 0 z\" fill=\"#000000\"/>",
        ));
        let part = symbol.part("p").expect("p");
        let bounds = part_bounds(part).expect("bounds");
        assert!(bounds
            .iter()
            .zip([10.0, 10.0, 30.0, 30.0])
            .all(|(a, b)| (a - b).abs() < 1e-9));
    }

    #[test]
    fn a_circle_and_a_rounded_rect_have_the_expected_bounds() {
        let symbol = svg(&wrapper(
            "<circle id=\"c\" cx=\"50\" cy=\"50\" r=\"20\" fill=\"#000000\"/>\
             <rect id=\"r\" x=\"5\" y=\"5\" width=\"40\" height=\"20\" rx=\"5\" fill=\"#000000\"/>",
        ));
        let circle = part_bounds(symbol.part("c").expect("c")).expect("circle bounds");
        for (actual, expected) in circle.iter().zip([30.0, 30.0, 70.0, 70.0]) {
            assert!((actual - expected).abs() < 0.05, "{circle:?}");
        }
        let rect = part_bounds(symbol.part("r").expect("r")).expect("rect bounds");
        assert!((rect[0] - 5.0).abs() < 1e-6);
        assert!((rect[3] - 25.0).abs() < 1e-6);
    }

    #[test]
    fn a_clip_path_becomes_a_mask_source() {
        let symbol = svg(&wrapper(
            "<defs><clipPath id=\"clip\"><circle cx=\"50\" cy=\"50\" r=\"10\"/></clipPath></defs>\
             <g id=\"g\" clip-path=\"url(#clip)\"><rect x=\"0\" y=\"0\" width=\"100\" height=\"100\" fill=\"#000000\"/></g>",
        ));
        let group = symbol.part("g").expect("g");
        assert_eq!(group.clip.len(), 1);
        let clip_bounds = part_bounds(&group.clip[0]).expect("clip bounds");
        assert!((clip_bounds[2] - 60.0).abs() < 0.1);
    }

    #[test]
    fn use_instantiates_the_referenced_element() {
        let symbol = svg(&wrapper(
            "<defs><rect id=\"tile\" x=\"0\" y=\"0\" width=\"10\" height=\"10\" fill=\"#000000\"/></defs>\
             <use id=\"u\" href=\"#tile\" x=\"30\" y=\"40\"/>",
        ));
        let used = symbol.part("u").expect("u");
        let bounds = part_bounds(used).expect("bounds");
        assert!(bounds
            .iter()
            .zip([30.0, 40.0, 40.0, 50.0])
            .all(|(a, b)| (a - b).abs() < 1e-9));
    }

    #[test]
    fn gradients_are_resolved_into_paint() {
        let symbol = svg(&wrapper(
            "<defs><linearGradient id=\"g\"><stop offset=\"0\" stop-color=\"#000000\"/><stop offset=\"1\" stop-color=\"#ffffff\"/></linearGradient></defs>\
             <rect id=\"r\" x=\"0\" y=\"0\" width=\"100\" height=\"100\" fill=\"url(#g)\"/>",
        ));
        let SymbolShape::Path(path) = &symbol.part("r").expect("r").shape else {
            panic!("a path");
        };
        assert!(path.paint.fill_gradient.is_some());
    }

    #[test]
    fn text_is_a_warning_naming_the_element() {
        let symbol = svg(&wrapper("<text id=\"caption\">Hi</text>"));
        assert!(symbol.parts.is_empty());
        assert!(symbol
            .warnings
            .iter()
            .any(|warning| warning.contains("caption") && warning.contains("text")));
    }

    #[test]
    fn a_malformed_document_is_an_error_not_a_panic() {
        assert!(parse_svg("<svg><g></svg>", "bad").is_err());
        // A broken `d` is a warning on that element, not a failure of the file.
        let symbol = svg("<svg><path id=\"p\" d=\"M 0 0 L\"/></svg>");
        assert!(symbol
            .warnings
            .iter()
            .any(|warning| warning.contains("'p'")));
    }
}
