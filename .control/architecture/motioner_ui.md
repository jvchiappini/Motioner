# Domain: motioner_ui

## Purpose

The window-free editor: one `Command` enum, one `Editor` that executes it, the
pure models behind the panels, and the live preview window. This crate is the
only place that names engine types on the editor side (through
`motioner_render`'s `live` feature).

## Key components

- `Editor` Ã¢â‚¬â€ session, transport, selection, undo, command execution (D-0024):
  `crates/motioner_ui/src/editor/mod.rs:113-660`.
- `editor::channels` Ã¢â‚¬â€ the one channel table. A `Channel` is
  `Transform(property, axis)` or `Content(field)`; keys, eases, sampling and
  key lookup all go through it (D-0028):
  `crates/motioner_ui/src/editor/channels.rs:28-200`.
- `editor::resolve` Ã¢â‚¬â€ commands to override edits; `edit` keys a channel when it
  is animated, otherwise writes a static value:
  `crates/motioner_ui/src/editor/resolve.rs:32-240`.
- `timeline_model` Ã¢â‚¬â€ timeline rows, content keys, visibility strips and
  summary diamonds: `crates/motioner_ui/src/timeline_model.rs:151-260`.
- `editor::query` Ã¢â‚¬â€ the JSON scene, including content channels:
  `crates/motioner_ui/src/editor/query.rs:85-250`.
- `inspect` Ã¢â‚¬â€ the inspector model and its pure layout; path layers get trim,
  Gradient stops get one offset row and one colour row each, with the same`n  stopwatch and diamonds; the timeline labels them `Stop <n> offset/colour``n  (D-0030): `crates/motioner_ui/src/inspect.rs:391-500` and`n  `crates/motioner_ui/src/timeline_model.rs:237-320`.
- The inspector opens with a whole-node blend grid (seven tiles, four per
  row); the active mode fills with the accent and a click writes the `blend`
  override (D-0031): `crates/motioner_ui/src/inspect.rs` and
  `crates/motioner_ui/src/panels/inspector.rs`.
  dash, cap and join rows (`crates/motioner_ui/src/inspect.rs:405-475`).
- `bounds` Ã¢â‚¬â€ selection boxes and hit testing; a path item resolves through
  `motioner_render::path_bounds` and is picked against its fill or stroke
  (`crates/motioner_ui/src/bounds.rs:70-205`).

## Diagram

Not needed: the panels read pure models; the app turns input into commands.

## Relevant decisions

- D-0022 / D-0024 Ã¢â‚¬â€ overrides and the command layer.
- D-0025 / D-0026 Ã¢â‚¬â€ orphans and canvas view state.
- D-0028 Ã¢â‚¬â€ content channels.
- D-0029 Ã¢â‚¬â€ path content rows, bounds and picking.

## Documentation status

partial Ã¢â‚¬â€ last verified: 2026-11-01
