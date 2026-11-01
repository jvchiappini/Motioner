# motioner_ui

Motioner's editor window: a live preview of an `.motion` (MDF) file with a
layers panel, a canvas with gizmos, an editable inspector and an editable
timeline. Edits are **overrides** (D-0022) stored next to the program; the
`.motion` file only changes through `motioner bake`.

## What it does

`motioner preview <file.motion>` opens a 1600×900 dark window:

- **Top bar** — app mark, file name with a state dot (green, pulsing after a
  reload; red on an error), composition pills (size, fps, length), the snapping
  and safe-area toggles and the canvas zoom.
- **Layers** (left) — one row per node, depth-first in document order, with a
  kind icon, indentation guides, a chevron that expands the row's property
  timeline, a dim detail (`r24`, `96 × 96`), and eye/lock buttons. Double-click
  a name to rename the layer (a `name` override). A collapsible **Orphans**
  section lists overrides whose node no longer exists; each can be re-bound to
  its best suggested node or deleted.
- **Canvas** — the composition on a darker pasteboard, fitted by default.
  Ctrl+wheel zooms around the pointer (5 % to 3200 %), a middle drag or
  Space+drag pans, `F` or a click on the zoom pill fits. Everything that maps
  window ↔ composition (picks, bounds, gizmos, snapping, guides) goes through
  one `Viewport`, so overlays always sit on the drawn pixels. Click a shape to
  select it; drag it to move, drag a corner to scale (Shift keeps the aspect
  ratio, Alt scales from the anchor), drag the grip above the top edge to
  rotate (Shift snaps to 15°). Snapping (on by default) aligns to the
  composition edges and centre and to other layers, drawing accent guides.
  Locked layers cannot be picked; hidden layers are not drawn or picked.
- **Properties** (right) — the selected layer's identity (key/id plus Copy),
  one row per **parameter** (Position X, Position Y, Scale X, Scale Y,
  Rotation, Anchor X, Anchor Y, Opacity) with its own stopwatch and keys, and
  content fields (size, radius, fill, stroke, width / text, size, colour,
  align).
  Numeric fields scrub horizontally (Shift ×10, Alt ×0.1) or type on click with
  a visible focus ring; colour fields accept `#RRGGBB[AA]`; align is a
  three-state control. Each animatable row starts with a fixed key column: the
  stopwatch (accent when the channel is animated, grey when not) and the key
  diamond at the playhead. With timeline keys selected, a **Keys** section
  shows the **ease grid**: one tile per MDF ease with its curve; the ease
  shared by the selected keys is highlighted. The panel scrolls with the wheel
  and shows a scroll thumb.
- **Timeline** — timecode and frame counter, transport, a ruler whose labels are
  centred on their tick and labelled to the composition end, one row per layer
  with a **visibility strip** (a solid bar where the layer is drawn, an empty
  outline where it is not: opacity 0, scale 0, hidden, or inherited from a
  parent), summary diamonds and inherited parent diamonds. Selecting a layer
  expands one sub-row per **parameter** — Position X, Position Y, Scale X,
  Scale Y, Rotation, Anchor X, Anchor Y, Opacity, then the content fields — on
  a darker names column; only the layer row carries the selection colour. Each
  sub-row has a stopwatch, the value at the playhead, the previous/next/add-key
  buttons, its diamonds and a solid line between keys whose rise follows the
  segment's easing. X and Y keyframes are independent. Content fields are
  channels too (D-0028): `fill`, `stroke`, `stroke_width`, `width`, `height`,
  `radius`, `size` and `color` get the same stopwatch, diamonds and eased
  lines, and the inspector keys them at the playhead. The timeline is 320 px
  tall by default, its splitter is draggable and it scrolls to a new selection
  and to newly selected keys.

### Keyframe markers

One visual language everywhere (timeline layer rows, property rows, the
inspector and the ◆ buttons), drawn by `widgets::key_marker`:

| Marker | Meaning |
|---|---|
| filled gold ◆ | a key on this channel (on a layer row: on one of its channels) |
| filled gold ◆ with a dark core | layer row: several channels have a key on that frame |
| filled accent ◆ with an accent ring | a selected key |
| hollow grey ◇ (dark inside) | a parent group's key (moves this layer, not selectable), or no key of this channel at the playhead |

Own keys of hidden or locked layers are drawn grey.
- **Status bar** — errors, then override warnings in the warning colour, then
  evaluation time, UI fps and dropped frames.

Selection is shared by the layers list, the timeline, the canvas and the
inspector. Panels are resizable with splitters. Saving the program or the
overrides file reloads them; the editor's own debounced write is recognised and
does not trigger a reload. A failed reload keeps the last good version.

## Editing and persistence

- Every edit becomes one or more `override(...)` statements in
  `<name>.overrides.motion` (D-0022), written 300 ms after the last change
  through a temp file and a rename.
- Every action is a `Command` run by the window-free `Editor` (D-0024); the
  same commands are available to agents through `motioner edit` and
  `motioner ctl` (`docs/agents.md`).
- Undo/redo is exact (`Ctrl+Z`, `Ctrl+Shift+Z`, `Ctrl+Y`); a drag is one step
  (an editor gesture).
- Replacing a key keeps its ease; animated vectors are always keyed per axis
  (`pos.x`, `pos.y`).
- The program is never edited by the editor. `motioner bake <file>` edits a
  private literal in place (a constructor argument, an assignment or a
  `.key()` value/ease) when the run evaluated it exactly once, and otherwise
  folds the overrides of nodes bound to a top-level variable into one generated
  block; shared literals and unbound node values are refused with a message and
  keep their overrides. When an in-place edit shifts byte offsets, bake re-keys
  every override that stays in the same write (D-0025, D-0027).

## Shortcuts

| Key | Action |
| --- | --- |
| Space | play / pause (on release; Space+drag pans instead) |
| Left / Right | previous / next frame (Shift: ten) |
| Up / Down | select previous / next layer |
| Home / End | first / last frame |
| L | toggle loop |
| G | toggle safe areas |
| R | reload now |
| K | add keys at the playhead (animated channels, else position) |
| Delete | delete the selected keys |
| U | show only animated properties in the timeline |
| F | fit the composition to the canvas |
| 1 | zoom to 100 % |
| Ctrl+Z | undo |
| Ctrl+Shift+Z, Ctrl+Y | redo |
| Esc | quit |
| Ctrl+wheel on the canvas | zoom around the pointer |
| middle drag / Space+drag on the canvas | pan |
| click the zoom pill (top right) | fit the composition |
| wheel over the ruler or tracks | step frames (Shift: ten) |
| wheel over a list or the inspector | scroll |
| drag a numeric label or field | scrub (Shift ×10, Alt ×0.1) |
| click a field | type; Enter commits, Esc cancels |
| chevron on a layer row | expand/collapse its properties (Alt: all) |
| drag a diamond | retime the selected keys (frame delta pill) |
| drag on empty track area | box-select keys |

While a field is being edited, global shortcuts are suspended so typing never
triggers playback.

## Determinism exception

Rendered pixels depend only on the document (program plus overrides) and the
integer frame index. The preview is the **only** place in Motioner where a wall
clock is allowed: it chooses *which* frame to show (`Transport::frame_at`
converts elapsed time with exact integer arithmetic, so NTSC rates do not
drift), animates chrome (tooltip delay, reload pulse) and schedules the
debounced overrides write. A paused window is a frozen frame and export stays
bit-for-bit deterministic. The canvas shows exactly the `FrameRenderer` output
for the transport frame (`tests/live_painter.rs` compares it with the golden).

## Structure

```text
src/
  lib.rs            crate docs and re-exports
  window.rs         run_preview, PreviewOptions, UiError
  app/              the window: mod (frame loop, keys, control polling), pointer
                    (hit testing), press, drag, canvas (painting and picks)
  command/          Command, CommandKind, errors; text form, parser, examples
  editor/           Editor: window-free state and the one command executor;
                    channels, view, resolve (commands → edits), fields, bake,
                    rebind (orphan suggestions), query (JSON)
  control.rs        live control endpoint (127.0.0.1, token) and ctl client
  capture.rs        in-app screenshots through the engine hook
  json.rs           tiny JSON writer for tool output
  session.rs        PreviewSession: program + overrides, debounced atomic writes
  edit.rs           pure EditCommand model and the bounded UndoStack
  viewport.rs       pure canvas mapping: fit, zoom, pan, window ↔ composition
  text_edit.rs      pure single-line field editor (caret, selection, keys)
  transport.rs      Transport and rational frame timing
  file_watch.rs     polling hot reload with debounce and self-write reset
  input.rs          pure key → Command mapping
  outline.rs        flat layer rows (name, detail, depth, keyframes)
  inspect.rs        editable inspector model and its pure layout
  gizmo.rs          handle hit testing, move/scale/rotate maths and snapping
  timeline_model.rs timeline rows, visibility strips, inherited keys, layout
  bounds.rs         selection bounds and canvas hit testing
  timeline.rs       frame ↔ x and tick spacing
  layout.rs         pure window layout, splitters and hit testing
  theme.rs          design tokens (the only colour literals) and DPI metrics
  text.rs           baseline text, kerning, tracking, ellipsis
  paint.rs          Painter: quads, borders, shadows, gradients, polygons, diamonds
  icons.rs          vector icon set
  widgets.rs        icon button, panel header, pill, tooltip
  clipboard.rs      "Copy" through PowerShell Set-Clipboard (Windows)
  panels/           top_bar, layers, canvas, inspector, timeline, status
```

Drawing order per frame: the canvas backdrop and the composition in the
engine's `Pure2D` pass (through `motioner_render::LivePainter`), then the canvas
overlays, the panels and the tooltip in the UI pass on top. The plan is
re-evaluated only when the frame or the document generation changes.

## Theme

`theme.rs` holds every colour as sRGB hex and linearises it once with
`motioner_render::srgb_to_linear`. The engine's `Color::hex` does not
linearise, which is why the first window looked washed out; never use it for
UI colours. Tokens: an 11-step cool-neutral ramp (pasteboard darkest, panels
lighter), one accent (`#5B8CFF`), danger, warning, success and keyframe
colours, a 4 px grid, radii, and an 11/12/13/15 px type scale. `Metrics` scales
logical sizes by the DPI factor and rounds to whole pixels. See D-0021 for why
the chrome is an immediate painter rather than engine widgets.

## Screenshots

The editor captures itself: the engine copies the frame it presents
(`AppContext::request_screenshot`, D-0023), so captures need no focus and
match the screen exactly (checked against a `PrintWindow` capture: identical
apart from the live fps readout).

```powershell
cargo build -j 2 -p motioner_app
scripts/ui_shot.ps1 -Out out/ui/shot.png                       # 1600x900, frame 120
scripts/ui_shot.ps1 -Out out/ui/720.png -Size 1280x720 -Scale 1.25 -Select 7
scripts/ui_shot.ps1 -Out out/ui/keys.png -Commands 'select("@288")','select_keys([("@288", "pos.x", 0.8)])'
scripts/crop_zoom.ps1 -In out/ui/shot.png -Out out/ui/zoom.png -X 670 -Y 580 -W 270 -H 44
```

Without `-Commands`, `ui_shot.ps1` runs `motioner preview <file> --frame N
--paused --window-size WxH --exit-after-frames 20 --screenshot <png>`. With
`-Commands` it opens a control endpoint, sends the commands with `motioner
ctl`, then `screenshot(...)` and `quit()`. The engine only honours a
screenshot asked for while drawing, so the request is made in `draw_ui`; an
adapter whose surface lacks `COPY_SRC` makes the capture fail with a clear
error after 30 frames. `out/` is not committed.

## Known limitations

- Content properties are animatable channels (D-0028), so their timeline rows
  have keys like transform rows; `text` and `align` stay discrete.
- A rotation gizmo rotates around the selection's centre by the pointer angle;
  the model's anchor is not moved.
- Snapping to other layers uses shape bounds only (text bounds need the
  renderer at drag time).
- `.id()` names cannot be created from the UI yet: the inspector shows the key
  and copies it so the user adds `.id("...")` in the program (D-0022).
- One UI font weight; medium is simulated. Text layers whose family is not
  Roboto are drawn with Roboto (the inspector says so).
- Hover and tooltips are verified by hit-testing tests and code review; the
  screenshot tool cannot synthesise hover.
- The chrome does not use `ferrous_ui_core` widgets or GUIMaker yet
  (T-ARCH-0007).
