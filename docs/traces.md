# Edit traces

A trace is the record of one committed edit: the program before and after,
the command that changed it, the planner strategy and the diff. Traces are the
training material for the Motioner code model (D-0036 section 6) and local by
design: nothing is uploaded, and recording is off for end users until they turn
it on for a project.

## Where they live

One JSONL file per day under `<project>/traces/`:

```
<name>.motioner/
  traces/
    2026-11-06.jsonl
```

Each line is one record. A file only grows by appending; nothing rewrites an
existing record. The writer is `motioner_io::traces` and the parser used by
tests and `motioner trace replay` is the same module, so the format and its
reader cannot drift.

## Schema, version 1

| Field | Type | Meaning |
|---|---|---|
| `version` | integer | `1`; a reader skips any other version. |
| `time` | string | RFC 3339 timestamp (local offset when the host reports one). |
| `file` | string | Project-relative program file. |
| `before_sha256` | string | SHA-256 of the program before the edit. |
| `before` | string | The program text before the edit. |
| `command` | string | The D-0024 command in its text form (`edit(...)`, `key(...)`, `add_layer(...)`). |
| `strategy` | string | The planner strategy, or `fallback`. |
| `diff` | string | Unified diff of the program change. |
| `after_sha256` | string | SHA-256 of the program after the edit. |
| `frame` | integer | Playhead frame when the edit ran. |
| `selection` | string or null | Selected node identity. |
| `conversation` | string or null | Reserved for the agent panel: the conversation turn. |
| `tool_calls` | array of strings | Reserved for the agent panel: the tool calls around the edit. |

The agent fields are reserved now so a future in-app agent writes the same
records as the window and the CLI; `motioner tools --schema` publishes the
surface those calls will use.

## When recording happens

- Debug builds: on by default. This is how Motioner records its own corpora
  (synthetic generation, dogfooding).
- Release builds: off unless the project turned recording on in
  `editor.motion` (`traces=True`) or `MOTIONER_TRACES=1` is set.
- `MOTIONER_TRACES=1` always wins, in every build.
- Traces are written after the undo entry is pushed, at the end of the commit,
  so recording adds no measurable latency to the gesture.
- A trace write failure is a warning, never an edit failure.

The View menu has the switch (`Edit traces (local only)`); the setting is stored
per project in `editor.motion` and the menu explains that records stay local.

## Export and replay

```
motioner trace export <project|dir> --out corpus.jsonl [--strategy S] [--since T]
motioner trace replay <corpus.jsonl>
```

`export` concatenates every day file, drops records with an unknown schema,
keeps the first record per `(before_sha256, command)` pair, and drops any record
that cannot be replayed exactly. `--since` takes a date prefix
(`2026-11-06`) or a full RFC 3339 timestamp.

`replay` applies each command to its `before` program with the real editor and
compares the SHA-256 of the result with `after_sha256`. A committed fixture
(`crates/motioner_app/tests/fixtures/traces.jsonl`) runs in CI, so a planner
change that breaks old records is caught immediately.

## Privacy

Traces contain program text. They are local files inside the project folder,
included in the project backup policy, and never sent anywhere. Sharing is
explicit and manual.
