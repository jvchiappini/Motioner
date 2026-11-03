# Domain: motioner_ui

## Purpose

The window-free editor: one `Command` enum, one `Editor` that executes it, the
pure models behind the panels, and the live preview window. This crate is the
only place that names engine types on the editor side (through
`motioner_render`'s `live` feature).

## Key components
- Mask section: a source picker (click cycles the nodes `can_mask` accepts),
  Alpha/Luma tiles, Off/On invert tiles and a Clear tile, backed by
  `mask_set`/`mask_clear`; the masked node and its source carry a mask badge in
  the layers panel (a source child is already indented under its node) -
  `crates/motioner_ui/src/inspect.rs:850-1080`,
  `crates/motioner_ui/src/panels/inspector.rs:160-320`,
  `crates/motioner_ui/src/panels/layers.rs:40-120`,
  `crates/motioner_ui/src/editor/resolve.rs:240-420`.
- Effects section: one row per stacked effect (up/down/remove tiles), one
  row per parameter with its stopwatch, and an "Add effect" row with the
  three kinds; `effect_add`/`effect_remove`/`effect_move` are commands:
  `crates/motioner_ui/src/inspect.rs:850-990` and
  `crates/motioner_ui/src/app/press.rs` (`press_segment`).

- `Editor` — session, transport, selection, undo, command execution (D-0024):
  `crates/motioner_ui/src/editor/mod.rs:113-660`.
- `labels` — the one display-label table for transform and content channels,
  shared by the inspector and the timeline so "Width"/"Height" cannot differ
  (D-0037): `crates/motioner_ui/src/labels.rs:1-116`.
- The inspector Keys section draws one tile per `Ease::ALL` preset with a
  curve thumbnail; a selected bezier key shows its four numbers (read-only;
  `set_ease(bezier(...))` edits them). `motion_blur_preview(on)` sets the
  editor flag and the top-bar indicator (D-0037).
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
  Gradient stops get one offset row and one colour row each, with the same`n  stopwatch and diamonds; the timeline labels them `Stop <n> offset/colour``n  (D-0030): `crates/motioner_ui/src/inspect.rs:391-500` and`n  `crates/motioner_ui/src/timeline_model.rs:237-320`.
- The inspector opens with a whole-node blend grid (seven tiles, four per
  row); the active mode fills with the accent and a click writes the `blend`
  override (D-0031): `crates/motioner_ui/src/inspect.rs` and
  `crates/motioner_ui/src/panels/inspector.rs`.
  dash, cap and join rows (`crates/motioner_ui/src/inspect.rs:405-475`).
- `bounds` — selection boxes and hit testing; a path item resolves through
  `motioner_render::path_bounds` and is picked against its fill or stroke
  (`crates/motioner_ui/src/bounds.rs:70-205`).

- Display names (D-0044): `Node::name`, the `name` override, the MDF
  `node.name` assignment and `motioner_dsl::default_name` for binding-derived
  labels (full loop path and function-call ordinals). The layers panel and
  `query` prefer the name and fall back to the kind label
  (`crates/motioner_ui/src/outline.rs:130-175`).
- Outline state: `outline::apply_visibility` collapses subtrees and applies
  `filter_layers` (name, kind, identity; ancestors kept); `solo`/`shy`/
  `show_shy` are preview-only and reach the painter through
  `LivePainter::paint_visible` (`crates/motioner_ui/src/editor/mod.rs`,
  `crates/motioner_render/src/lib.rs`).
- Graph editor (D-0044): `graph::GraphModel` maps one numeric channel to a
  plot; keys and bezier handles are hit-tested and every drag becomes one
  `ease` or `move_key_value` command; the curve is drawn as anti-aliased line
  segments sampled per screen column, and opening fits the key time/value
  range with padding (`crates/motioner_ui/src/graph.rs`,
  `crates/motioner_ui/src/panels/timeline.rs`).
- Performance: `timeline_model::layout_into` virtualises rows into a reusable
  buffer; visibility runs are cached per document generation; value texts,
  channel labels, identity, header/ruler strings and the graph caption are
  cached and rewritten in place. The optional `alloc-count` feature counts
  allocations on the UI thread only and always shows the reading (D-0044
  records the residual 9/frame caused by the engine's per-frame `GuiBatch`).
- Motion-blur preview (D-0044): `app::blur` renders through the same
  `FrameRenderer::render_preview_frame` accumulation as `still`/`render`,
  reuses one `Evaluator`/`RenderPlan`, rewrites its sprite texture in place
  when the size is unchanged, caches while paused and caps playback at four
  samples.

## Diagram

Not needed: the panels read pure models; the app turns input into commands.

## Relevant decisions

- D-0022 / D-0024 — overrides and the command layer.
- D-0025 / D-0026 — orphans and canvas view state.
- D-0028 — content channels.
- D-0029 — path content rows, bounds and picking.

## Documentation status

partial — last verified: 2026-11-01
