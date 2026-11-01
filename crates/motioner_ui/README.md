# motioner_ui

Motioner's first editor window: a live preview of an `.motion` (MDF) file.

## What it does

`motioner preview <file.motion>` opens a 1600×900 dark window with:

- **Top bar** — file name, canvas size and rational fps, frame / total, and
  non-drop-frame timecode `HH:MM:SS:FF`.
- **Scene outline** (left) — one row per node, depth-first in document order.
  Click a row to select it; rectangles and circles get a 1 px outline around
  their evaluated bounds in the viewport. Text has no bounds in the plan, so it
  selects only in the outline.
- **Viewport** — the composition letterboxed over a dark pasteboard. Items can
  overflow the composition edge, like After Effects.
- **Transport** — `go to start`, `-10`, `-1`, play/pause, `+1`, `+10`, `go to
  end`, loop. A scrubber below shows second/frame ticks and the playhead; drag
  it to scrub, or use the wheel.
- **Status bar** — MDF diagnostics in red with `line:column`, or `ok`; measured
  UI fps and dropped frame count.

Saving the file reloads it automatically (250 ms poll, 150 ms debounce). A
failed reload keeps the last good composition on screen and shows the
diagnostic. Starting with a file that has an MDF error opens the window with no
composition and the diagnostic visible, so fixing and saving recovers.

## Shortcuts

| Key | Action |
| --- | --- |
| Space | play / pause |
| Left / Right | step one frame (Shift: ten) |
| Home / End | first / last frame |
| L | toggle loop |
| R | reload now |
| Esc | quit |
| wheel over timeline | step frames (Shift: ten) |

## Determinism exception

Rendered pixels depend only on the document and the integer frame index. The
preview is the **only** place in Motioner where a wall clock is allowed, and
only to choose *which* frame to show: `Transport::frame_at` converts elapsed
time with exact integer arithmetic (`elapsed_ns × fps_num / (fps_den × 1e9)`),
so NTSC rates do not drift. The evaluator and renderer only ever see the
integer frame, so a paused window is a frozen frame and export stays
bit-for-bit deterministic.

## Architecture

- `session::PreviewSession` — path, last good `Document`, evaluator, cached
  `RenderPlan` (keyed by document generation + frame) and diagnostics.
- `transport::Transport` — `Stopped | Playing { anchor_frame, anchor_time } |
  Scrubbing`, rational fps, frame and duration, loop.
- `file_watch::FileWatch` — stat polling with debounce; no extra dependency.
- `timeline` — `frame_to_x` / `x_to_frame` and label spacing (≥ 60 px).
- `outline` — deterministic flat scene rows.
- `input` — pure `action_for` / `apply`.
- `window` — layout, drawing and the `FerrousApp` implementation.

The composition is drawn in the engine's Pure2D pass through
`motioner_render::LivePainter`; the chrome is drawn in the engine's UI pass,
which the renderer composites on top. The plan is re-evaluated only when the
frame index or the document generation changes.

2D **views** are deliberately not used: the multi-view Pure2D path clips shapes
only (not text) and had a viewport-origin bug for hard triangle edges. The
preview draws with the single main camera and applies `ViewFit::contain`.

## Known limitations

- No editing yet: the outline selects, but properties are not editable.
- No audio, no layers, no effects.
- Text bounds are not exposed by `RenderPlan`, so text selection has no viewport
  rectangle; font metrics or an item bounds field would be needed.
- The chrome is custom-drawn, not built on `ferrous_ui_core` widgets. The
  engine runner owns the `UiSystem` and does not expose widget `resolved_rect`
  or a way to re-run `configure_ui` after a resize, so a retained tree could not
  be used to obtain the viewport rect or to reflow absolute layouts. The
  `DrawContext`/`GuiBatch` immediate path is used instead. GUIMaker integration
  remains T-ARCH-0007 (D-0020).
- DPI: layout is computed every frame from the physical window size and the
  window scale factor; text sizes are scaled by the factor. Tested at 100 %,
  125 % and 150 % logical scale.
