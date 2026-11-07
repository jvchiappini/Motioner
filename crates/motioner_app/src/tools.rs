//! `motioner tools --schema`: the tool list an agent (in-app or external)
//! receives, generated from the command table and a short list of code tools.
//!
//! There is exactly one generator: prompt 13's agent runtime adds descriptors
//! to [`code_tools`], it never writes a second schema. A test fails when a
//! [`CommandKind`] is missing, so the published schema can never drift from
//! the commands the editor actually understands (D-0024, D-0036 section 7).

use motioner_ui::{CommandKind, Json};

/// One tool parameter.
fn param(name: &'static str, kind: &'static str, required: bool) -> Parameter {
    Parameter {
        name: name.to_owned(),
        kind,
        required,
    }
}

/// One tool parameter.
struct Parameter {
    name: String,
    kind: &'static str,
    required: bool,
}

/// One tool descriptor.
struct Tool {
    name: String,
    description: String,
    parameters: Vec<Parameter>,
    example: String,
}

/// The code tools every agent has, independent of the command table.
fn code_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "read_program".to_owned(),
            description: "Return the current program text and its line count.".to_owned(),
            parameters: vec![],
            example: "read_program()".to_owned(),
        },
        Tool {
            name: "apply_code_edit".to_owned(),
            description: "Replace the byte range [start, end) of the program with text and run it."
                .to_owned(),
            parameters: vec![
                param("start", "integer", true),
                param("end", "integer", true),
                param("text", "string", true),
            ],
            example: "apply_code_edit(120, 128, \"circle(40)\")".to_owned(),
        },
        Tool {
            name: "run".to_owned(),
            description: "Run the program and report diagnostics and layer count.".to_owned(),
            parameters: vec![],
            example: "run()".to_owned(),
        },
        Tool {
            name: "query".to_owned(),
            description: "Return the scene at a frame as JSON.".to_owned(),
            parameters: vec![
                param("frame", "integer", false),
                param("node", "string", false),
            ],
            example: "query(frame=48, node=\"title\")".to_owned(),
        },
        Tool {
            name: "render_still".to_owned(),
            description: "Render one frame headlessly to a PNG.".to_owned(),
            parameters: vec![
                param("frame", "integer", true),
                param("path", "string", true),
            ],
            example: "render_still(48, \"out/still.png\")".to_owned(),
        },
    ]
}

/// The parameter list parsed from a command signature such as
/// `key(id, property, time, value, ease="linear")`.
fn parameters(signature: &str) -> Vec<Parameter> {
    let Some((_, rest)) = signature.split_once('(') else {
        return Vec::new();
    };
    let rest = rest.trim_end_matches(')');
    if rest.trim().is_empty() {
        return Vec::new();
    }
    rest.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let part = part.trim_end_matches("...");
            let (name, default) = part
                .split_once('=')
                .map_or((part, None), |(name, _)| (name, Some(())));
            let required = default.is_none();
            Parameter {
                name: name.trim().to_owned(),
                kind: kind_of(name),
                required,
            }
        })
        .collect()
}

fn kind_of(name: &str) -> &'static str {
    match name {
        "frame" | "frames" | "index" | "to" | "row" | "rows" | "steps" | "n" => "integer",
        "x" | "y" | "dx" | "dy" | "zoom" | "factor" | "size" | "offset" | "stagger"
        | "duration" | "value" | "radius" | "width" | "height" => "number",
        "on" | "open" | "add" | "invert" | "locked" | "visible" | "overrides" => "boolean",
        "ids" | "keys" | "points" | "from" | "to_time" | "from_time" => "array",
        _ => "string",
    }
}

/// The whole schema as one JSON document.
#[must_use]
pub fn schema() -> String {
    let mut tools: Vec<Tool> = CommandKind::ALL
        .iter()
        .map(|kind| Tool {
            name: kind.name().to_owned(),
            description: kind.signature().to_owned(),
            parameters: parameters(kind.signature()),
            example: kind.example().to_text(),
        })
        .collect();
    tools.extend(code_tools());
    let mut j = Json::new();
    j.begin_object();
    j.key("version").int(1);
    j.key("tools").begin_array();
    for tool in &tools {
        j.begin_object();
        j.key("name").string(&tool.name);
        j.key("description").string(&tool.description);
        j.key("parameters").begin_array();
        for parameter in &tool.parameters {
            j.begin_object();
            j.key("name").string(&parameter.name);
            j.key("type").string(parameter.kind);
            j.key("required").boolean(parameter.required);
            j.end_object();
        }
        j.end_array();
        j.key("example").string(&tool.example);
        j.end_object();
    }
    j.end_array();
    j.end_object();
    j.finish()
}

/// The names of the code tools (tests only; the schema is the public list).
#[cfg(test)]
#[must_use]
pub fn code_tool_names() -> Vec<String> {
    code_tools().into_iter().map(|tool| tool.name).collect()
}

#[cfg(test)]
mod tests {
    use super::{code_tool_names, schema};
    use motioner_ui::CommandKind;

    #[test]
    fn every_command_is_in_the_schema() {
        let json = schema();
        for kind in CommandKind::ALL {
            assert!(
                json.contains(&format!("\"name\":\"{}\"", kind.name())),
                "{} is missing from `motioner tools --schema`",
                kind.name()
            );
        }
    }

    #[test]
    fn the_code_tools_and_the_version_are_present() {
        let json = schema();
        assert!(json.contains("\"version\":1"), "{json}");
        for name in code_tool_names() {
            assert!(json.contains(&format!("\"name\":\"{name}\"")), "{name}");
        }
        assert!(json.contains("\"example\":"), "{json}");
    }
}
