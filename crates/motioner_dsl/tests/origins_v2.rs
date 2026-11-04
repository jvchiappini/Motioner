//! Origins v2: statements, blocks, bindings, loops and calls (D-0036).

use motioner_dsl::{
    execute_detailed, ExpressionOrigin, IterationValue, Limits, OriginKind, ValueOrigins,
};

fn origins(source: &str) -> ValueOrigins {
    execute_detailed(&motioner_dsl::parse(source).unwrap(), &Limits::default())
        .unwrap()
        .origins
}

fn text(source: &str, span: motioner_dsl::Span) -> &str {
    &source[span.start..span.end]
}

const TOP_LEVEL: &str = r##"comp(name="T", duration=1.0)
c = circle(40, fill="#ffffff")
c.pos = (200, 300)
c.fill = "#ff0000"
"##;

#[test]
fn a_top_level_node_records_its_statement_block_and_binding() {
    let origins = origins(TOP_LEVEL);
    assert_eq!(origins.nodes().len(), 1);
    let node = &origins.nodes()[0];
    assert!(text(TOP_LEVEL, node.creation).starts_with("circle("));
    assert!(
        text(TOP_LEVEL, node.statement).starts_with("c = circle("),
        "the creation statement includes the assignment"
    );
    assert_eq!(node.block.indent, 0);
    assert!(text(TOP_LEVEL, node.block.span).contains("c.fill"));
    assert!(node.site.loops.is_empty());
    assert!(node.site.calls.is_empty());

    assert_eq!(node.bindings.len(), 1);
    let binding = node.bindings[0];
    assert_eq!(origins.name(binding.name), "c");
    assert!(!binding.reassigned_later);
    assert!(text(TOP_LEVEL, binding.statement).starts_with("c = circle("));
}

#[test]
fn every_property_write_records_its_statement_and_expression_class() {
    let origins = origins(TOP_LEVEL);
    let node = origins.nodes()[0].node;
    let pos = origins.set_origin(node, "pos").unwrap();
    assert_eq!(pos.kind, OriginKind::Set);
    assert!(text(TOP_LEVEL, pos.statement).starts_with("c.pos ="));
    assert_eq!(
        pos.expression,
        ExpressionOrigin::Literal(motioner_dsl::Literal::Pair)
    );
    assert_eq!(pos.block.indent, 0);

    let fill = origins.set_origin(node, "fill").unwrap();
    assert!(text(TOP_LEVEL, fill.statement).starts_with("c.fill ="));
    assert_eq!(
        fill.expression,
        ExpressionOrigin::Literal(motioner_dsl::Literal::Text)
    );
    // The constructor argument is still recorded as a static value.
    let radius = origins.set_origin(node, "radius").unwrap();
    assert_eq!(
        radius.expression,
        ExpressionOrigin::Literal(motioner_dsl::Literal::Number)
    );
    assert!(text(TOP_LEVEL, radius.statement).starts_with("c = circle("));
}

#[test]
fn a_reassigned_binding_is_flagged_for_the_first_node() {
    let source = r#"comp(name="R", duration=1.0)
c = circle(10)
c = rect(20, 20)
"#;
    let origins = origins(source);
    assert_eq!(origins.nodes().len(), 2);
    let first = &origins.nodes()[0];
    let second = &origins.nodes()[1];
    assert_eq!(origins.name(first.bindings[0].name), "c");
    assert!(first.bindings[0].reassigned_later);
    assert_eq!(origins.name(second.bindings[0].name), "c");
    assert!(!second.bindings[0].reassigned_later);
}

#[test]
fn a_loop_records_its_span_variable_value_and_body_for_every_iteration() {
    let source = r#"comp(name="L", duration=1.0)
for i in range(4):
    dot = circle(40)
    dot.pos = (100 + i * 20, 400)
"#;
    let origins = origins(source);
    assert_eq!(origins.nodes().len(), 4);
    for (index, node) in origins.nodes().iter().enumerate() {
        assert_eq!(node.site.loops.len(), 1);
        let loop_origin = &node.site.loops[0];
        assert_eq!(loop_origin.index, i64::try_from(index).unwrap());
        assert_eq!(
            loop_origin.value,
            Some(IterationValue::Number(f64::from(
                u32::try_from(index).unwrap()
            )))
        );
        assert_eq!(origins.name(loop_origin.variables[0]), "i");
        assert!(text(source, loop_origin.span).starts_with("for i in range(4):"));
        assert_eq!(text(source, loop_origin.iterable), "range(4)");
        assert_eq!(loop_origin.indent, 4);
        assert!(text(source, loop_origin.body).contains("dot.pos"));
        assert_eq!(node.block.indent, 4);

        let pos = origins.set_origin(node.node, "pos").unwrap();
        assert_eq!(pos.expression, ExpressionOrigin::Computed);
        assert_eq!(pos.site.loops[0].index, loop_origin.index);
    }
}

#[test]
fn nested_loops_record_every_level_outermost_first() {
    let source = r#"comp(name="N", duration=1.0)
for row in range(2):
    for col in range(3):
        d = circle(5)
"#;
    let origins = origins(source);
    assert_eq!(origins.nodes().len(), 6);
    for node in origins.nodes() {
        let loops = &node.site.loops;
        assert_eq!(loops.len(), 2);
        assert_eq!(origins.name(loops[0].variables[0]), "row");
        assert_eq!(origins.name(loops[1].variables[0]), "col");
        assert_eq!(loops[1].indent, 8);
    }
}

#[test]
fn a_function_call_records_parameters_the_definition_and_the_returned_node() {
    let source = r##"comp(name="F", duration=1.0)
def badge(x):
    b = circle(x)
    b.fill = "#ffffff"
    return b
a = badge(10)
d = badge(20)
"##;
    let origins = origins(source);
    assert_eq!(origins.nodes().len(), 2);
    let expected: [&[&str]; 2] = [&["b", "a"], &["b", "d"]];
    for (node, expected_names) in origins.nodes().iter().zip(expected) {
        assert_eq!(node.site.calls.len(), 1);
        let call = &node.site.calls[0];
        assert_eq!(origins.name(call.function), "badge");
        assert_eq!(call.parameters.len(), 1);
        assert_eq!(origins.name(call.parameters[0].name), "x");
        assert!(call.parameters[0].default.is_none());
        assert!(call.returns_node, "badge() returns the node it made");
        assert!(text(source, call.definition).starts_with("def badge(x)"));
        assert!(text(source, call.call_site).starts_with("badge("));

        let radius = origins.set_origin(node.node, "radius").unwrap();
        assert_eq!(
            radius.expression,
            ExpressionOrigin::Parameter {
                name: call.parameters[0].name
            }
        );
        assert_eq!(radius.literal, None, "a parameter is not a literal");
        // The node is bound inside badge() and again at the call site.
        assert_eq!(node.bindings.len(), 2);
        let names: Vec<&str> = node
            .bindings
            .iter()
            .map(|binding| origins.name(binding.name))
            .collect();
        assert_eq!(names, expected_names.to_vec());
        // The constructor argument at each call site is a literal.
        let fill = origins.set_origin(node.node, "fill").unwrap();
        assert_eq!(
            fill.expression,
            ExpressionOrigin::Literal(motioner_dsl::Literal::Text)
        );
    }
}

#[test]
fn a_loop_variable_is_classified_in_an_assignment() {
    let source = r#"comp(name="V", duration=1.0)
for i in range(2):
    dot = circle(10)
    dot.opacity = i
"#;
    let origins = origins(source);
    for node in origins.nodes() {
        let x = origins.set_origin(node.node, "opacity").unwrap();
        let ExpressionOrigin::LoopVariable { name } = x.expression else {
            panic!("expected a loop variable, got {:?}", x.expression);
        };
        assert_eq!(origins.name(name), "i");
    }
}

#[test]
fn a_key_records_its_statement_and_value_expression() {
    let source = r#"comp(name="K", duration=1.0)
c = circle(10)
c.key("opacity", 0.0, 0.0)
c.key("opacity", 1.0, 1.0, ease="quad_out")
"#;
    let origins = origins(source);
    let node = origins.nodes()[0].node;
    let key = origins.key_origin(node, "opacity", 60).unwrap();
    assert!(text(source, key.statement).starts_with("c.key("));
    assert!(key.ease.is_some());
    assert_eq!(
        key.expression,
        ExpressionOrigin::Literal(motioner_dsl::Literal::Number)
    );
}
