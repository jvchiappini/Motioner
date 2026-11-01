# Domain: motioner_dsl

## Purpose

MDF: lexer, parser, interpreter and the scene API that turns a program into a
document, plus value origins and bake. The language is the project format
(D-0005, D-0011); it is deterministic (same text → same document).

## Key components

- `Scene` — the 2D scene API: constructors, attributes and `key`:
  `crates/motioner_dsl/src/scene.rs:63-280`.
- `Scene::key` — transform channels and content fields share one method; a
  content field has no axes and `text`/`align` reject keys:
  `crates/motioner_dsl/src/scene.rs:377-500`.
- `Scene::path` / `polygon` / `star` / `arc` — build a `PathGeometry` from MDF
  values; a point is `(x, y)` or `(x, y, in_dx, in_dy, out_dx, out_dy)` and
  the paint/cap/join/trim/dash arguments map to content fields (D-0029):
  `crates/motioner_dsl/src/scene.rs:243-400`.
- `ValueOrigins` — the span, literal shape and ease of every written value, the
  input of the hybrid bake: `crates/motioner_dsl/src/origins.rs:52-174`.
- `bake` — edits private literals in place, otherwise appends a generated
  block, then verifies by re-running (D-0027):
  `crates/motioner_dsl/src/bake.rs:77-208`.
- `override_set` parsing and execution — the overrides file shares the MDF
  syntax with the editor's command layer: `crates/motioner_dsl/src/overrides_file.rs`.

## Diagram

Not needed: the interpreter drives the core document API; the scene module is
the only bridge.

## Relevant decisions

- D-0011 — MDF is the document format.
- D-0022 / D-0025 / D-0027 — identities, orphan re-keying and hybrid bake.
- D-0028 — content fields are animatable channels.

## Documentation status

partial — last verified: 2026-11-01
