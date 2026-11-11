# Agent tools (`motioner tool`)

Motioner publishes one tool catalogue for the in-app agent, the local model
and external agents such as Claude Code. The catalogue is generated from
Rust definitions (`crates/motioner_tools/src/catalogue.rs`) and merged with
the D-0024 command tools by the single generator in
`crates/motioner_app/src/tools.rs`.

```text
motioner tools --schema                              # JSON Schema for every tool
motioner tool <project|file.motion> <name> [--args JSON] [--out DIR]
motioner tool <project|file.motion> --batch FILE.jsonl
```

`--args` is a JSON object; images are written to `--out` (default
`out/tools/`) and their paths appear in the result. Every call prints one
JSON object:

```json
{"ok":true,"result":{...}}
{"ok":false,"error":{"kind":"invalid_args","message":"..."}}
```

## How an external agent uses it

1. `motioner tools --schema` once, to load the tool definitions into the
   model's tool format.
2. `motioner tool demo.motioner project_summary` and `outline` to learn the
   ids.
3. `describe_frame`, `check_design`, `motion_report`, `layout_report` and
   `legibility_report` are the **text twins** of a render: prefer them over
   images to verify a change.
4. `render_still` with `overlay="marks"` when a visual question is needed;
   the legend maps `#n` to the node id, and later calls accept `#n` wherever
   an id is expected.
5. Mutations: `run_commands` (D-0024 command text), `apply_code_edit`,
   `write_file`, `format`, `lint`, `undo`, `redo`, `checkpoint`, `restore`.
   Every mutating call returns the code diff and the undo checkpoint.
6. `render_preview` and `export` are the final steps; the harness asks for
   approval before an export.

A batch is JSONL, one call per line, and replays deterministically against
the same project state:

```json
{"tool":"project_summary","args":{}}
{"tool":"describe_frame","args":{"frame":90}}
{"tool":"check_design","args":{}}
```

## Families

| Family | Tools |
|---|---|
| project | `project_summary`, `outline`, `get_node`, `read_program`, `search_code`, `selection`, `std_reference`, `find_examples` |
| editing | `run_commands`, `apply_code_edit`, `write_file`, `run_program`, `format`, `lint`, `checkpoint`, `restore`, `undo`, `redo` |
| seeing | `render_still`, `render_filmstrip`, `render_contact_sheet`, `render_zoom`, `compare`, `view_asset`, `render_variants`, `view_editor` |
| analysis | `describe_frame`, `check_design`, `motion_report`, `layout_report`, `legibility_report`, `hit_test`, `code_for_node`, `node_for_code`, `sample` |
| media | `analyze_image`, `analyze_video`, `analyze_audio` |
| resources | `list_fonts`, `list_themes`, `search_icons`, `brand_kit`, `import_asset`, `place_asset` |
| user | `ask_user`, `ask_user_visual`, `show_plan`, `request_approval`, `remember_preference` |
| export | `render_preview`, `export` |

`family: "rig"` is reserved for prompt 16 (properties, poses, clips, inputs)
without being implemented here.

## Cost classes

`cheap`, `render`, `analysis` and `user`. The harness counts each class
against the per-turn budgets (D-0012); the CLI ignores budgets.

## Status

`implemented: false` in the schema means the tool is declared but only the
prompt 14 harness can run it: the five user tools, `view_editor`,
`import_asset`, `place_asset` and `render_variants` (throwaway project
copies). Calling them from `motioner tool` returns a typed
`requires_harness` error, never a panic. `render_preview` and `export` are
declared here; the CLI path for the final video is still
`motioner render`.

## Images

Stills use a 768 px long edge by default (maximum 1568), strips and sheets
1536 wide, all PNG sRGB. A tool-driven render never changes the document:
marks, safe areas and grids are a second pass rendered from a synthetic
document and composited over the frame. The in-memory image cache is keyed
by (program hash, frame, size, overlay).
