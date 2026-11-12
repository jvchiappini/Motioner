# Domain: motioner_dsl

## Purpose

MDF: lexer, parser, interpreter and the scene API that turns a program into a
document, plus value origins and bake. The language is the project format
(D-0005, D-0011); it is deterministic (same text â†’ same document).

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
- `Scene::linear_gradient` / `radial_gradient` are pure builtins that
  build a `Gradient` from `(start, end)` or `(center, radius)` and a
  `stops` list; constructors attach it to `fill`/`stroke`/text `color`,
  and `ContentField::from_name` parses `fill_stop<n>_color` (D-0030):
  `crates/motioner_dsl/src/builtins.rs:80-190` and
  `crates/motioner_dsl/src/scene.rs:700-790`.
- `bezier(x1, y1, x2, y2)` is a pure builtin returning a `Value::Ease`;
  `key(..., ease=...)` accepts a name string or that value. `stagger` and
  `node.wiggle` emit ordinary keys through the same `Scene::key` path
  (D-0037): `crates/motioner_dsl/src/builtins.rs:70-95` and
  `crates/motioner_dsl/src/scene.rs:690-850`.
- `ValueOrigins` — the span, literal shape and ease of every written value,
  plus origins v2 (D-0036): the creation site, the statement and block that
  wrote each property, the interned binding and whether it is reassigned, the
  loop stack (span, variable, iterable, index, literal value, body) and the
  call stack (call site, definition, parameters, returns-the-node), and whether
  the expression is a literal, a parameter, a loop variable or computed:
  `crates/motioner_dsl/src/origins.rs:1-515`.
- `CodeEdit` — ordered, non-overlapping byte-range replacements with apply,
  invert (byte-exact undo), compose, provenance re-keying and override
  re-keying: `crates/motioner_dsl/src/code_edit.rs:132-365`.
- `plan` — resolves one core `EditCommand` into a `CodeEdit` or a
  `Fallback(reason)`: private literal, loop guard, call site, local assignment
  next to the node's last write. `plan_with_scope(..., Scope::This | Scope::All)`
  exposes the "this one / all" choice for loop instances. `verify` re-runs the
  program (with optional overrides) and compares the document by position;
  bake, the editor and tests share it (D-0036):
  `crates/motioner_dsl/src/plan.rs:62-140`.
- `call_site` — D-0036 section 2.3: passes a parameter as a keyword argument
  (or edits the positional one), binds a returned call result and assigns
  after the call, or promotes a literal to a keyword parameter whose default
  is the old text. MDF now supports defaulted parameters:
  `crates/motioner_dsl/src/call_site.rs`.
- `loop_guard` — D-0036 section 2.2: builds `if i == 3:` from the loop stack
  (nested guards joined with `and`), inserts it in the innermost loop body
  after the node's last write, extends an existing guard, merges repeated
  iterations into `if i in (1, 3):`, removes dead guards, renames `_`, rewrites
  repeated/non-literal loops to `enumerate`, and binds inline nodes. `Scope::All`
  edits the shared literal instead and verifies every instance:
  `crates/motioner_dsl/src/loop_guard.rs`.
- `bake` — batch write-through: each override goes through `plan`; refusals
  fall back to the generated block (D-0022) and verification uses the same
  function: `crates/motioner_dsl/src/bake.rs:68-220`.
- `write_text` — one set of number/pair/quote/optional-`None` rules for
  planner snippets and the generated block:
  `crates/motioner_dsl/src/write_text.rs`.
- `override_set` parsing and execution — the overrides file shares the MDF
  syntax with the editor's command layer: `crates/motioner_dsl/src/overrides_file.rs`.

## Diagram

Not needed: the interpreter drives the core document API; the scene module is
the only bridge.

## Relevant decisions

- D-0011 — MDF is the document format.
- D-0022 / D-0025 / D-0027 — identities, orphan re-keying and hybrid bake.
- D-0036 — origins v2 feeds write-through planning (statement/block/binding/
  loop/call context).
- D-0028 — content fields are animatable channels.

## Documentation status

partial — last verified: 2026-11-04


## Prompt 16 additions (imports and rigs)

- `use()` is a builtin whose module execution and namespace values live in
  the interpreter; assets are dispatched by extension in the DSL and only
  SVG/modules call the injected `AssetResolver` (D-0051):
  `crates/motioner_dsl/src/assets.rs:1-59`, `crates/motioner_dsl/src/interpreter.rs:1070-1160`.
- Symbols, rig builders, properties, drivers, poses, clips, state machines
  and the bake are a child module of the scene so they share the document
  without widening it: `crates/motioner_dsl/src/scene/rigs.rs:1-120`.
- `rig_prop` and `rig_property_key` declare and key properties
  (`crates/motioner_dsl/src/scene/rigs.rs:492-680`); drivers are compiled to
  `motioner_rig::Expr` at registration
  (`crates/motioner_dsl/src/scene/rigs.rs:1420-1600`).
- `bake_rigs` runs `motioner_rig::bake` for every rig at `into_document`
  and writes transform/opacity/stroke keys, inserting a hold key before a
  change so rest gaps never interpolate
  (`crates/motioner_dsl/src/scene/rigs.rs:1239-1380`).

