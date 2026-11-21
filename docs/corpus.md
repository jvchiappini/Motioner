# The corpus pipeline

Motioner's small motion-design model is trained on MDF programs: a frontier
model generates candidates, the editor and its users produce real edits, and
`motioner corpus check` filters both into a clean corpus (D-0036). The tooling
never calls a model; it needs no key and no network.

## Seed pairs

Every program in `examples/` has a sibling `<name>.prompt.md` with a short
English brief. The pair `(brief, program)` is a seed example of the language,
and the briefs are the starting point for generation prompts.

## Checking a directory

```
motioner corpus check <dir> [--frames N]
```

For each `.motion` file in the directory the tool:

1. parses and runs the program without overrides;
2. requires no warnings;
3. runs `motioner fmt --check` on the text;
4. runs the advisory linter;
5. renders `N` evenly spaced stills headlessly (at least 2);
6. rejects a program whose every still is blank or near-blank (fewer than
   0.5 % of pixels differ from the first one); a fade-in may legitimately
   start blank;
7. renders the first frame twice and requires byte-identical output;
8. records the layer count, composition length, features used, render times and
   every rejection reason.

`manifest.json` is written next to the programs. It has one entry per file with
`file`, `status`, `accepted`, `duration_ms`, `layers`, `duration`, `features`,
`render_ms` and `reasons`, plus the accepted/rejected totals. The exit code is
zero for a directory of rejected programs: a rejected candidate is data, not a
tool failure. Unreadable directories, a missing renderer or a failed manifest
write are tool failures and exit non-zero.

Features come from the AST (`motioner_app::corpus::features`), not from a text
search, so a comment or a string that mentions `text` never counts.

## Tool schema

```
motioner tools --schema
```

Prints one JSON document with every D-0024 command (name, signature as the
description, parameters with a coarse type, an example) plus the code tools
`read_program`, `apply_code_edit`, `run`, `query` and `render_still`. The
schema is generated from the command table; a test fails when a command is
missing. The future in-app agent, external agents and the training data share
this one list.

## No model here

Generation (Claude Fable 5 in the product plan), distillation and evaluation
live in a separate pipeline fed by traces (`docs/traces.md`) and this corpus
output. `motioner corpus check` is only the filter.
