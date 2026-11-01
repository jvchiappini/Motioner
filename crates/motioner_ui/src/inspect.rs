//! Read-only inspector model: what the properties panel shows.
//!
//! [`inspect_node`] samples the selected node's transform at the current
//! frame with the evaluator's own [`motioner_eval::sample`], so the panel
//! shows exactly the values the frame was rendered with. [`inspect_comp`]
//! describes the composition when nothing is selected. Both are pure and
//! return display strings, so formatting is unit-tested without a window.

use motioner_core::{
    Color, Composition, Document, Frame, NodeContent, NodeId, Paint, TextAlign, Transform,
};
use motioner_eval::sample;

use crate::outline::{number, NodeKind};

/// A displayed value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Two labelled fields side by side, such as `X 960` / `Y 480`.
    Pair {
        /// First field: short label and value.
        first: (&'static str, String),
        /// Second field: short label and value.
        second: (&'static str, String),
    },
    /// One full-width field.
    Text(String),
    /// A colour swatch with its hex code.
    Color(Color),
    /// No value (for example "no fill").
    None,
}

/// One labelled property row.
#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    /// Row label.
    pub label: &'static str,
    /// Displayed value.
    pub value: Value,
    /// Whether the property has keyframes.
    pub animated: bool,
}

/// A titled group of property rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Section title, shown as a small caption.
    pub title: &'static str,
    /// Rows in display order.
    pub props: Vec<Prop>,
}

/// Formats a colour as `#RRGGBB`, plus the alpha byte when it is not opaque.
#[must_use]
pub fn hex(color: Color) -> String {
    if color.a == 255 {
        format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            color.r, color.g, color.b, color.a
        )
    }
}

fn fixed(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    number(rounded)
}

fn percent(value: f64) -> String {
    format!("{}%", fixed(value * 100.0))
}

fn prop(label: &'static str, value: Value, animated: bool) -> Prop {
    Prop {
        label,
        value,
        animated,
    }
}

fn pair(
    label: &'static str,
    a: (&'static str, String),
    b: (&'static str, String),
    animated: bool,
) -> Prop {
    prop(
        label,
        Value::Pair {
            first: a,
            second: b,
        },
        animated,
    )
}

/// Position, scale, rotation and anchor sampled at `frame`.
fn transform_section(t: &Transform, frame: Frame) -> Section {
    let position = sample(&t.position, frame);
    let scale = sample(&t.scale, frame);
    let anchor = sample(&t.anchor, frame);
    let rotation = sample(&t.rotation, frame);
    Section {
        title: "Transform",
        props: vec![
            pair(
                "Position",
                ("X", fixed(position.x)),
                ("Y", fixed(position.y)),
                !t.position.keys().is_empty(),
            ),
            pair(
                "Scale",
                ("W", percent(scale.x)),
                ("H", percent(scale.y)),
                !t.scale.keys().is_empty(),
            ),
            prop(
                "Rotation",
                Value::Text(format!("{}°", fixed(rotation))),
                !t.rotation.keys().is_empty(),
            ),
            pair(
                "Anchor",
                ("X", fixed(anchor.x)),
                ("Y", fixed(anchor.y)),
                !t.anchor.keys().is_empty(),
            ),
        ],
    }
}

/// The sections shown for `node` at `frame`, or `None` if the node is gone.
#[must_use]
pub fn inspect_node(document: &Document, node: NodeId, frame: Frame) -> Option<Vec<Section>> {
    let node = document.node(node)?;
    let t = node.transform();
    let opacity = sample(&t.opacity, frame).clamp(0.0, 1.0);
    let mut sections = vec![transform_section(t, frame)];

    match node.content() {
        NodeContent::Group => {}
        NodeContent::Rect {
            width,
            height,
            radius,
            paint,
        } => {
            sections.push(Section {
                title: "Shape",
                props: vec![
                    pair("Size", ("W", fixed(*width)), ("H", fixed(*height)), false),
                    prop("Radius", Value::Text(fixed(*radius)), false),
                ],
            });
            sections.push(appearance(
                Some(paint),
                None,
                opacity,
                !t.opacity.keys().is_empty(),
            ));
        }
        NodeContent::Circle { radius, paint } => {
            sections.push(Section {
                title: "Shape",
                props: vec![prop("Radius", Value::Text(fixed(*radius)), false)],
            });
            sections.push(appearance(
                Some(paint),
                None,
                opacity,
                !t.opacity.keys().is_empty(),
            ));
        }
        NodeContent::Text(text) => {
            let align = match text.align {
                TextAlign::Left => "Left",
                TextAlign::Center => "Center",
                TextAlign::Right => "Right",
            };
            sections.push(Section {
                title: "Text",
                props: vec![
                    prop("Content", Value::Text(text.text.replace('\n', " ")), false),
                    prop("Font", Value::Text(font_label(&text.font)), false),
                    pair(
                        "Size",
                        ("", format!("{} px", fixed(text.size))),
                        ("", align.to_owned()),
                        false,
                    ),
                ],
            });
            sections.push(appearance(
                None,
                Some(text.color),
                opacity,
                !t.opacity.keys().is_empty(),
            ));
        }
    }
    if matches!(node.content(), NodeContent::Group) {
        sections.push(Section {
            title: "Layer",
            props: vec![
                prop(
                    "Opacity",
                    Value::Text(percent(opacity)),
                    !t.opacity.keys().is_empty(),
                ),
                prop(
                    "Children",
                    Value::Text(node.children().len().to_string()),
                    false,
                ),
            ],
        });
    }
    Some(sections)
}

/// The requested family, flagged when the renderer substitutes the embedded
/// Roboto (the only font the renderer ships today).
fn font_label(family: &str) -> String {
    if family.eq_ignore_ascii_case("roboto") {
        family.to_owned()
    } else {
        format!("{family} (uses Roboto)")
    }
}

fn appearance(
    paint: Option<&Paint>,
    color: Option<Color>,
    opacity: f64,
    opacity_animated: bool,
) -> Section {
    let mut props = Vec::new();
    if let Some(color) = color {
        props.push(prop("Color", Value::Color(color), false));
    }
    if let Some(paint) = paint {
        props.push(prop(
            "Fill",
            paint.fill.map_or(Value::None, Value::Color),
            false,
        ));
        match paint.stroke {
            Some(stroke) => {
                props.push(prop("Stroke", Value::Color(stroke.color), false));
                props.push(prop(
                    "Width",
                    Value::Text(format!("{} px", fixed(stroke.width))),
                    false,
                ));
            }
            None => props.push(prop("Stroke", Value::None, false)),
        }
    }
    props.push(prop(
        "Opacity",
        Value::Text(percent(opacity)),
        opacity_animated,
    ));
    Section {
        title: "Appearance",
        props,
    }
}

/// The sections shown for the composition when nothing is selected.
#[must_use]
pub fn inspect_comp(composition: &Composition, layers: usize) -> Vec<Section> {
    let rate = composition.frame_rate();
    let frames = composition.duration();
    let seconds = frames as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
    let fps = if rate.denominator() == 1 {
        rate.numerator().to_string()
    } else {
        format!("{:.3}", rate.as_f64())
    };
    vec![
        Section {
            title: "Composition",
            props: vec![
                prop("Name", Value::Text(composition.name().to_owned()), false),
                pair(
                    "Size",
                    ("W", composition.width().to_string()),
                    ("H", composition.height().to_string()),
                    false,
                ),
                pair(
                    "Timing",
                    ("", format!("{fps} fps")),
                    ("", format!("{} s", fixed(seconds))),
                    false,
                ),
                prop("Frames", Value::Text(frames.to_string()), false),
                prop("Background", Value::Color(composition.background()), false),
            ],
        },
        Section {
            title: "Scene",
            props: vec![prop("Layers", Value::Text(layers.to_string()), false)],
        },
    ]
}

/// A one-line kind label for the inspector header.
#[must_use]
pub const fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Group => "Group layer",
        NodeKind::Rect => "Rectangle layer",
        NodeKind::Circle => "Circle layer",
        NodeKind::Text => "Text layer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    fn hello() -> Document {
        motioner_dsl::run(HELLO, &motioner_dsl::Limits::default()).unwrap()
    }

    fn find<'a>(sections: &'a [Section], title: &str, label: &str) -> &'a Prop {
        sections
            .iter()
            .find(|s| s.title == title)
            .and_then(|s| s.props.iter().find(|p| p.label == label))
            .unwrap()
    }

    #[test]
    fn title_values_follow_the_frame() {
        let document = hello();
        let title = document.roots()[0];
        let start = inspect_node(&document, title, 0).unwrap();
        let settled = inspect_node(&document, title, 120).unwrap();
        let position = find(&start, "Transform", "Position");
        assert!(position.animated);
        assert_eq!(
            position.value,
            Value::Pair {
                first: ("X", "960".to_owned()),
                second: ("Y", "520".to_owned())
            }
        );
        assert_eq!(
            find(&settled, "Transform", "Position").value,
            Value::Pair {
                first: ("X", "960".to_owned()),
                second: ("Y", "480".to_owned())
            }
        );
        assert_eq!(
            find(&start, "Appearance", "Opacity").value,
            Value::Text("0%".to_owned())
        );
        assert_eq!(
            find(&settled, "Appearance", "Opacity").value,
            Value::Text("100%".to_owned())
        );
        assert_eq!(
            find(&settled, "Text", "Content").value,
            Value::Text("Motioner".to_owned())
        );
        assert_eq!(
            find(&settled, "Text", "Font").value,
            Value::Text("Inter (uses Roboto)".to_owned())
        );
        assert!(!find(&settled, "Transform", "Rotation").animated);
    }

    #[test]
    fn shapes_report_size_and_paint() {
        let document = hello();
        let rows = crate::outline_rows(&document);
        let rect = rows.iter().find(|r| r.kind == NodeKind::Rect).unwrap();
        let sections = inspect_node(&document, rect.node, 0).unwrap();
        assert_eq!(
            find(&sections, "Shape", "Size").value,
            Value::Pair {
                first: ("W", "96".to_owned()),
                second: ("H", "96".to_owned())
            }
        );
        assert_eq!(
            find(&sections, "Appearance", "Fill").value,
            Value::Color(Color::rgb(0x2a, 0x2a, 0x40))
        );
        assert_eq!(
            find(&sections, "Appearance", "Width").value,
            Value::Text("4 px".to_owned())
        );
    }

    #[test]
    fn composition_summary() {
        let document = hello();
        let sections = inspect_comp(document.composition(), 10);
        assert_eq!(
            find(&sections, "Composition", "Frames").value,
            Value::Text("300".to_owned())
        );
        assert_eq!(
            find(&sections, "Composition", "Timing").value,
            Value::Pair {
                first: ("", "60 fps".to_owned()),
                second: ("", "5 s".to_owned())
            }
        );
    }

    #[test]
    fn hex_codes() {
        assert_eq!(hex(Color::rgb(0x0b, 0x0b, 0x12)), "#0B0B12");
        assert_eq!(
            hex(Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            }),
            "#01020304"
        );
    }

    #[test]
    fn a_missing_node_inspects_to_none() {
        let document = hello();
        let other = motioner_dsl::run(
            "comp(name=\"A\", duration=1.0)\n",
            &motioner_dsl::Limits::default(),
        )
        .unwrap();
        assert!(other.is_empty());
        assert!(inspect_node(&other, document.roots()[0], 0).is_none());
    }
}
