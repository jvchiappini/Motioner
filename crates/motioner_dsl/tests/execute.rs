//! End-to-end tests: MDF source in, document (or diagnostic) out.

use motioner_core::{Document, Ease, NodeContent, Vec2};
use motioner_dsl::{run, ErrorCategory, Limits};

const HEADER: &str = "comp(name=\"T\", width=200, height=100, fps=60, duration=2.0)\n";

fn build(body: &str) -> Document {
    let source = format!("{HEADER}{body}");
    run(&source, &Limits::default()).unwrap_or_else(|e| panic!("{}", e.render(&source, "test")))
}

fn fail(body: &str) -> motioner_dsl::Diagnostic {
    let source = format!("{HEADER}{body}");
    run(&source, &Limits::default()).expect_err("program should fail")
}

/// Runs `body` and returns the static x position of the first node, which
/// tests use to observe computed values.
fn first_x(body: &str) -> f64 {
    let document = build(body);
    let first = document.nodes().next().expect("a node");
    first.transform().position.value().x
}

#[test]
fn hello_example_builds_the_expected_document() {
    let source = include_str!("../../../examples/hello.motion");
    let document = run(source, &Limits::default()).unwrap();
    let comp = document.composition();
    assert_eq!(
        (comp.width(), comp.height(), comp.duration()),
        (1920, 1080, 300)
    );
    assert_eq!(comp.frame_rate().numerator(), 60);

    let texts = document
        .nodes()
        .filter(|n| matches!(n.content(), NodeContent::Text(_)))
        .count();
    let circles = document
        .nodes()
        .filter(|n| matches!(n.content(), NodeContent::Circle { .. }))
        .count();
    assert_eq!((texts, circles), (1, 5));

    let title = document.nodes().next().unwrap();
    let keys = title.transform().opacity.keys();
    assert_eq!(keys.len(), 2);
    assert_eq!((keys[1].frame, keys[1].ease), (48, Ease::QuadOut));

    let last_dot = document.nodes().nth(5).unwrap();
    // `key("scale", ...)` keys both axes, so each axis shows the same keys.
    let scale = &last_dot.transform().scale;
    let x_keys = scale.axis(motioner_core::Axis::X).keys();
    let y_keys = scale.axis(motioner_core::Axis::Y).keys();
    assert_eq!(x_keys[0].frame, 36);
    assert_eq!((x_keys[1].value, y_keys[1].value), (1.0, 1.0));
    assert_eq!(scale.value(), Vec2::splat(1.0));
}

#[test]
fn execution_is_deterministic() {
    let source = include_str!("../../../examples/hello.motion");
    let a = run(source, &Limits::default()).unwrap();
    let b = run(source, &Limits::default()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn arithmetic_and_builtins_follow_the_spec() {
    let cases = [
        ("7 / 2", 3.5),
        ("7 // 2", 3.0),
        ("-7 // 2", -4.0),
        ("-7 % 3", 2.0),
        ("2 + 3 * 4 - 1", 13.0),
        ("len([1, 2, 3]) + len(\"abc\") + len(range(0, 10, 3))", 10.0),
        ("min(4, 2, 8) + max([1, 9, 3])", 11.0),
        ("abs(-3) + round(2.5) + round(3.5) + round(1.26, 1)", 10.3),
        ("int(\"42\") + int(3.9) + float(\"0.5\")", 45.5),
        ("lerp(10, 20, 0.25) + clamp(15, 0, 10)", 22.5),
        ("round(sin(0) + cos(0))", 1.0),
        ("len(str(12.5) + str(True) + str(None))", 12.0),
    ];
    for (expr, expected) in cases {
        let x = first_x(&format!("n = rect(1, 1)\nn.pos = ({expr}, 0)\n"));
        assert!(
            (x - expected).abs() < 1e-9,
            "{expr} = {x}, expected {expected}"
        );
    }
}

// Exact comparison is intended: the expected values are exactly representable.
#[allow(clippy::float_cmp)]
#[test]
fn control_flow_functions_and_scopes() {
    let body = "\
total = 0
def add_until(limit, skip):
    acc = 0
    for i in range(100):
        if i == skip:
            continue
        elif i >= limit:
            break
        else:
            acc += i
    return acc

def fact(n):
    if n <= 1:
        return 1
    return n * fact(n - 1)

total = add_until(5, 2) + fact(5)
items = [1, 2, 3]
items[1] = 20
items[-1] *= 2
for word in [\"a\", \"bc\"]:
    total += len(word)
n = rect(1, 1)
n.pos = (total + items[1] + items[2], 0)
";
    // add_until = 0+1+3+4 = 8, fact(5) = 120, words = 3, items = 20 + 6.
    assert_eq!(first_x(body), 157.0);
}

// Exact comparison is intended: the expected values are exactly representable.
#[allow(clippy::float_cmp)]
#[test]
fn logical_operators_short_circuit_and_return_operands() {
    let body = "\
def boom():
    return undefined_name
a = 0 and boom()
b = 5 or boom()
c = 0
if not (1 == 2) and 3 != 4:
    c = 1
n = rect(1, 1)
n.pos = (a + b + c, 0)
";
    assert_eq!(first_x(body), 6.0);
}

#[test]
fn functions_do_not_leak_locals() {
    let error = fail("def f():\n    secret = 1\n    return secret\nf()\nx = secret\n");
    assert_eq!(error.category, ErrorCategory::Name);
    assert_eq!(error.span.line, 6);
}

#[test]
fn every_blend_mode_parses_and_round_trips() {
    for mode in motioner_core::BlendMode::ALL {
        let name = mode.name();
        let document = build(&format!("n = rect(1, 1)\nn.blend = \"{name}\"\n"));
        let node = document.node(document.roots()[0]).unwrap();
        assert_eq!(node.blend(), mode, "{name}");
    }
}

// Exact comparison is intended: the expected values are exactly representable.
#[allow(clippy::float_cmp)]
#[test]
fn groups_and_properties_build_the_tree() {
    let document = build(
        "\
g = group()
a = rect(10, 20, radius=2, fill=\"#ff000080\", stroke=\"#00ff00\", stroke_width=3)
b = text(\"hi\", size=30, align=\"right\")
g.add(a, b)
g.rotation = 45
g.scale = (2, 3)
g.anchor = (5, 5)
g.opacity = 0.5
g.blend = \"normal\"
g.pos = (g.pos[0] + 1, g.pos[1] + 1)
",
    );
    let group = document.node(document.roots()[0]).unwrap();
    assert_eq!(document.roots().len(), 1);
    assert_eq!(group.children().len(), 2);
    let transform = group.transform();
    assert_eq!(transform.rotation.value(), 45.0);
    assert_eq!(transform.scale.value(), Vec2::new(2.0, 3.0));
    assert_eq!(transform.opacity.value(), 0.5);
    let NodeContent::Rect { paint, .. } = document.node(group.children()[0]).unwrap().content()
    else {
        panic!("expected rect");
    };
    assert_eq!(paint.fill.value().unwrap().a, 0x80);
    assert_eq!(paint.stroke_width.value(), 3.0);
}

#[test]
fn runtime_errors_carry_category_span_and_stack() {
    let error =
        fail("def inner():\n    return 1 + \"a\"\ndef outer():\n    return inner()\nouter()\n");
    assert_eq!(error.category, ErrorCategory::Type);
    assert_eq!((error.span.line, error.span.column), (3, 12));
    let functions: Vec<_> = error.stack.iter().map(|f| f.function.as_str()).collect();
    assert_eq!(functions, ["outer", "inner"]);
    assert!(error.to_string().contains("in outer() called at 6:1"));
}

#[test]
fn scene_api_rejects_invalid_use() {
    for (body, category) in [
        ("n = rect(1)\n", ErrorCategory::Type),
        ("n = rect(-1, 1)\n", ErrorCategory::Value),
        ("n = rect(1, 1, colour=\"#fff\")\n", ErrorCategory::Type),
        ("n = circle(5, fill=\"red\")\n", ErrorCategory::Value),
        ("n = text(5)\n", ErrorCategory::Type),
        ("n = text(\"a\", align=\"middle\")\n", ErrorCategory::Value),
        ("n = rect(1, 1)\nn.pos = 3\n", ErrorCategory::Type),
        ("n = rect(1, 1)\nn.color = 3\n", ErrorCategory::Name),
        (
            "n = rect(1, 1)\nn.blend = \"wobble\"\n",
            ErrorCategory::Unsupported,
        ),
        (
            "n = rect(1, 1)\nn.key(\"opacity\", 0, 1, ease=\"wobble\")\n",
            ErrorCategory::Value,
        ),
        (
            "n = rect(1, 1)\nn.key(\"size\", 0, 1)\n",
            ErrorCategory::Value,
        ),
        ("n = rect(1, 1)\nn.add(n)\n", ErrorCategory::Value),
        ("g = group()\ng.add(g)\n", ErrorCategory::Value),
        ("comp(name=\"again\")\n", ErrorCategory::Value),
        ("x = [1][5]\n", ErrorCategory::Value),
        ("x = 1 / 0\n", ErrorCategory::Value),
        ("x = 3(1)\n", ErrorCategory::Type),
    ] {
        let error = fail(body);
        assert_eq!(error.category, category, "{body}: {error}");
        assert!(error.span.line >= 2, "{body}: {error}");
    }
}

#[test]
fn programs_must_create_a_composition_first() {
    let limits = Limits::default();
    let error = run("x = 1\n", &limits).unwrap_err();
    assert_eq!(error.category, ErrorCategory::Value);
    let error = run("n = rect(1, 1)\n", &limits).unwrap_err();
    assert!(error.message.contains("comp("), "{error}");
    let error = run("comp(name=\"x\", fps=12.5)\n", &limits).unwrap_err();
    assert_eq!(error.category, ErrorCategory::Value);
    let ntsc = run("comp(name=\"x\", fps=29.97)\n", &limits).unwrap();
    assert_eq!(ntsc.composition().frame_rate().denominator(), 1001);
}

#[test]
fn limits_stop_runaway_programs() {
    let tight = Limits {
        max_steps: 1_000,
        max_call_depth: 16,
        max_nodes: 10,
        max_string_len: 64,
        max_list_len: 8,
        max_value_depth: 100,
    };
    for (body, needle) in [
        ("for i in range(1000000):\n    x = i\n", "step limit"),
        ("def f(n):\n    return f(n + 1)\nf(0)\n", "call depth"),
        ("for i in range(20):\n    rect(1, 1)\n", "node limit"),
        (
            "s = \"x\"\nfor i in range(10):\n    s = s + s\n",
            "string length",
        ),
        (
            "l = [0]\nfor i in range(10):\n    l = l + l\n",
            "list length",
        ),
    ] {
        let source = format!("{HEADER}{body}");
        let error = run(&source, &tight).unwrap_err();
        assert_eq!(error.category, ErrorCategory::Limit, "{body}: {error}");
        assert!(error.message.contains(needle), "{body}: {error}");
    }
}

#[test]
fn default_call_depth_does_not_overflow_the_stack() {
    let error = fail("def f(n):\n    return f(n + 1)\nf(0)\n");
    assert_eq!(error.category, ErrorCategory::Limit);
    assert_eq!(error.stack.len(), Limits::default().max_call_depth);
}

#[test]
fn content_fields_are_animatable() {
    let document = build(
        "cover = rect(20, 10, 4, fill=\"#FF0000\")\n\
         cover.id(\"cover\")\n\
         cover.key(\"fill\", 0.0, \"#FF0000\")\n\
         cover.key(\"fill\", 1.0, \"#00FF00\", ease=\"cubic_out\")\n\
         cover.key(\"width\", 1.0, 40)\n\
         cover.key(\"stroke\", 0.5, \"#0000FF\")\n\
         cover.key(\"stroke_width\", 0.5, 3)\n",
    );
    let cover = document.find("cover").expect("the rect");
    let content = document.node(cover).unwrap().content();
    assert_eq!(
        content
            .channel(motioner_core::ContentField::Fill)
            .unwrap()
            .key_frames(),
        vec![0, 60]
    );
    assert_eq!(
        content
            .channel(motioner_core::ContentField::Width)
            .unwrap()
            .key_frames(),
        vec![60]
    );
    // Attributes read the static value, untouched by keyframes.
    assert_eq!(
        content.static_value(motioner_core::ContentField::Width),
        Some(motioner_core::ContentValue::Number(20.0))
    );
}

#[test]
fn discrete_content_fields_reject_keys() {
    let error = fail("t = text(\"hi\")\nt.key(\"text\", 0.0, \"bye\")\n");
    assert_eq!(error.category, ErrorCategory::Value);
    assert!(error.message.contains("discrete"), "{error}");
    let error = fail("c = circle(4)\nc.key(\"width\", 0.0, 9)\n");
    assert_eq!(error.category, ErrorCategory::Value);
}

#[test]
fn content_attribute_reads_and_writes_use_the_channels() {
    let document = build(
        "r = rect(10, 20, 3, fill=\"#112233\", stroke=\"#445566\", stroke_width=2)\n\
         r.fill = \"#ABCDEF\"\n",
    );
    let r = document.nodes().next().unwrap();
    assert_eq!(
        r.content().static_value(motioner_core::ContentField::Fill),
        Some(motioner_core::ContentValue::Color(
            motioner_core::Color::parse_hex("#ABCDEF").ok()
        ))
    );
    assert_eq!(
        r.content()
            .static_value(motioner_core::ContentField::StrokeWidth),
        Some(motioner_core::ContentValue::Number(2.0))
    );
}

#[test]
fn path_constructors_build_closed_and_open_geometry() {
    let document = build(
        "p = path([(0, 0), (100, 0, 0, 0, 60, -40), (200, 0)], closed=False,\n\
         \x20        stroke=\"#FF5A6E\", stroke_width=8, cap=\"square\", join=\"miter\")\n\
         p.id(\"p\")\n\
         tri = polygon(3, 40, fill=\"#00FF00\")\n\
         tri.id(\"tri\")\n\
         star = star(5, 20, 50, stroke=\"#FFFFFF\")\n\
         star.id(\"star\")\n\
         a = arc(30, 0, 90, stroke=\"#5CDBC5\", cap=\"butt\")\n\
         a.id(\"arc\")\n\
         d = path([(0, 0), (10, 10)], closed=True)\n\
         d.id(\"d\")\n",
    );
    let p = document.find("p").unwrap();
    let NodeContent::Path {
        geometry, paint, ..
    } = document.node(p).unwrap().content()
    else {
        panic!("p is a path");
    };
    assert_eq!(geometry.points().len(), 3);
    assert!(!geometry.closed());
    assert_eq!(geometry.cap(), motioner_core::LineCap::Square);
    assert_eq!(geometry.join(), motioner_core::LineJoin::Miter);
    assert!(geometry.points()[1].has_handles());
    assert!((paint.stroke_width.value() - 8.0).abs() < 1e-9);

    let tri = document.find("tri").unwrap();
    let NodeContent::Path { geometry, .. } = document.node(tri).unwrap().content() else {
        panic!("tri is a path");
    };
    assert!(geometry.closed());
    assert_eq!(geometry.points().len(), 3);

    let star = document.find("star").unwrap();
    let NodeContent::Path { geometry, .. } = document.node(star).unwrap().content() else {
        panic!("star is a path");
    };
    assert_eq!(geometry.points().len(), 10);

    let arc = document.find("arc").unwrap();
    let NodeContent::Path { geometry, .. } = document.node(arc).unwrap().content() else {
        panic!("arc is a path");
    };
    assert!(!geometry.closed());
    assert!(geometry.points().len() >= 2);
    assert_eq!(geometry.cap(), motioner_core::LineCap::Butt);

    let d = document.find("d").unwrap();
    let NodeContent::Path { geometry, .. } = document.node(d).unwrap().content() else {
        panic!("d is a path");
    };
    assert!(geometry.closed());
}

#[test]
fn path_trim_and_dash_are_animatable_channels() {
    let document = build(
        "p = path([(0, 0), (200, 0)], stroke=\"#FFFFFF\")\n\
         p.id(\"p\")\n\
         p.key(\"trim_end\", 0.0, 0.0)\n\
         p.key(\"trim_end\", 1.0, 1.0, ease=\"cubic_out\")\n\
         p.key(\"trim_start\", 0.5, 0.25)\n\
         p.key(\"dash\", 0.0, 10)\n\
         p.key(\"dash_offset\", 0.0, 0)\n\
         p.dash_offset = 12\n",
    );
    let p = document.find("p").unwrap();
    let content = document.node(p).unwrap().content();
    for (field, frames) in [
        (motioner_core::ContentField::TrimEnd, vec![0, 60]),
        (motioner_core::ContentField::TrimStart, vec![30]),
        (motioner_core::ContentField::Dash, vec![0]),
    ] {
        assert_eq!(
            content.channel(field).unwrap().key_frames(),
            frames,
            "{field:?}"
        );
    }
    assert_eq!(
        content.static_value(motioner_core::ContentField::DashOffset),
        Some(motioner_core::ContentValue::Number(12.0))
    );
    assert_eq!(
        content.static_value(motioner_core::ContentField::TrimEnd),
        Some(motioner_core::ContentValue::Number(1.0))
    );
}

#[test]
fn path_rejects_bad_points_and_discrete_keys() {
    let error = fail("p = path([(0, 0)])\n");
    assert_eq!(error.category, ErrorCategory::Value);
    assert!(error.message.contains("two points"), "{error}");
    let error = fail("p = path([(0, 0, 1, 2)])\n");
    assert_eq!(error.category, ErrorCategory::Value);
    let error = fail("p = polygon(4, 10)\np.key(\"cap\", 0.0, \"round\")\n");
    assert_eq!(error.category, ErrorCategory::Value);
    assert!(error.message.contains("discrete"), "{error}");
}

#[test]
fn gradients_build_key_and_override() {
    let document = build(
        "r = rect(200, 120, radius=16, \
         fill=linear_gradient(start=(0, 0), end=(200, 0), \
                              stops=[(0, \"#16121F\"), (0.6, \"#7C5CFF\"), (1, \"#FF5A6E\")]), \
         stroke=radial_gradient(center=(0, 0), radius=80, \
                                stops=[(0, \"#FFFFFF\"), (1, \"#00000000\")]), \
         stroke_width=6)\n\
         r.id(\"card\")\n\
         r.fill_stop1_color = \"#00FF00\"\n\
         r.key(\"fill_stop0_offset\", 1.0, 0.2, ease=\"cubic_out\")\n\
         r.key(\"fill_stop2_offset\", 1.0, 0.9)\n",
    );
    let id = document.find("card").unwrap();
    let content = document.node(id).unwrap().content();
    assert_eq!(
        content.gradient_stop_count(motioner_core::PaintSlot::Fill),
        3
    );
    assert_eq!(
        content.gradient_stop_count(motioner_core::PaintSlot::Stroke),
        2
    );
    let Some(motioner_core::ContentValue::Gradient(Some(gradient))) =
        content.static_value(motioner_core::ContentField::Fill)
    else {
        panic!("the fill is a gradient");
    };
    assert_eq!(
        gradient.kind(),
        motioner_core::GradientKind::Linear {
            start: Vec2::ZERO,
            end: Vec2::new(200.0, 0.0)
        }
    );
    assert_eq!(
        content.static_value(motioner_core::ContentField::FillStopColor(1)),
        Some(motioner_core::ContentValue::Color(Some(
            motioner_core::Color::rgb(0, 255, 0)
        )))
    );
    assert_eq!(
        content
            .channel(motioner_core::ContentField::FillStopOffset(0))
            .unwrap()
            .key_frames(),
        vec![60]
    );
    assert_eq!(
        content
            .channel(motioner_core::ContentField::FillStopOffset(2))
            .unwrap()
            .key_frames(),
        vec![60]
    );
}

#[test]
fn gradient_errors_are_reported() {
    let error = fail("r = rect(10, 10, fill=linear_gradient(start=(0, 0), end=(0, 0), stops=[(0, \"#000000\"), (1, \"#FFFFFF\")]))\n");
    assert_eq!(error.category, ErrorCategory::Value);
    assert!(error.message.contains("different endpoints"), "{error}");
    let error = fail("r = rect(10, 10, fill=linear_gradient(start=(0, 0), end=(10, 0), stops=[(0, \"#000000\")]))\n");
    assert_eq!(error.category, ErrorCategory::Value);
    let error =
        fail("r = rect(10, 10, fill=linear_gradient(start=(0, 0), end=(10, 0), stops=\"nope\"))\n");
    assert_eq!(error.category, ErrorCategory::Type);
    let error = fail("r = rect(10, 10)\nr.key(\"fill\", 0.0, linear_gradient(start=(0, 0), end=(1, 0), stops=[(0, \"#000000\"), (1, \"#FFFFFF\")]))\n");
    assert_eq!(error.category, ErrorCategory::Value);
    assert!(error.message.contains("stops"), "{error}");
}

#[test]
fn effects_parse_update_and_keep_their_stack_order() {
    let document = build(
        "\
n = rect(10, 10)
n.effect(\"drop_shadow\", color=\"#00000080\", offset=(4, 6), blur=8, opacity=0.5)
n.effect(\"blur\", radius=3)
n.effect(\"glow\", color=\"#66E0FF\", radius=10, intensity=1.5)
n.effect(\"blur\", radius=5)
",
    );
    let node = document.node(document.roots()[0]).unwrap();
    let kinds: Vec<&str> = node.effects().iter().map(|e| e.kind().name()).collect();
    assert_eq!(kinds, ["drop_shadow", "blur", "glow"]);
    let blur = node.effect(motioner_core::EffectKind::Blur).unwrap();
    assert!((number_value(blur, motioner_core::EffectParam::Radius) - 5.0).abs() < 1e-9);
    let shadow = node.effect(motioner_core::EffectKind::DropShadow).unwrap();
    assert!((number_value(shadow, motioner_core::EffectParam::Opacity) - 0.5).abs() < 1e-9);
    let glow = node.effect(motioner_core::EffectKind::Glow).unwrap();
    assert!((number_value(glow, motioner_core::EffectParam::Intensity) - 1.5).abs() < 1e-9);
}

#[test]
fn effect_parameters_key_and_override() {
    let document = build(
        "\
n = rect(10, 10)
n.effect(\"blur\", radius=2)
n.key(\"blur.radius\", 1.0, 12)
o = rect(10, 10)
o.effect(\"drop_shadow\")
o.key(\"drop_shadow.offset.x\", 1.0, 20)
",
    );
    let first = document.nodes().next().unwrap();
    let blur = first.effect(motioner_core::EffectKind::Blur).unwrap();
    assert_eq!(
        number_channel(blur, motioner_core::EffectParam::Radius)
            .keys()
            .len(),
        1
    );
    let second = document.nodes().nth(1).unwrap();
    let shadow = second
        .effect(motioner_core::EffectKind::DropShadow)
        .unwrap();
    let Some(motioner_core::EffectChannelRef::Number(offset)) = shadow.channel_ref(
        motioner_core::EffectParam::Offset,
        Some(motioner_core::Axis::X),
    ) else {
        panic!("not a number channel");
    };
    assert_eq!(offset.keys().len(), 1);
}

#[test]
fn effect_errors_are_typed() {
    let error = fail("n = rect(1, 1)\nn.effect(\"wobble\")\n");
    assert_eq!(error.category, ErrorCategory::Unsupported);
    assert!(error.message.contains("drop_shadow"), "{error}");
    let error = fail("n = rect(1, 1)\nn.effect(\"blur\", radius=1, color=\"#FFFFFF\")\n");
    assert_eq!(error.category, ErrorCategory::Type);
    let error = fail("n = rect(1, 1)\nn.effect(\"blur\")\nn.key(\"blur.radius\", 0.0, -1)\n");
    assert_eq!(error.category, ErrorCategory::Value);
}

fn number_channel(
    effect: &motioner_core::Effect,
    param: motioner_core::EffectParam,
) -> &motioner_core::Animatable<f64> {
    match effect.channel_ref(param, None) {
        Some(motioner_core::EffectChannelRef::Number(value)) => value,
        _ => panic!("not a number channel"),
    }
}

fn number_value(effect: &motioner_core::Effect, param: motioner_core::EffectParam) -> f64 {
    number_channel(effect, param).value()
}
