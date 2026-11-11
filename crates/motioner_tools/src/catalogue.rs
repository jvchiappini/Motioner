//! The D-0043 tool catalogue v2.
//!
//! One list of descriptors: name, description written for a model, cost class,
//! parameter schema, result schema and an example call. `motioner tools
//! --schema` merges this list with the D-0024 command tools; it never builds a
//! second one.
//!
//! The `family` field reserves room for prompt 16's rig tools (`properties`,
//! `poses`, `clips`, `inputs`) without implementing them here.

use serde_json::{json, Value};

/// How expensive a call is for the D-0012 budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cost {
    /// Reads the document or code; no pixels.
    Cheap,
    /// Renders an image.
    Render,
    /// Analyses media or the document deeply.
    Analysis,
    /// Waits for the user.
    User,
}

impl Cost {
    /// Stable lower-case name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cheap => "cheap",
            Self::Render => "render",
            Self::Analysis => "analysis",
            Self::User => "user",
        }
    }
}

/// One parameter of a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    /// Parameter name.
    pub name: &'static str,
    /// JSON type (`integer`, `number`, `string`, `boolean`, `array`, `object`).
    pub kind: &'static str,
    /// Whether the caller must pass it.
    pub required: bool,
    /// What it means.
    pub description: &'static str,
}

const fn param(
    name: &'static str,
    kind: &'static str,
    required: bool,
    description: &'static str,
) -> Param {
    Param {
        name,
        kind,
        required,
        description,
    }
}

/// One tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    /// Stable name.
    pub name: &'static str,
    /// When to use it, what it returns and what it costs.
    pub description: &'static str,
    /// Cost class.
    pub cost: Cost,
    /// Family (`project`, `editing`, `seeing`, `analysis`, `media`,
    /// `resources`, `user`, `export`; `rig` is reserved for prompt 16).
    pub family: &'static str,
    /// Whether this build executes it outside the harness.
    pub implemented: bool,
    /// Parameters.
    pub params: &'static [Param],
    /// A call that runs on `examples/demo.motioner` (or a frame of it).
    pub example: &'static str,
}

const NONE: &[Param] = &[];

/// The catalogue, in documentation order.
pub const TOOLS: &[ToolSpec] = &[
    // Project and code.
    ToolSpec {
        name: "project_summary",
        description: "Start here. Returns comps, size, fps, duration, theme, tempo, scenes, layer count, assets, pending overrides and the MDF/std versions without rendering. Cost: cheap.",
        cost: Cost::Cheap,
        family: "project",
        implemented: true,
        params: NONE,
        example: "project_summary()",
    },
    ToolSpec {
        name: "outline",
        description: "The node tree with id, name, kind, time range, parent and visibility. Use it to find ids before any other tool. Cost: cheap.",
        cost: Cost::Cheap,
        family: "project",
        implemented: true,
        params: &[
            param("comp", "string", false, "Composition name; the main one when omitted."),
            param("depth", "integer", false, "Maximum depth; all levels when omitted."),
            param("filter", "string", false, "Case-insensitive name substring."),
        ],
        example: "outline(depth=3)",
    },
    ToolSpec {
        name: "get_node",
        description: "Every property of one node: static values or keys, effects, masks, bounds at a frame and its code location. Cost: cheap.",
        cost: Cost::Cheap,
        family: "project",
        implemented: true,
        params: &[
            param("id", "string", true, "Explicit id, provenance key or `#n` from an overlay."),
            param("frame", "integer", false, "Frame used for bounds and keyed values."),
        ],
        example: "get_node(\"title\", frame=30)",
    },
    ToolSpec {
        name: "read_program",
        description: "Numbered source lines of the current program. Cost: cheap.",
        cost: Cost::Cheap,
        family: "project",
        implemented: true,
        params: &[
            param("file", "string", false, "Composition or module path; the open program when omitted."),
            param("start_line", "integer", false, "First line, 1-based."),
            param("end_line", "integer", false, "Last line, inclusive."),
        ],
        example: "read_program(start_line=1, end_line=40)",
    },
    ToolSpec {
        name: "search_code",
        description: "Case-insensitive substring or regex search over the program, with file and line. Cost: cheap.",
        cost: Cost::Cheap,
        family: "project",
        implemented: true,
        params: &[
            param("query", "string", true, "Text or regular expression."),
            param("regex", "boolean", false, "Treat the query as a regular expression."),
        ],
        example: "search_code(\"enter(\")",
    },
    ToolSpec {
        name: "selection",
        description: "Selected node ids, playhead frame and the visible time range. Cost: cheap.",
        cost: Cost::Cheap,
        family: "project",
        implemented: true,
        params: NONE,
        example: "selection()",
    },
    ToolSpec {
        name: "std_reference",
        description: "Signature, defaults, example and preview of a std function, preset, theme or component. Use it before writing std code. Cost: cheap.",
        cost: Cost::Cheap,
        family: "project",
        implemented: true,
        params: &[param("name", "string", true, "std name such as `rise`, `card` or `dark`.")],
        example: "std_reference(\"rise\")",
    },
    ToolSpec {
        name: "find_examples",
        description: "Curated examples matching a query, with their brief and path. Cost: cheap.",
        cost: Cost::Cheap,
        family: "project",
        implemented: true,
        params: &[
            param("query", "string", true, "What the example should show."),
            param("limit", "integer", false, "Maximum examples, default 5."),
        ],
        example: "find_examples(\"kinetic title\")",
    },
    // Editing.
    ToolSpec {
        name: "run_commands",
        description: "Runs D-0024 command text through the same Editor as the window, as one atomic batch. Returns per-command results, the code diff, write-through strategies, ids and an undo checkpoint. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[
            param("commands", "array", true, "Command strings, run in order."),
            param("scope", "string", false, "`this` (default) or `all`."),
        ],
        example: "run_commands([\"select title\", \"set opacity 0.5\"])",
    },
    ToolSpec {
        name: "apply_code_edit",
        description: "Replaces line ranges or exact text in a program, then runs it. Returns the diff, diagnostics, verification and an undo checkpoint. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[
            param("file", "string", true, "Program to edit."),
            param("edits", "array", true, "`{start_line,end_line,new_text}` or `{old_text,new_text}` entries."),
        ],
        example: "apply_code_edit(\"main.motion\", [{\"start_line\":10,\"end_line\":10,\"new_text\":\"t.enter(\\\"rise\\\")\\n\"}])",
    },
    ToolSpec {
        name: "write_file",
        description: "Writes a new composition or module file. Refuses to overwrite existing files. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[
            param("file", "string", true, "Path to create."),
            param("contents", "string", true, "Full file text."),
        ],
        example: "write_file(\"scenes/intro.motion\", \"comp(name=\\\"Intro\\\")\\n\")",
    },
    ToolSpec {
        name: "run_program",
        description: "Recompiles the program and returns diagnostics with spans and suggestions plus the run time. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: NONE,
        example: "run_program()",
    },
    ToolSpec {
        name: "format",
        description: "Rewrites the program in the canonical MDF style and returns the diff. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[param("file", "string", false, "Program to format; the open one when omitted.")],
        example: "format()",
    },
    ToolSpec {
        name: "lint",
        description: "Advisory diagnostics with fixes (shadowed std names, unused variables, magic numbers). Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[param("file", "string", false, "Program to lint; the open one when omitted.")],
        example: "lint()",
    },
    ToolSpec {
        name: "checkpoint",
        description: "Names the current program state so a later `restore` can return to it. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[param("name", "string", true, "Checkpoint name.")],
        example: "checkpoint(\"before redesign\")",
    },
    ToolSpec {
        name: "restore",
        description: "Restores a named checkpoint and returns the diff back to it. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[param("checkpoint", "string", true, "Checkpoint name.")],
        example: "restore(\"before redesign\")",
    },
    ToolSpec {
        name: "undo",
        description: "Undoes the last edits and returns the diff. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[param("steps", "integer", false, "Number of steps, default 1.")],
        example: "undo()",
    },
    ToolSpec {
        name: "redo",
        description: "Redoes undone edits and returns the diff. Cost: cheap.",
        cost: Cost::Cheap,
        family: "editing",
        implemented: true,
        params: &[param("steps", "integer", false, "Number of steps, default 1.")],
        example: "redo()",
    },
    // Seeing.
    ToolSpec {
        name: "render_still",
        description: "Renders one frame to a PNG (long edge 768 by default, maximum 1568) with an optional overlay pass. Returns the image path and, for marks, the `#n -> id, name` legend. Cost: render.",
        cost: Cost::Render,
        family: "seeing",
        implemented: true,
        params: &[
            param("frame", "integer", false, "Frame to render."),
            param("time", "number", false, "Seconds; used when `frame` is absent."),
            param("size", "integer", false, "Long edge in pixels, default 768, max 1568."),
            param("overlay", "string", false, "`none`, `marks`, `safe`, `grid` or `bounds`."),
            param("ids", "array", false, "Only these ids in the marks overlay."),
        ],
        example: "render_still(frame=90, overlay=\"marks\")",
    },
    ToolSpec {
        name: "render_filmstrip",
        description: "Renders up to 8 frames into one strip image with frame labels. Cost: render.",
        cost: Cost::Render,
        family: "seeing",
        implemented: true,
        params: &[
            param("start", "integer", true, "First frame."),
            param("end", "integer", true, "Last frame."),
            param("count", "integer", false, "Frames to sample, default 6, max 8."),
            param("overlay", "string", false, "Overlay name."),
        ],
        example: "render_filmstrip(0, 120, count=6)",
    },
    ToolSpec {
        name: "render_contact_sheet",
        description: "Renders up to 12 frames into one grid image with frame labels. Cost: render.",
        cost: Cost::Render,
        family: "seeing",
        implemented: true,
        params: &[
            param("count", "integer", false, "Frames to sample, default 9, max 12."),
            param("overlay", "string", false, "Overlay name."),
        ],
        example: "render_contact_sheet(count=9)",
    },
    ToolSpec {
        name: "render_zoom",
        description: "Crops a region of a rendered frame and scales it for inspection. Cost: render.",
        cost: Cost::Render,
        family: "seeing",
        implemented: true,
        params: &[
            param("frame", "integer", true, "Frame to render."),
            param("region", "array", true, "`[x, y, w, h]` in composition pixels."),
            param("size", "integer", false, "Long edge of the crop, default 768."),
        ],
        example: "render_zoom(90, [760, 340, 400, 220])",
    },
    ToolSpec {
        name: "compare",
        description: "Shows two frames (or two checkpoint renders) side by side, or a diff heatmap with the changed regions. Cost: render.",
        cost: Cost::Render,
        family: "seeing",
        implemented: true,
        params: &[
            param("a", "integer", true, "Frame or checkpoint name A."),
            param("b", "integer", true, "Frame or checkpoint name B."),
            param("mode", "string", false, "`side` (default) or `diff`."),
        ],
        example: "compare(30, 90, mode=\"diff\")",
    },
    ToolSpec {
        name: "view_asset",
        description: "Shows an imported image or a video frame with its metadata. Cost: render.",
        cost: Cost::Render,
        family: "seeing",
        implemented: true,
        params: &[
            param("key", "string", true, "Asset key."),
            param("frame", "integer", false, "Video frame to decode."),
            param("size", "integer", false, "Long edge in pixels."),
        ],
        example: "view_asset(\"bg\")",
    },
    ToolSpec {
        name: "render_variants",
        description: "Applies up to 4 edits on throwaway copies, renders one frame per variant into a grid, and leaves the project byte-identical. Wired by the prompt 14 harness. Cost: render.",
        cost: Cost::Render,
        family: "seeing",
        implemented: false,
        params: &[
            param("variants", "array", true, "`[{label, commands or edits}]`, max 4."),
            param("frame", "integer", true, "Frame to render."),
        ],
        example: "render_variants([{\"label\":\"A\",\"commands\":[\"set opacity 0.5\"]}], 90)",
    },
    ToolSpec {
        name: "view_editor",
        description: "Screenshot of the editor viewport. Harness-only: the headless CLI cannot open a window. Cost: render.",
        cost: Cost::Render,
        family: "seeing",
        implemented: false,
        params: NONE,
        example: "view_editor()",
    },
    // Analysing.
    ToolSpec {
        name: "describe_frame",
        description: "Exact text twin of a render: every visible element with id, kind, box, colours, text, z and opacity. Prefer it over an image for verification. Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[
            param("frame", "integer", true, "Frame to describe."),
            param("ids", "array", false, "Only these ids."),
        ],
        example: "describe_frame(90)",
    },
    ToolSpec {
        name: "check_design",
        description: "Runs the D-0040 rubric over a frame range and returns errors, warnings and infos with the measured value and a fix. Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[
            param("start", "integer", false, "First frame."),
            param("end", "integer", false, "Last frame."),
            param("rules", "array", false, "Only these rule ids."),
        ],
        example: "check_design()",
    },
    ToolSpec {
        name: "motion_report",
        description: "Per element onsets, durations, peak speed, velocity jumps and eases, plus global stagger, dead time and overload. Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[
            param("ids", "array", false, "Only these ids."),
            param("start", "integer", false, "First frame."),
            param("end", "integer", false, "Last frame."),
        ],
        example: "motion_report()",
    },
    ToolSpec {
        name: "layout_report",
        description: "Overlaps, safe-area violations, alignment groups and type hierarchy at one frame. Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[param("frame", "integer", true, "Frame to analyse.")],
        example: "layout_report(90)",
    },
    ToolSpec {
        name: "legibility_report",
        description: "Per text: on-screen time against reading time and contrast against the real background (from a render without the text). Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[
            param("start", "integer", false, "First frame."),
            param("end", "integer", false, "Last frame."),
        ],
        example: "legibility_report()",
    },
    ToolSpec {
        name: "hit_test",
        description: "Ids under a point at a frame, topmost first. Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[
            param("frame", "integer", true, "Frame."),
            param("x", "number", true, "Composition X."),
            param("y", "number", true, "Composition Y."),
        ],
        example: "hit_test(90, 960, 540)",
    },
    ToolSpec {
        name: "code_for_node",
        description: "The statement that created a node. Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[param("id", "string", true, "Node id.")],
        example: "code_for_node(\"title\")",
    },
    ToolSpec {
        name: "node_for_code",
        description: "Ids created or written by a source line. Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[
            param("file", "string", false, "File; the open program when omitted."),
            param("line", "integer", true, "1-based line."),
        ],
        example: "node_for_code(line=42)",
    },
    ToolSpec {
        name: "sample",
        description: "Exact channel values of a node at frames. Cost: analysis.",
        cost: Cost::Analysis,
        family: "analysis",
        implemented: true,
        params: &[
            param("id", "string", true, "Node id."),
            param("property", "string", true, "`pos.x`, `opacity`, `size`, ..."),
            param("frames", "array", true, "Frames to sample."),
        ],
        example: "sample(\"title\", \"opacity\", [0, 30, 60])",
    },
    // Media.
    ToolSpec {
        name: "analyze_image",
        description: "Dominant palette, salient region, empty regions ranked for text and luminance per region. Cost: analysis.",
        cost: Cost::Analysis,
        family: "media",
        implemented: true,
        params: &[param("key", "string", true, "Asset key.")],
        example: "analyze_image(\"bg\")",
    },
    ToolSpec {
        name: "analyze_video",
        description: "Scene cuts, motion intensity curve and calm segments of a video asset. Cost: analysis.",
        cost: Cost::Analysis,
        family: "media",
        implemented: true,
        params: &[param("key", "string", true, "Asset key.")],
        example: "analyze_video(\"clip\")",
    },
    ToolSpec {
        name: "analyze_audio",
        description: "Duration, tempo, beat and downbeat times, sections and loudness curve. Cost: analysis.",
        cost: Cost::Analysis,
        family: "media",
        implemented: true,
        params: &[param("key", "string", true, "Asset key.")],
        example: "analyze_audio(\"music\")",
    },
    // Resources.
    ToolSpec {
        name: "list_fonts",
        description: "Bundled and project font families with weights. Cost: cheap.",
        cost: Cost::Cheap,
        family: "resources",
        implemented: true,
        params: NONE,
        example: "list_fonts()",
    },
    ToolSpec {
        name: "list_themes",
        description: "The ten std themes with their palette highlights. Cost: cheap.",
        cost: Cost::Cheap,
        family: "resources",
        implemented: true,
        params: NONE,
        example: "list_themes()",
    },
    ToolSpec {
        name: "search_icons",
        description: "Bundled permissive icon set by name. Cost: cheap.",
        cost: Cost::Cheap,
        family: "resources",
        implemented: true,
        params: &[
            param("query", "string", true, "Icon name or keyword."),
            param("limit", "integer", false, "Maximum results."),
        ],
        example: "search_icons(\"arrow\")",
    },
    ToolSpec {
        name: "brand_kit",
        description: "Reads or writes the project brand kit: colours, fonts and logo asset key. Cost: cheap.",
        cost: Cost::Cheap,
        family: "resources",
        implemented: true,
        params: &[
            param("action", "string", true, "`get` or `set`."),
            param("values", "object", false, "Values to write."),
        ],
        example: "brand_kit(\"get\")",
    },
    ToolSpec {
        name: "import_asset",
        description: "Imports a file the user attached in this conversation. Harness-only. Cost: cheap.",
        cost: Cost::Cheap,
        family: "resources",
        implemented: false,
        params: &[param("attachment", "string", true, "Attachment path or name.")],
        example: "import_asset(\"bg.png\")",
    },
    ToolSpec {
        name: "place_asset",
        description: "Places an imported asset in the composition. Cost: cheap.",
        cost: Cost::Cheap,
        family: "resources",
        implemented: false,
        params: &[
            param("key", "string", true, "Asset key."),
            param("at", "number", false, "Time in seconds."),
            param("pos", "array", false, "`[x, y]`."),
        ],
        example: "place_asset(\"bg\", pos=[960, 540])",
    },
    // User tools (harness).
    ToolSpec {
        name: "ask_user",
        description: "Asks up to 3 questions with options and a recommended default. Harness-only. Cost: user.",
        cost: Cost::User,
        family: "user",
        implemented: false,
        params: &[param("questions", "array", true, "Question objects.")],
        example: "ask_user([{\"question\":\"Format?\",\"options\":[{\"label\":\"16:9\"}]}])",
    },
    ToolSpec {
        name: "ask_user_visual",
        description: "Asks the user to pick one of the variants rendered by `render_variants`. Harness-only. Cost: user.",
        cost: Cost::User,
        family: "user",
        implemented: false,
        params: &[
            param("question", "string", true, "What to choose."),
            param("variants", "array", true, "Variant labels."),
        ],
        example: "ask_user_visual(\"Which one?\", [\"A\", \"B\"])",
    },
    ToolSpec {
        name: "show_plan",
        description: "Shows the design plan for approval. Harness-only. Cost: user.",
        cost: Cost::User,
        family: "user",
        implemented: false,
        params: &[param("plan", "object", true, "Style, palette, type, tempo and scenes.")],
        example: "show_plan({\"style\":\"editorial\"})",
    },
    ToolSpec {
        name: "request_approval",
        description: "Asks for approval before a costly or destructive action. Harness-only. Cost: user.",
        cost: Cost::User,
        family: "user",
        implemented: false,
        params: &[
            param("action", "string", true, "What will happen."),
            param("reason", "string", true, "Why."),
            param("cost", "string", false, "Estimated cost."),
        ],
        example: "request_approval(\"export\", \"final render\")",
    },
    ToolSpec {
        name: "remember_preference",
        description: "Stores a user preference (with consent). Harness-only. Cost: user.",
        cost: Cost::User,
        family: "user",
        implemented: false,
        params: &[
            param("key", "string", true, "Preference name."),
            param("value", "string", true, "Preference value."),
        ],
        example: "remember_preference(\"palette\", \"mono\")",
    },
    // Export.
    ToolSpec {
        name: "render_preview",
        description: "Low-quality MP4 preview plus one image of sampled frames. Cost: render.",
        cost: Cost::Render,
        family: "export",
        implemented: true,
        params: &[
            param("start", "integer", false, "First frame."),
            param("end", "integer", false, "Last frame."),
            param("quality", "string", false, "`low` (default) or `medium`."),
        ],
        example: "render_preview(0, 120)",
    },
    ToolSpec {
        name: "export",
        description: "Final MP4 export. Requires `request_approval` in the same turn. Cost: render.",
        cost: Cost::Render,
        family: "export",
        implemented: true,
        params: &[
            param("path", "string", false, "Output path."),
            param("format", "string", false, "`mp4` (default)."),
        ],
        example: "export(\"out/final.mp4\")",
    },
];

/// The spec of `name`.
#[must_use]
pub fn spec(name: &str) -> Option<&'static ToolSpec> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// The result schema of `name` as JSON Schema.
#[must_use]
#[allow(clippy::too_many_lines, clippy::match_same_arms)] // exact schemas per tool, D-0043
pub fn result_schema(name: &str) -> Value {
    let object = |properties: Value| {
        json!({
            "type": "object",
            "properties": properties,
            "required": [],
        })
    };
    match name {
        "project_summary" => object(json!({
            "comps": {"type": "array"},
            "width": {"type": "integer"},
            "height": {"type": "integer"},
            "fps": {"type": "string"},
            "duration": {"type": "integer"},
            "theme": {"type": "string"},
            "tempo": {"type": "number"},
            "scenes": {"type": "array"},
            "layers": {"type": "integer"},
            "assets": {"type": "integer"},
            "overrides": {"type": "integer"},
            "mdf_version": {"type": "string"},
            "std_version": {"type": "integer"}
        })),
        "outline" => object(json!({
            "nodes": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"}, "name": {"type": "string"},
                    "kind": {"type": "string"}, "depth": {"type": "integer"},
                    "parent": {"type": "string"}, "visible": {"type": "boolean"},
                    "start": {"type": "integer"}, "end": {"type": "integer"}
                }
            }}
        })),
        "get_node" => object(json!({
            "id": {"type": "string"},
            "name": {"type": "string"},
            "kind": {"type": "string"},
            "bounds": {"type": "array"},
            "code": {"type": "object"},
            "channels": {"type": "array"}
        })),
        "read_program" => object(json!({
            "file": {"type": "string"},
            "lines": {"type": "array", "items": {"type": "string"}}
        })),
        "search_code" => object(json!({
            "matches": {"type": "array", "items": {
                "type": "object",
                "properties": {"file": {"type": "string"}, "line": {"type": "integer"}, "text": {"type": "string"}}
            }}
        })),
        "selection" => object(json!({
            "ids": {"type": "array"},
            "frame": {"type": "integer"}
        })),
        "std_reference" => object(json!({
            "name": {"type": "string"},
            "signature": {"type": "string"},
            "parameters": {"type": "array"},
            "source": {"type": "string"}
        })),
        "find_examples" => object(json!({
            "examples": {"type": "array", "items": {
                "type": "object",
                "properties": {"path": {"type": "string"}, "brief": {"type": "string"}}
            }}
        })),
        "run_commands" | "apply_code_edit" | "write_file" | "run_program" | "format" | "lint"
        | "restore" | "undo" | "redo" => object(json!({
            "ok": {"type": "boolean"},
            "diff": {"type": "string"},
            "diagnostics": {"type": "array"},
            "strategies": {"type": "array"},
            "ids": {"type": "array"},
            "checkpoint": {"type": "string"}
        })),
        "checkpoint" => object(json!({"checkpoint": {"type": "string"}})),
        "render_still"
        | "render_filmstrip"
        | "render_contact_sheet"
        | "render_zoom"
        | "view_asset"
        | "render_variants"
        | "compare"
        | "view_editor"
        | "export" => object(json!({
            "images": {"type": "array", "items": {"type": "string"}},
            "frames": {"type": "array"},
            "legend": {"type": "array"},
            "changed_regions": {"type": "array"}
        })),
        "describe_frame" => object(json!({
            "frame": {"type": "integer"},
            "elements": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"}, "kind": {"type": "string"},
                    "box": {"type": "array"}, "text": {"type": "string"},
                    "z": {"type": "integer"}, "opacity": {"type": "number"}
                }
            }}
        })),
        "check_design" => object(json!({
            "errors": {"type": "integer"},
            "warnings": {"type": "integer"},
            "infos": {"type": "integer"},
            "issues": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "rule": {"type": "string"}, "severity": {"type": "string"},
                    "nodes": {"type": "array"}, "frames": {"type": "array"},
                    "measured": {"type": "string"}, "threshold": {"type": "string"},
                    "fix": {"type": "string"}
                }
            }}
        })),
        "motion_report" => object(json!({
            "elements": {"type": "array"},
            "stagger": {"type": "object"},
            "dead_time_frames": {"type": "integer"},
            "overload_peak": {"type": "integer"}
        })),
        "layout_report" => object(json!({
            "frame": {"type": "integer"},
            "text_overlaps": {"type": "array"},
            "busy_overlaps": {"type": "array"},
            "safe_violations": {"type": "array"},
            "alignment_groups": {"type": "array"},
            "type_levels": {"type": "array"}
        })),
        "legibility_report" => object(json!({
            "texts": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"}, "visible_seconds": {"type": "number"},
                    "required_seconds": {"type": "number"}, "contrast": {"type": "number"}
                }
            }}
        })),
        "hit_test" => object(json!({"hits": {"type": "array"}})),
        "code_for_node" => {
            object(json!({"statement": {"type": "string"}, "line": {"type": "integer"}}))
        }
        "node_for_code" => object(json!({"ids": {"type": "array"}})),
        "sample" => object(json!({"values": {"type": "array"}})),
        "analyze_image" => object(json!({
            "palette": {"type": "array"},
            "salient": {"type": "object"},
            "empty_regions": {"type": "array"}
        })),
        "analyze_video" => object(json!({
            "cuts": {"type": "array"},
            "motion": {"type": "array"},
            "calm_segments": {"type": "array"}
        })),
        "analyze_audio" => object(json!({
            "duration": {"type": "number"},
            "bpm": {"type": "number"},
            "beats": {"type": "array"},
            "downbeats": {"type": "array"},
            "sections": {"type": "array"}
        })),
        "list_fonts" => object(json!({"families": {"type": "array"}})),
        "list_themes" => object(json!({"themes": {"type": "array"}})),
        "search_icons" => object(json!({"icons": {"type": "array"}})),
        "brand_kit" => object(json!({"colors": {"type": "object"}, "fonts": {"type": "object"}})),
        "import_asset" => object(json!({"key": {"type": "string"}, "kind": {"type": "string"}})),
        "place_asset" => object(json!({"ids": {"type": "array"}})),
        "render_preview" => object(json!({
            "path": {"type": "string"},
            "duration": {"type": "number"},
            "images": {"type": "array"}
        })),
        "ask_user"
        | "ask_user_visual"
        | "show_plan"
        | "request_approval"
        | "remember_preference" => object(json!({"answer": {"type": "string"}})),
        _ => object(json!({})),
    }
}

/// One descriptor as JSON, as the schema command prints it.
#[must_use]
pub fn to_json(tool: &ToolSpec) -> Value {
    let parameters = tool
        .params
        .iter()
        .map(|parameter| {
            json!({
                "name": parameter.name,
                "type": parameter.kind,
                "required": parameter.required,
                "description": parameter.description,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "name": tool.name,
        "description": tool.description,
        "cost": tool.cost.name(),
        "family": tool.family,
        "implemented": tool.implemented,
        "parameters": parameters,
        "result": result_schema(tool.name),
        "example": tool.example,
    })
}

/// Every tool as JSON.
#[must_use]
pub fn catalogue_json() -> Vec<Value> {
    TOOLS.iter().map(to_json).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_tool_is_complete_and_unique() {
        let mut names = HashSet::new();
        for tool in TOOLS {
            assert!(names.insert(tool.name), "duplicate tool {}", tool.name);
            assert!(
                tool.description.len() > 40,
                "{} needs a real description",
                tool.name
            );
            assert!(!tool.example.is_empty(), "{} has no example", tool.name);
            assert!(!tool.family.is_empty(), "{} has no family", tool.name);
            let schema = result_schema(tool.name);
            assert_eq!(
                schema["type"], "object",
                "{} result is not an object",
                tool.name
            );
            for parameter in tool.params {
                assert!(!parameter.description.is_empty(), "{} param", tool.name);
            }
        }
        assert!(
            TOOLS.len() >= 40,
            "the catalogue lost tools: {}",
            TOOLS.len()
        );
    }

    #[test]
    fn the_schema_is_serialisable_and_keeps_the_contract() {
        let catalogue = catalogue_json();
        let text = serde_json::to_string(&catalogue).unwrap();
        assert!(text.contains("\"name\":\"check_design\""));
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), TOOLS.len());
    }

    #[test]
    fn user_tools_are_marked_harness_only() {
        for name in [
            "ask_user",
            "ask_user_visual",
            "show_plan",
            "request_approval",
            "remember_preference",
        ] {
            let tool = spec(name).unwrap();
            assert!(!tool.implemented, "{name}");
            assert_eq!(tool.cost, Cost::User);
        }
    }
}
