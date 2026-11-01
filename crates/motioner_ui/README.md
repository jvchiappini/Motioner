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
  section lists overrides whose node no longer exists; each can be deleted.
- **Canvas** — the composition letterboxed on a darker pasteboard. Click a
  shape to select it; drag it to move, drag a corner to scale (Shift keeps the
  aspect ratio, Alt scales from the anchor), drag the grip above the top edge
  to rotate (Shift snaps to 15°). Snapping (on by default) aligns to the
  composition edges and centre and to other layers, drawing accent guides.
  Locked layers cannot be picked; hidden layers are not drawn or picked.
- **Properties** (right) — the selected layer's identity (key/id plus Copy),
  one row per **parameter** (Position X, Position Y, Scale X, Scale Y,
  Rotation, Anchor X, Anchor Y, Opacity) with its own stopwatch and keys, and
  content fields (size, radius, fill, stroke, width / text, size, colour,
  align).
  Numeric fields scrub horizontally (Shift ×10, Alt ×0.1) or type on click with
  a visible focus ring; colour fields accept `#RRGGBB[AA]`; align is a
  three-state control. A stopwatch starts or stops animation; the diamond shows
  whether the playhead is on a key. With timeline keys selected, a **Keys**
  section shows an Ease control that cycles through the MDF easings.
- **Timeline** — timecode and frame counter, transport, a ruler whose labels are
  centred on their tick and labelled to the composition end, one row per layer
  with a **visibility strip** (sampled from the document, with inherited parent
  opacity/scale/visibility), summary diamonds (tinted when several channels
  share the frame) and **hollow inherited diamonds** on children of animated
  groups. Selecting a layer expands one sub-row per **parameter** — Position X,
  Position Y, Scale X, Scale Y, Rotation, Anchor X, Anchor Y, Opacity, then the
  content fields — each with a stopwatch, the value at the playhead, the
  previous/next/add-key buttons, its own diamonds and a thin sampled polyline
  showing each segment's easing. X and Y keyframes are independent.
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
- Undo/redo is exact (`Ctrl+Z`, `Ctrl+Shift+Z`, `Ctrl+Y`); a drag is one step.
- The program is never edited by the editor. `motioner bake <file>` folds the
  overrides of nodes bound to a top-level variable into one generated block and
  rewrites the overrides file without them; unbound nodes are refused with a
  message and keep their overrides.

## Shortcuts

| Key | Action |
| --- | --- |
| Space | play / pause |
| Left / Right | previous / next frame (Shift: ten) |
| Up / Down | select previous / next layer |
| Home / End | first / last frame |
| L | toggle loop |
| G | toggle safe areas |
| R | reload now |
| K | add keys at the playhead (animated channels, else position) |
| Delete | delete the selected keys |
| U | show only animated properties in the timeline |
| Ctrl+Z | undo |
| Ctrl+Shift+Z, Ctrl+Y | redo |
| Esc | quit |
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
  app.rs            FerrousApp: state, input, drags, per-frame draw order
  session.rs        PreviewSession: program + overrides, debounced atomic writes
  edit.rs           pure EditCommand model and the bounded UndoStack
  text_edit.rs      pure single-line field editor (caret, selection, keys)
  transport.rs      Transport and rational frame timing
  file_watch.rs     polling hot reload with debounce and self-write reset
  input.rs          pure key → action mapping
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

Captures need no focus and no engine change:

```powershell
cargo build -j 2 -p motioner_app
scripts/ui_shot.ps1 -Out out/ui/shot.png                       # 1600x900, frame 120
scripts/ui_shot.ps1 -Out out/ui/720.png -Size 1280x720 -Scale 1.25 -Select 7
scripts/ui_shot.ps1 -File out/ui/broken.motion -Out out/ui/error.png
scripts/crop_zoom.ps1 -In out/ui/shot.png -Out out/ui/zoom.png -X 670 -Y 580 -W 270 -H 44
```

`ui_shot.ps1` runs `motioner preview <file> --frame N --paused --window-size
WxH [--ui-scale F] [--select ROW]` and captures the client area with
`capture_window.ps1` (Win32 `PrintWindow` with `PW_RENDERFULLCONTENT`, which
returns the DWM-composed GPU frame). `out/` is not committed.

## Known limitations

- Content properties are not animatable in the core, so their timeline rows are
  static.
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
