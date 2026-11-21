//! D-0043 contract: every implemented tool answers with a result shaped like
//! its published schema.

use std::path::PathBuf;
use std::sync::Arc;

use motioner_core::TextMetrics;
use motioner_dsl::{execute_detailed, parse, Limits};
use motioner_render::{CpuTextMetrics, FontSet, ImageSet};
use motioner_tools::{catalogue, exec, result_schema, Runtime, ToolError, TOOLS};
use serde_json::{json, Value};

const SOURCE: &str = r##"
comp(name="Tools", width=1280, height=720, fps=60, duration=3.0, background="#101418")
t = text("Hello tools", "Inter", 72, "#FFFFFF", "center")
t.id("title")
t.pos = (640, 300)
t.key("opacity", 0.0, 0.0)
t.key("opacity", 1.0, 1.0, ease="quad_out")
r = rect(300, 160, fill="#5B8CFF")
r.id("card")
r.pos = (640, 470)
"##;

fn host() -> exec::FixtureHost {
    let program = parse(SOURCE).unwrap();
    let execution = execute_detailed(&program, &Limits::default()).unwrap();
    let fonts = FontSet::bundled();
    let metrics: Arc<dyn TextMetrics> = Arc::new(CpuTextMetrics::new(fonts.clone()));
    exec::FixtureHost {
        program: PathBuf::from("examples/hello.motion"),
        source: SOURCE.to_owned(),
        document: execution.document,
        origins: Some(execution.origins),
        images: ImageSet::new(),
        fonts,
        metrics,
    }
}

fn validate(schema: &Value, result: &Value) -> Result<(), String> {
    let kind = schema
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("object");
    match kind {
        "object" => {
            let object = result
                .as_object()
                .ok_or_else(|| format!("expected an object, got {result}"))?;
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for name in required {
                    let name = name.as_str().unwrap_or_default();
                    if !object.contains_key(name) {
                        return Err(format!("missing required field {name}"));
                    }
                }
            }
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, property) in properties {
                    let Some(value) = object.get(name) else {
                        continue;
                    };
                    let expected = property.get("type").and_then(Value::as_str);
                    let ok = match expected {
                        Some("integer") => value.is_i64() || value.is_u64(),
                        Some("number") => value.is_number(),
                        Some("string") => value.is_string(),
                        Some("boolean") => value.is_boolean(),
                        Some("array") => value.is_array(),
                        Some("object") => value.is_object(),
                        _ => true,
                    };
                    if !ok {
                        return Err(format!("field {name} should be {expected:?}, got {value}"));
                    }
                }
            }
            Ok(())
        }
        "array" => result
            .as_array()
            .map(|_| ())
            .ok_or("expected an array".to_owned()),
        _ => Ok(()),
    }
}

fn out_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("motioner_tool_contracts");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[allow(clippy::needless_pass_by_value)] // every call site passes a fresh JSON literal
fn call(host: &mut exec::FixtureHost, name: &str, args: Value) -> Value {
    let mut runtime = Runtime::default();
    let output = exec::execute(name, &args, &mut runtime, host, &out_dir())
        .unwrap_or_else(|error| panic!("{name} failed: {error}"));
    validate(&result_schema(name), &output.result)
        .unwrap_or_else(|error| panic!("{name} result does not match its schema: {error}"));
    output.result
}

#[test]
fn read_and_analysis_tools_match_their_schemas() {
    let mut host = host();
    call(&mut host, "project_summary", json!({}));
    call(&mut host, "outline", json!({}));
    let node = call(&mut host, "get_node", json!({"id": "title", "frame": 30}));
    assert_eq!(node["kind"], "text");
    call(&mut host, "read_program", json!({"start_line": 1}));
    let search = call(&mut host, "search_code", json!({"query": "title"}));
    assert!(!search["matches"].as_array().unwrap().is_empty());
    call(&mut host, "selection", json!({}));
    call(&mut host, "std_reference", json!({"name": "enter"}));
    call(&mut host, "find_examples", json!({"query": "hello"}));
    let description = call(&mut host, "describe_frame", json!({"frame": 30}));
    assert!(description["elements"].as_array().unwrap().len() >= 2);
    let check = call(&mut host, "check_design", json!({}));
    assert!(check["errors"].is_i64() || check["errors"].is_u64());
    call(&mut host, "motion_report", json!({}));
    call(&mut host, "layout_report", json!({"frame": 30}));
    call(&mut host, "legibility_report", json!({}));
    let hits = call(
        &mut host,
        "hit_test",
        json!({"frame": 30, "x": 640, "y": 300}),
    );
    assert!(!hits["hits"].as_array().unwrap().is_empty());
    call(&mut host, "code_for_node", json!({"id": "title"}));
    call(&mut host, "node_for_code", json!({"line": 5}));
    let samples = call(
        &mut host,
        "sample",
        json!({"id": "title", "property": "opacity", "frames": [0, 30, 60]}),
    );
    assert_eq!(samples["values"].as_array().unwrap().len(), 3);
    call(&mut host, "list_fonts", json!({}));
    let themes = call(&mut host, "list_themes", json!({}));
    assert_eq!(themes["themes"].as_array().unwrap().len(), 10);
    call(&mut host, "search_icons", json!({"query": "arrow"}));
}

#[test]
fn mutating_tools_fail_typed_without_a_real_host() {
    let mut host = host();
    let mut runtime = Runtime::default();
    for (name, args) in [
        ("run_commands", json!({"commands": ["select title"]})),
        ("run_program", json!({})),
        ("undo", json!({})),
        ("checkpoint", json!({"name": "x"})),
    ] {
        let error = exec::execute(name, &args, &mut runtime, &mut host, &out_dir())
            .expect_err("a fixture host cannot mutate");
        assert_eq!(error.kind(), "requires_harness", "{name}: {error}");
    }
}

#[test]
fn harness_only_and_unknown_tools_are_typed_errors() {
    let mut host = host();
    let mut runtime = Runtime::default();
    for name in [
        "ask_user",
        "ask_user_visual",
        "show_plan",
        "request_approval",
        "remember_preference",
        "view_editor",
    ] {
        let error = exec::execute(name, &json!({}), &mut runtime, &mut host, &out_dir())
            .expect_err("harness-only tools must fail in the CLI");
        assert_eq!(error.kind(), "requires_harness", "{name}");
    }
    let error = exec::execute("nope", &json!({}), &mut runtime, &mut host, &out_dir())
        .expect_err("unknown tool");
    assert!(matches!(error, ToolError::Unknown(_)));
}

#[test]
fn every_catalogue_tool_is_reachable_or_marked_unimplemented() {
    for tool in TOOLS {
        assert!(!tool.name.is_empty());
        assert!(catalogue::spec(tool.name).is_some());
        if !tool.implemented {
            continue;
        }
        // The dispatcher knows the name: calling it with empty arguments must
        // fail on arguments, never on an unknown name.
        let mut host = host();
        let mut runtime = Runtime::default();
        match exec::execute(tool.name, &json!({}), &mut runtime, &mut host, &out_dir()) {
            Ok(_) => {}
            Err(error) => assert_ne!(
                error.kind(),
                "unknown_tool",
                "{} is published but not dispatched",
                tool.name
            ),
        }
    }
}

#[test]
fn the_schema_has_one_entry_per_tool_and_keeps_command_tools() {
    let catalogue = catalogue::catalogue_json();
    assert_eq!(catalogue.len(), TOOLS.len());
    let names: Vec<&str> = catalogue
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();
    assert!(names.contains(&"render_still"));
    assert!(names.contains(&"run_commands"));
    for tool in &catalogue {
        assert!(tool.get("result").is_some());
        assert!(tool.get("example").is_some());
    }
}

#[test]
fn the_example_calls_parse_as_catalogue_names() {
    for tool in TOOLS {
        let name = tool
            .example
            .split_once('(')
            .map(|(name, _)| name)
            .unwrap_or_default();
        assert_eq!(
            name, tool.name,
            "example of {} names another tool",
            tool.name
        );
    }
}

#[test]
fn render_tools_write_real_images() {
    let _lock = motioner_testkit::gpu::lock();
    let mut host = host();
    let still = call(
        &mut host,
        "render_still",
        serde_json::json!({"frame": 30, "size": 256}),
    );
    let path = still["images"][0].as_str().unwrap();
    assert!(std::path::Path::new(path).is_file(), "{path}");
    let film = call(
        &mut host,
        "render_filmstrip",
        serde_json::json!({"start": 0, "end": 120, "count": 4}),
    );
    assert!(std::path::Path::new(film["images"][0].as_str().unwrap()).is_file());
    let sheet = call(
        &mut host,
        "render_contact_sheet",
        serde_json::json!({"count": 4}),
    );
    assert!(std::path::Path::new(sheet["images"][0].as_str().unwrap()).is_file());
    let zoom = call(
        &mut host,
        "render_zoom",
        serde_json::json!({"frame": 30, "region": [500, 200, 300, 200], "size": 256}),
    );
    assert!(std::path::Path::new(zoom["images"][0].as_str().unwrap()).is_file());
    let diff = call(
        &mut host,
        "compare",
        serde_json::json!({"a": 0, "b": 60, "mode": "diff"}),
    );
    assert!(diff["changed_regions"].is_array());
    let marks = call(
        &mut host,
        "render_still",
        serde_json::json!({"frame": 30, "overlay": "marks", "size": 256}),
    );
    assert!(!marks["legend"].as_array().unwrap().is_empty());
}
