# Domain: motioner_ui

## Purpose

The window-free editor: one `Command` enum, one `Editor` that executes it, the
pure models behind the panels, and the live preview window. This crate is the
only place that names engine types on the editor side (through
`motioner_render`'s `live` feature).

## Key components

- `Editor` — session, transport, selection, undo, command execution (D-0024):
  `crates/motioner_ui/src/editor/mod.rs:113-660`.
- `editor::channels` — the one channel table. A `Channel` is
  `Transform(property, axis)` or `Content(field)`; keys, eases, sampling and
  key lookup all go through it (D-0028):
  `crates/motioner_ui/src/editor/channels.rs:28-200`.
- `editor::resolve` — commands to override edits; `edit` keys a channel when it
  is animated, otherwise writes a static value:
  `crates/motioner_ui/src/editor/resolve.rs:32-240`.
- `timeline_model` — timeline rows, content keys, visibility strips and
  summary diamonds: `crates/motioner_ui/src/timeline_model.rs:151-260`.
- `editor::query` — the JSON scene, including content channels:
  `crates/motioner_ui/src/editor/query.rs:85-250`.
- `inspect` — the inspector model and its pure layout; path layers get trim,
  dash, cap and join rows (`crates/motioner_ui/src/inspect.rs:405-475`).
- `bounds` — selection boxes and hit testing; a path item resolves through
  `motioner_render::path_bounds` and is picked against its fill or stroke
  (`crates/motioner_ui/src/bounds.rs:70-205`).

## Diagram

Not needed: the panels read pure models; the app turns input into commands.

## Relevant decisions

- D-0022 / D-0024 — overrides and the command layer.
- D-0025 / D-0026 — orphans and canvas view state.
- D-0028 — content channels.
- D-0029 — path content rows, bounds and picking.

## Documentation status

partial — last verified: 2026-11-01
