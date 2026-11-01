# motioner_ui

Motioner's editor window: a live preview of an `.motion` (MDF) file with a
layers panel, a canvas, a read-only inspector and a timeline.

## What it does

`motioner preview <file.motion>` opens a 1600×900 dark window:

- **Top bar** — app mark, file name with a state dot (green, pulsing after a
  reload; red on an error), composition pills (size, fps, length), a
  safe-area toggle and the canvas zoom.
- **Layers** (left) — one row per node, depth-first in document order, with a
  kind icon, indentation guides, a disclosure chevron for groups, a dim detail
  (`r24`, `96 × 96`) and eye/lock actions on hover (display only for now).
- **Canvas** — the composition letterboxed on a darker pasteboard with a
  frame border and a soft shadow. Click a shape to select it. The selection
  is framed with handles and a size label; the hovered layer gets a thin
  outline; `G` shows title/action safe areas.
- **Properties** (right) — the selected layer's transform, shape, text and
  appearance sampled at the current frame (a yellow diamond marks keyframed
  properties), or the composition summary when nothing is selected.
- **Timeline** — timecode and frame counter, icon transport (start, previous,
  play/pause, next, end, loop) with tooltips, a ruler with major and minor
  ticks, one track per layer aligned with a names column, lifetime bars with
  the animated span and keyframe diamonds, and the playhead with its frame.
- **Status bar** — diagnostics (`line:column message`) or "No issues", and
  evaluation time, UI fps and dropped frames.

Selection is shared by the layers list, the timeline names, the canvas and the
inspector; selecting reveals the row in both lists. Panels are resizable with
splitters (cursor changes on hover). Saving the file reloads it automatically;
a failed reload keeps the last good version and shows a banner, a red dot and
the diagnostic. A file that fails before any good load opens the window with an
error card. An empty composition shows a hint on the canvas.

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
| Esc | quit |
| wheel over the ruler or tracks | step frames (Shift: ten) |
| wheel over a list or the inspector | scroll |

## Determinism exception

Rendered pixels depend only on the document and the integer frame index. The
preview is the **only** place in Motioner where a wall clock is allowed: it
chooses *which* frame to show (`Transport::frame_at` converts elapsed time with
exact integer arithmetic, so NTSC rates do not drift) and animates chrome
(tooltip delay, reload pulse). The evaluator and renderer only see the integer
frame, so a paused window is a frozen frame and export stays bit-for-bit
deterministic. The canvas shows exactly the `FrameRenderer` output for the
transport frame (`tests/live_painter.rs` compares it with the golden).

## Structure

```text
src/
  lib.rs          crate docs and re-exports
  window.rs       run_preview, PreviewOptions, UiError
  app.rs          FerrousApp: state, input, per-frame draw order
  session.rs      PreviewSession: path, last good document, plan cache
  transport.rs    Transport and rational frame timing
  file_watch.rs   polling hot reload with debounce
  input.rs        pure key → action mapping
  outline.rs      flat layer rows (name, detail, depth, keyframes)
  inspect.rs      pure inspector model (sampled properties)
  bounds.rs       selection bounds and canvas hit testing
  timeline.rs     frame ↔ x and tick spacing
  layout.rs       pure window layout, splitters and hit testing
  theme.rs        design tokens (the only colour literals) and DPI metrics
  text.rs         baseline text, kerning, tracking, ellipsis
  paint.rs        Painter: quads, borders, shadows, gradients, polygons
  icons.rs        vector icon set
  widgets.rs      icon button, panel header, pill, field, tooltip
  panels/         top_bar, layers, canvas, inspector, timeline, status
```

Drawing order per frame: the canvas backdrop and the composition in the
engine's `Pure2D` pass (through `motioner_render::LivePainter`), then the
canvas overlays, the panels and the tooltip in the UI pass on top. The plan is
re-evaluated only when the frame or the document generation changes. Engine 2D
views are not used (they clip shapes but not text).

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

- Read-only: no property editing, no visibility/lock, no keyframe editing.
- Every layer lives for the whole composition (the model has no in/out
  points), so lifetime bars always span the timeline.
- One UI font weight; medium is simulated. Text layers whose family is not
  Roboto are drawn with Roboto (the inspector says so).
- Hover and tooltips are verified by hit-testing tests and code review; the
  screenshot tool cannot synthesise hover.
- The chrome does not use `ferrous_ui_core` widgets or GUIMaker yet
  (T-ARCH-0007).
